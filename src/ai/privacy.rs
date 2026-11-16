//! Proxy de confidentialité : [PrivAiTe](https://github.com/crp4222/PrivAiTe) (logiciel libre, BSD-3),
//! un proxy local qui remplace les noms, e-mails, numéros et secrets par des étiquettes (`<PERSON_1>`)
//! avant l'envoi au fournisseur d'IA, puis remet les vraies valeurs dans la réponse.
//!
//! Installation en un clic, seulement pour qui la demande (environ 1,5 Go) : `uv` (vérifié par son
//! empreinte) installe un Python et PrivAiTe dans `%LOCALAPPDATA%\KitePDF\privaite`, sans toucher au
//! reste de l'ordinateur ; les modèles de détection sont téléchargés tout de suite.
//!
//! Le proxy démarre au premier envoi, écoute seulement sur `127.0.0.1` avec un mot de passe tiré au
//! hasard, et reçoit les clés API par son environnement (jamais sur le disque). Il s'arrête avec Kite,
//! même si Kite plante (objet tâche de Windows).

use crate::i18n::tr;
use crate::trf;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Option masquée dans la 1.3 : la détection laisse passer des données (adresse incomplète,
/// numéro de contrat…), et le proxy est lourd (environ 1,2 Go de mémoire, une minute de démarrage).
/// Tant que c'est `false`, rien de PrivAiTe n'apparaît ni ne s'active, même avec `ai_privacy=1`.
pub const VISIBLE: bool = false;

/// Mode confidentialité réellement actif.
pub fn active(prefs: &crate::settings::AiPrefs) -> bool {
    VISIBLE && prefs.privacy_proxy
}

pub const PRIVAITE_VERSION: &str = "0.8.2";
const UV_VERSION: &str = "0.13.6";
const UV_URL: &str = "https://github.com/astral-sh/uv/releases/download/0.13.6/uv-x86_64-pc-windows-msvc.zip";
/// Empreinte de l'archive publiée par Astral (champ `digest` de la release GitHub).
const UV_ZIP_SHA256: &str = "a2a8820d5eea32f08ef467ef3b7f66883a9b076d5737a2fc2fbb5f458d1bb50f";
const PYTHON_VERSION: &str = "3.12";
pub const PROJECT_URL: &str = "https://github.com/crp4222/PrivAiTe";
/// Taille approximative une fois installé (Python, dépendances, modèles).
pub const DISK_SIZE: &str = "2";

pub fn dir() -> PathBuf {
    crate::settings::local_data_dir().join("privaite")
}

fn uv() -> PathBuf {
    dir().join("uv.exe")
}

fn python() -> PathBuf {
    dir().join("venv").join("Scripts").join("python.exe")
}

fn marker() -> PathBuf {
    dir().join("installed.txt")
}

pub fn install_log() -> PathBuf {
    dir().join("install.log")
}

pub fn proxy_log() -> PathBuf {
    dir().join("proxy.log")
}

pub fn is_installed() -> bool {
    marker().is_file() && python().is_file()
}

/// Taille installée, notée à la fin de l'installation.
pub fn installed_size() -> Option<String> {
    std::fs::read_to_string(marker()).ok()?.lines().find_map(|l| l.strip_prefix("size=").map(str::to_string))
}

// ---------------------------------------------------------------------------------------------
// Processus enfants : sans fenêtre de console, tués avec Kite (objet tâche de Windows)
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimit {
        per_process_user_time: i64,
        per_job_user_time: i64,
        limit_flags: u32,
        minimum_working_set: usize,
        maximum_working_set: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io: [u64; 6],
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(job: *mut c_void, class: i32, info: *mut c_void, length: u32) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    }

    /// Objet tâche partagé : fermé par Windows à la fin de Kite, il emporte les processus qu'il contient.
    fn job() -> Option<usize> {
        static JOB: OnceLock<Option<usize>> = OnceLock::new();
        *JOB.get_or_init(|| {
            // SAFETY : appels Win32 avec des structures correctement dimensionnées.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                let mut info = ExtendedLimit::default();
                info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    job,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    &mut info as *mut _ as *mut c_void,
                    std::mem::size_of::<ExtendedLimit>() as u32,
                );
                (ok != 0).then_some(job as usize)
            }
        })
    }

    pub fn adopt(child: &std::process::Child) {
        use std::os::windows::io::AsRawHandle;
        if let Some(job) = job() {
            // SAFETY : le handle du processus est valide tant que `child` existe.
            unsafe {
                AssignProcessToJobObject(job as *mut c_void, child.as_raw_handle() as *mut c_void);
            }
        }
    }
}

#[cfg(not(windows))]
mod job {
    pub fn adopt(_child: &std::process::Child) {}
}

fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    // Tout reste dans le dossier de Kite ; la configuration uv de l'utilisateur est ignorée.
    command
        .env("UV_PYTHON_INSTALL_DIR", dir().join("python"))
        .env("UV_CACHE_DIR", dir().join("cache"))
        .env("UV_NO_CONFIG", "1")
        .env("HF_HOME", dir().join("huggingface"))
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
        .env("PYTHONUTF8", "1");
    command
}

// ---------------------------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum InstallState {
    Idle,
    /// Étape (1 à 4) et ce qui se passe
    Running { step: u8, label: String, detail: String },
    Failed(String),
}

static INSTALL: Mutex<InstallState> = Mutex::new(InstallState::Idle);
static INSTALL_CANCEL: AtomicBool = AtomicBool::new(false);
static INSTALL_CHILD: Mutex<Option<Child>> = Mutex::new(None);

pub fn install_state() -> InstallState {
    INSTALL.lock().map(|s| s.clone()).unwrap_or(InstallState::Idle)
}

pub const INSTALL_STEPS: u8 = 4;

fn set_state(state: InstallState, ctx: &egui::Context) {
    if let Ok(mut s) = INSTALL.lock() {
        *s = state;
    }
    ctx.request_repaint();
}

fn set_detail(detail: &str, ctx: &egui::Context) {
    if let Ok(mut s) = INSTALL.lock() {
        if let InstallState::Running { detail: d, .. } = &mut *s {
            *d = detail.chars().take(160).collect();
        }
    }
    ctx.request_repaint();
}

/// Lance l'installation dans un fil à part (rien si elle est déjà en cours).
pub fn start_install(ui_lang_en: bool, ctx: egui::Context) {
    {
        let Ok(mut state) = INSTALL.lock() else { return };
        if matches!(*state, InstallState::Running { .. }) {
            return;
        }
        *state = InstallState::Running { step: 1, label: String::new(), detail: String::new() };
    }
    INSTALL_CANCEL.store(false, Ordering::Relaxed);
    let _ = std::thread::Builder::new().name("kite-privaite-install".into()).spawn(move || {
        let result = install(ui_lang_en, &ctx);
        match result {
            Ok(()) => set_state(InstallState::Idle, &ctx),
            Err(e) if INSTALL_CANCEL.load(Ordering::Relaxed) => {
                let _ = e;
                set_state(InstallState::Idle, &ctx);
            }
            Err(e) => set_state(InstallState::Failed(e), &ctx),
        }
    });
}

pub fn cancel_install() {
    INSTALL_CANCEL.store(true, Ordering::Relaxed);
    if let Ok(mut child) = INSTALL_CHILD.lock() {
        if let Some(child) = child.as_mut() {
            let _ = child.kill();
        }
    }
}

/// Supprime l'installation (le proxy est arrêté d'abord).
pub fn uninstall() -> Result<(), String> {
    shutdown();
    if dir().exists() {
        std::fs::remove_dir_all(dir()).map_err(|e| e.to_string())?;
    }
    if let Ok(mut s) = INSTALL.lock() {
        *s = InstallState::Idle;
    }
    Ok(())
}

fn log_line(text: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(install_log()) {
        let _ = writeln!(file, "{text}");
    }
}

/// Lance `command`, recopie sa sortie dans le journal et montre la dernière ligne à l'utilisateur.
fn run_logged(mut command: Command, ctx: &egui::Context) -> Result<(), String> {
    log_line(&format!("$ {command:?}"));
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    job::adopt(&child);
    let stderr = child.stderr.take();
    let stdout = child.stdout.take();
    if let Ok(mut slot) = INSTALL_CHILD.lock() {
        *slot = Some(child);
    }
    let ctx_err = ctx.clone();
    let reader = std::thread::spawn(move || {
        if let Some(err) = stderr {
            for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
                log_line(&line);
                if !line.trim().is_empty() {
                    set_detail(line.trim(), &ctx_err);
                }
            }
        }
    });
    if let Some(out) = stdout {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            log_line(&line);
        }
    }
    let _ = reader.join();
    let status = INSTALL_CHILD
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .map(|mut child| child.wait())
        .transpose()
        .map_err(|e| e.to_string())?;
    if INSTALL_CANCEL.load(Ordering::Relaxed) {
        return Err(tr("Installation annulée.", "Installation cancelled.").into());
    }
    match status {
        Some(status) if status.success() => Ok(()),
        Some(status) => Err(trf!(
            "La commande a échoué ({}). Détails dans le journal d'installation.",
            "The command failed ({}). Details in the installation log.",
            status
        )),
        None => Err(tr("Commande interrompue.", "Command interrupted.").into()),
    }
}

fn install(ui_lang_en: bool, ctx: &egui::Context) -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(marker());
    log_line(&format!("--- Installation de PrivAiTe {PRIVAITE_VERSION} ({})", chrono::Local::now().to_rfc3339()));
    let step = |n: u8, label: &str| {
        set_state(InstallState::Running { step: n, label: label.into(), detail: String::new() }, ctx);
    };

    // 1. uv, vérifié par son empreinte.
    step(1, tr("Téléchargement de l'installateur uv (15 Mo)…", "Downloading the uv installer (15 MB)…"));
    if !uv().is_file() {
        let zip = dir().join("uv.zip.download");
        let mut progress = |received: u64, total: Option<u64>| {
            if let Some(total) = total.filter(|t| *t > 0) {
                set_detail(&format!("{} %", received * 100 / total), ctx);
            }
        };
        crate::deps::fetch(UV_URL, &zip, &mut progress).map_err(|e| format!("{e:#}"))?;
        let found = crate::deps::sha256_file(&zip).map_err(|e| e.to_string())?;
        if !found.eq_ignore_ascii_case(UV_ZIP_SHA256) {
            let _ = std::fs::remove_file(&zip);
            return Err(trf!(
                "L'archive de uv {} n'a pas l'empreinte attendue : installation arrêtée par sécurité.",
                "The uv {} archive does not have the expected fingerprint: installation stopped for safety.",
                UV_VERSION
            ));
        }
        let staged = dir().join("uv.exe.part");
        crate::deps::extract_member(&zip, UV_URL, "uv.exe", &staged).map_err(|e| format!("{e:#}"))?;
        std::fs::rename(&staged, uv()).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&zip);
    }
    if INSTALL_CANCEL.load(Ordering::Relaxed) {
        return Err(String::new());
    }

    // 2. Python, dans le dossier de Kite.
    step(2, tr("Installation de Python (environ 40 Mo)…", "Installing Python (about 40 MB)…"));
    if !python().is_file() {
        let mut venv = command(&uv());
        venv.args(["venv", "--seed", "--python", PYTHON_VERSION, "--python-preference", "only-managed"])
            .arg(dir().join("venv"));
        run_logged(venv, ctx)?;
    }

    // 3. PrivAiTe et ses dépendances.
    step(3, tr(
        "Installation de PrivAiTe et de ses dépendances (environ 700 Mo)…",
        "Installing PrivAiTe and its dependencies (about 700 MB)…",
    ));
    let mut pip = command(&uv());
    pip.args(["pip", "install", "--python"])
        .arg(python())
        .arg(format!("privaite=={PRIVAITE_VERSION}"));
    run_logged(pip, ctx)?;

    // 4. Premier démarrage : PrivAiTe télécharge ses modèles de détection.
    step(4, tr(
        "Téléchargement des modèles de détection (environ 600 Mo)…",
        "Downloading the detection models (about 600 MB)…",
    ));
    let warmup = vec![ProxyEntry {
        alias: "kite-warmup".into(),
        litellm_model: "openai/warmup".into(),
        api_base: Some("http://127.0.0.1:9".into()),
        key: None,
        forward_params: vec![],
    }];
    let running = start_proxy(&warmup, ui_lang_en, Duration::from_secs(20 * 60), &|detail| set_detail(&detail, ctx));
    match running {
        Ok(mut proxy) => {
            let _ = proxy.child.kill();
            let _ = proxy.child.wait();
        }
        Err(e) => return Err(e),
    }
    if INSTALL_CANCEL.load(Ordering::Relaxed) {
        return Err(String::new());
    }
    let size = folder_size(&dir());
    std::fs::write(
        marker(),
        format!("privaite={PRIVAITE_VERSION}\nuv={UV_VERSION}\nsize={:.1}\n", size as f64 / 1e9),
    )
    .map_err(|e| e.to_string())?;
    log_line("--- Installation terminée");
    Ok(())
}

fn folder_size(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => folder_size(&e.path()),
                    _ => e.metadata().map(|m| m.len()).unwrap_or(0),
                })
                .sum()
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Proxy
// ---------------------------------------------------------------------------------------------

/// Un modèle exposé par le proxy, sous l'alias `kite-<fournisseur>`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyEntry {
    pub alias: String,
    /// Modèle au sens de LiteLLM : `openai/<modèle>` (API compatible OpenAI) ou `anthropic/<modèle>`
    pub litellm_model: String,
    pub api_base: Option<String>,
    /// Clé API, donnée au proxy par une variable d'environnement
    pub key: Option<String>,
    /// Champs propres au fournisseur transmis tels quels (non anonymisés : réglages, pas de texte)
    pub forward_params: Vec<&'static str>,
}

impl ProxyEntry {
    fn key_env(&self) -> String {
        format!("KITE_KEY_{}", self.alias.trim_start_matches("kite-").to_ascii_uppercase().replace('-', "_"))
    }
}

/// Adresse du proxy en marche et mot de passe à lui présenter.
#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    pub base_url: String,
    pub token: String,
}

struct Running {
    child: Child,
    endpoint: Endpoint,
    config_hash: u64,
    /// Dernière demande au proxy : il s'arrête après `IDLE_STOP` sans question.
    last_used: Instant,
}

static PROXY: Mutex<Option<Running>> = Mutex::new(None);

/// Le proxy garde environ 1,2 Go de mémoire : il s'arrête après 15 minutes sans question
/// (la question suivante attend son redémarrage).
const IDLE_STOP: Duration = Duration::from_secs(15 * 60);

/// Délai d'inactivité (`KITE_PRIVAITE_IDLE_SECS` le raccourcit pour les essais).
fn idle_stop() -> Duration {
    std::env::var("KITE_PRIVAITE_IDLE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(IDLE_STOP)
}

/// Demandes en cours à travers le proxy : il ne s'arrête jamais pendant l'une d'elles
/// (l'analyse d'un long document peut prendre une demi-minute).
static IN_USE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Tenu pendant toute une demande au proxy ; à la fin, le délai d'inactivité repart de zéro.
pub struct InUse(());

pub fn in_use() -> InUse {
    IN_USE.fetch_add(1, Ordering::SeqCst);
    InUse(())
}

impl Drop for InUse {
    fn drop(&mut self) {
        if let Ok(mut slot) = PROXY.lock() {
            if let Some(running) = slot.as_mut() {
                running.last_used = Instant::now();
            }
        }
        IN_USE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Surveille l'inactivité du proxy (un seul fil pour toute la session).
fn watch_idle() {
    static WATCHER: std::sync::Once = std::sync::Once::new();
    WATCHER.call_once(|| {
        let _ = std::thread::Builder::new().name("kite-privaite-idle".into()).spawn(|| loop {
            std::thread::sleep(Duration::from_secs(30).min(idle_stop()));
            if let Ok(mut slot) = PROXY.lock() {
                if IN_USE.load(Ordering::SeqCst) == 0 && slot.as_ref().is_some_and(|r| r.last_used.elapsed() >= idle_stop()) {
                    if let Some(mut running) = slot.take() {
                        let _ = running.child.kill();
                        let _ = running.child.wait();
                    }
                }
            }
        });
    });
}

fn yaml_string(text: &str) -> String {
    // Une chaîne JSON est une chaîne YAML valide (guillemets et échappements compris).
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

/// Configuration de PrivAiTe (aucune clé : elles arrivent par l'environnement).
pub fn config_yaml(entries: &[ProxyEntry], port: u16, english: bool) -> String {
    let mut providers = String::new();
    for entry in entries {
        providers.push_str(&format!("  - model_name: {}\n    litellm_params:\n      model: {}\n", yaml_string(&entry.alias), yaml_string(&entry.litellm_model)));
        if let Some(base) = &entry.api_base {
            providers.push_str(&format!("      api_base: {}\n", yaml_string(base)));
        }
        let key = if entry.key.is_some() { format!("\"${{{}}}\"", entry.key_env()) } else { "\"none\"".into() };
        providers.push_str(&format!("      api_key: {key}\n"));
        if !entry.forward_params.is_empty() {
            let list: Vec<String> = entry.forward_params.iter().map(|p| yaml_string(p)).collect();
            providers.push_str(&format!("    forward_params: [{}]\n", list.join(", ")));
        }
    }
    // spaCy sans modèle de langue (`nlp_engine: blank`, comme le préréglage « ultrafast » de PrivAiTe) :
    // les noms viennent du modèle MiniLM, multilingue. Mesuré sur un article de 23 pages : 300 Mo de
    // mémoire en moins, analyse 30 % plus rapide, rien de moins détecté.
    let language = if english { "en" } else { "fr" };
    // Une ligne par élément : l'indentation YAML compte (une suite de chaîne en fin de ligne la supprimerait).
    let lines = [
        "server:".to_string(),
        "  host: \"127.0.0.1\"".into(),
        format!("  port: {port}"),
        "  workers: 1".into(),
        "  log_level: \"warning\"".into(),
        "auth:".into(),
        "  enabled: true".into(),
        "providers:".into(),
        providers.trim_end().to_string(),
        "pii:".into(),
        "  enabled: true".into(),
        "  preset: \"minilm\"".into(),
        "  on_error: \"block\"".into(),
        "  detectors:".into(),
        "    presidio:".into(),
        format!("      languages: [\"{language}\"]"),
        "      nlp_engine: \"blank\"".into(),
        "  detection_cache:".into(),
        "    enabled: true".into(),
        "  anonymization:".into(),
        "    method: \"placeholder\"".into(),
        "    entity_overrides:".into(),
        "      CREDIT_CARD:".into(),
        "        method: \"mask\"".into(),
        "        masking_char: \"*\"".into(),
        "      SECRET:".into(),
        "        method: \"redact\"".into(),
        "logging:".into(),
        "  format: \"json\"".into(),
        // Journal détaillé pour le diagnostic (`KITE_AI_DEBUG=1`), discret sinon.
        format!("  level: \"{}\"", if std::env::var_os("KITE_AI_DEBUG").is_some() { "debug" } else { "warning" }),
    ];
    lines.join("\n") + "\n"
}

fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for round in 0..2u64 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(round);
        hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        hasher.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out
}

fn free_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

fn last_log_lines(path: &Path, count: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// Démarre PrivAiTe et attend qu'il soit prêt (`GET /ready`).
fn start_proxy(entries: &[ProxyEntry], english: bool, patience: Duration, status: &dyn Fn(String)) -> Result<Running, String> {
    let port = free_port()?;
    let token = random_token();
    let config_path = dir().join("kite-privaite.yaml");
    std::fs::write(&config_path, config_yaml(entries, port, english)).map_err(|e| e.to_string())?;
    let log = std::fs::File::create(proxy_log()).map_err(|e| e.to_string())?;
    let mut cmd = command(&python());
    cmd.args(["-m", "privaite", "--config"])
        .arg(&config_path)
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .env("PRIVAITE_API_KEYS", &token)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log));
    for entry in entries {
        if let Some(key) = &entry.key {
            cmd.env(entry.key_env(), key);
        }
    }
    let mut child = cmd.spawn().map_err(|e| trf!("PrivAiTe n'a pas pu démarrer : {}", "PrivAiTe could not start: {}", e))?;
    job::adopt(&child);
    let endpoint = Endpoint { base_url: format!("http://127.0.0.1:{port}/v1"), token };
    let ready_url = format!("http://127.0.0.1:{port}/ready");
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).build();
    let started = Instant::now();
    loop {
        if INSTALL_CANCEL.load(Ordering::Relaxed) && matches!(install_state(), InstallState::Running { .. }) {
            let _ = child.kill();
            return Err(tr("Installation annulée.", "Installation cancelled.").into());
        }
        if let Ok(Some(exit)) = child.try_wait() {
            return Err(trf!(
                "PrivAiTe s'est arrêté au démarrage ({}) :\n{}",
                "PrivAiTe stopped while starting ({}):\n{}",
                exit,
                last_log_lines(&proxy_log(), 6)
            ));
        }
        if agent.get(&ready_url).call().is_ok() {
            return Ok(Running { child, endpoint, config_hash: 0, last_used: Instant::now() });
        }
        if started.elapsed() > patience {
            let _ = child.kill();
            return Err(trf!(
                "PrivAiTe n'est pas prêt après {} s :\n{}",
                "PrivAiTe is not ready after {} s:\n{}",
                patience.as_secs(),
                last_log_lines(&proxy_log(), 6)
            ));
        }
        let elapsed = started.elapsed().as_secs();
        if elapsed >= 5 {
            // Seules les lignes parlantes (téléchargements, chargement des modèles) sont montrées.
            let last = last_log_lines(&proxy_log(), 1);
            let useful = ["Download", "Collecting", "Installing", "Loading", "%"].iter().any(|k| last.contains(k));
            status(if useful { format!("{elapsed} s · {}", last.chars().take(120).collect::<String>()) } else { format!("{elapsed} s") });
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}

fn config_hash(entries: &[ProxyEntry], english: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    english.hash(&mut hasher);
    for e in entries {
        (&e.alias, &e.litellm_model, &e.api_base, &e.key, &e.forward_params).hash(&mut hasher);
    }
    hasher.finish()
}

/// Proxy prêt pour ces modèles : celui qui tourne s'il convient, sinon un nouveau (premier démarrage :
/// jusqu'à une minute, le temps de charger les modèles de détection).
pub fn ensure_running(entries: &[ProxyEntry], english: bool, status: &dyn Fn(String)) -> Result<Endpoint, String> {
    if !is_installed() {
        return Err(tr(
            "Le proxy de confidentialité n'est pas installé (Réglages › Assistant IA).",
            "The privacy proxy is not installed (Settings › AI assistant).",
        )
        .into());
    }
    let hash = config_hash(entries, english);
    let mut slot = PROXY.lock().map_err(|e| e.to_string())?;
    if let Some(running) = slot.as_mut() {
        if running.config_hash == hash && matches!(running.child.try_wait(), Ok(None)) {
            running.last_used = Instant::now();
            return Ok(running.endpoint.clone());
        }
        let _ = running.child.kill();
        let _ = running.child.wait();
        *slot = None;
    }
    let mut running = start_proxy(entries, english, Duration::from_secs(180), status)?;
    running.config_hash = hash;
    running.last_used = Instant::now();
    let endpoint = running.endpoint.clone();
    *slot = Some(running);
    watch_idle();
    Ok(endpoint)
}

/// Arrête le proxy (fermeture de Kite, désinstallation).
pub fn shutdown() {
    if let Ok(mut slot) = PROXY.lock() {
        if let Some(mut running) = slot.take() {
            let _ = running.child.kill();
            let _ = running.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_keeps_keys_out_and_listens_locally() {
        let entries = vec![
            ProxyEntry {
                alias: "kite-gemini".into(),
                litellm_model: "openai/gemini-flash-lite-latest".into(),
                api_base: Some("https://generativelanguage.googleapis.com/v1beta/openai".into()),
                key: Some("SECRET-KEY-VALUE".into()),
                forward_params: vec![],
            },
            ProxyEntry {
                alias: "kite-openrouter".into(),
                litellm_model: "openai/nvidia/nemotron-3.5-lightning:free".into(),
                api_base: Some("https://openrouter.ai/api/v1".into()),
                key: Some("other".into()),
                forward_params: vec!["reasoning", "models"],
            },
        ];
        let yaml = config_yaml(&entries, 8452, false);
        assert!(!yaml.contains("SECRET-KEY-VALUE"));
        assert!(yaml.contains("api_key: \"${KITE_KEY_GEMINI}\""));
        assert!(yaml.contains("host: \"127.0.0.1\""));
        assert!(yaml.contains("preset: \"minilm\""));
        assert!(yaml.contains("forward_params: [\"reasoning\", \"models\"]"));
        assert!(yaml.contains("\n      languages: [\"fr\"]\n      nlp_engine: \"blank\"\n"));
        assert!(yaml.contains("\n  detection_cache:\n    enabled: true\n"));
        assert!(yaml.contains("\n      CREDIT_CARD:\n        method: \"mask\"\n"));
        assert!(yaml.contains("\n  - model_name: \"kite-gemini\"\n    litellm_params:\n      model: "));
        assert_eq!(entries[1].key_env(), "KITE_KEY_OPENROUTER");
        // Un changement de modèle redémarre le proxy, pas une question de plus.
        assert_ne!(config_hash(&entries, false), config_hash(&entries[..1], false));
        assert_eq!(config_hash(&entries, false), config_hash(&entries.clone(), false));
        assert_eq!(random_token().len(), 32);
        assert_ne!(random_token(), random_token());
    }
}
