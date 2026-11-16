//! Requêtes vers le fournisseur d'IA, dans un fil à part : la réponse arrive morceau par morceau
//! (flux SSE) et l'interface l'affiche au fur et à mesure.
//!
//! Relais : si le fournisseur choisi ne répond pas avant le premier mot (limite gratuite atteinte,
//! panne, modèle introuvable...), la question part chez le fournisseur suivant dont la clé est rangée,
//! les plus rapides d'abord. L'utilisateur voit lequel a répondu.

use super::providers;
use super::sources::{self, SourceSpec};
use super::{parse_sse_line, ChatRequest, StreamEvent, Turn};
use crate::i18n::tr;
use crate::settings::AiProvider;
use crate::trf;
use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Nouvelles d'une requête en cours, lues par l'interface à chaque image.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    /// Étape en cours (« Lecture des documents… »)
    Status(String),
    /// Remarque à garder sous la réponse (document réduit, relais vers un autre fournisseur)
    Note(String),
    /// Fournisseur et modèle qui répondent, avec le temps avant le premier mot
    Via(String),
    Text(String),
    Done { stop_reason: Option<String> },
    Error(String),
}

pub struct ChatJob {
    pub events: Receiver<ChatEvent>,
    pub cancel: Arc<AtomicBool>,
}

/// Un fournisseur à essayer, avec son modèle (vide : choisi automatiquement) et sa clé.
#[derive(Debug, Clone)]
pub struct Route {
    pub provider: AiProvider,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

/// Ce qu'il faut pour poser la question : le contexte est préparé dans le fil de la requête.
pub struct ChatInput {
    /// Fournisseur choisi d'abord, puis les relais
    pub routes: Vec<Route>,
    pub english: bool,
    pub sources: Vec<SourceSpec>,
    pub send_page_images: bool,
    pub turns: Vec<Turn>,
    /// Passer par le proxy de confidentialité (PrivAiTe) : texte seulement, aucun envoi direct
    pub privacy: bool,
}

fn agent(read_timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        // Silence maximal entre deux morceaux de réponse.
        .timeout_read(read_timeout)
        .user_agent(&format!("KitePDF/{}", env!("CARGO_PKG_VERSION")))
        .build()
}

fn with_auth(request: ureq::Request, provider: AiProvider, api_key: Option<&str>) -> ureq::Request {
    let request = match (provider, api_key) {
        (AiProvider::Anthropic, Some(key)) => request.set("x-api-key", key).set("anthropic-version", super::ANTHROPIC_VERSION),
        (AiProvider::Anthropic, None) => request.set("anthropic-version", super::ANTHROPIC_VERSION),
        (_, Some(key)) if !key.is_empty() => request.set("Authorization", &format!("Bearer {key}")),
        _ => request,
    };
    if provider == AiProvider::OpenRouter {
        // Identification de l'application, demandée par OpenRouter (aucune donnée de l'utilisateur).
        request.set("HTTP-Referer", "https://github.com/padimato/kite-pdf").set("X-Title", "Kite PDF")
    } else {
        request
    }
}

/// Message d'erreur lisible pour une réponse HTTP en échec.
fn http_error(code: u16, body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            let v = if v.is_array() { v[0].clone() } else { v };
            v["error"]["message"].as_str().or(v["error"].as_str()).or(v["message"].as_str()).map(str::to_string)
        })
        .unwrap_or_default();
    let summary = match code {
        401 | 403 => tr(
            "Clé API refusée. Vérifiez-la dans Réglages › Assistant IA.",
            "API key rejected. Check it in Settings › AI assistant.",
        )
        .to_string(),
        404 | 410 => tr(
            "Modèle ou adresse introuvable. Choisissez un autre modèle dans Réglages › Assistant IA.",
            "Model or address not found. Choose another model in Settings › AI assistant.",
        )
        .to_string(),
        413 => tr("Les documents envoyés sont trop gros pour ce modèle.", "The documents sent are too large for this model.").to_string(),
        429 => tr(
            "Limite d'utilisation atteinte chez le fournisseur. Réessayez dans un moment.",
            "Usage limit reached at the provider. Try again in a moment.",
        )
        .to_string(),
        529 | 503 | 502 | 500 => tr("Le service est surchargé. Réessayez dans un moment.", "The service is overloaded. Try again in a moment.").to_string(),
        _ => trf!("Erreur {} du service.", "Service error {}.", code),
    };
    let detail: String = detail.chars().take(300).collect();
    if detail.is_empty() {
        summary
    } else {
        format!("{summary}\n({detail})")
    }
}

fn transport_error(error: &ureq::Transport) -> String {
    trf!(
        "Impossible de joindre le service : {}",
        "Cannot reach the service: {}",
        error.to_string()
    )
}

/// Échec d'une requête : `answered` si une partie de la réponse avait déjà été affichée (pas de relais).
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub message: String,
    pub answered: bool,
}

/// Pose la question dans un fil à part.
pub fn start_chat(input: ChatInput, ctx: egui::Context) -> ChatJob {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_thread = cancel.clone();
    let _ = std::thread::Builder::new().name("kite-ai-chat".into()).spawn(move || {
        run_chat(input, &tx, &cancel_thread, &ctx);
        ctx.request_repaint();
    });
    ChatJob { events: rx, cancel }
}

/// Modèles choisis automatiquement, gardés 30 minutes : le proxy de confidentialité n'a pas à redémarrer
/// à chaque question parce que la liste d'OpenRouter a bougé.
static AUTO_MODELS: std::sync::Mutex<Option<(Instant, String, (String, Vec<String>))>> = std::sync::Mutex::new(None);

/// Modèles gratuits d'OpenRouter aux plus grands contextes : le premier, et deux de secours
/// (la liste des modèles gratuits change souvent et ils saturent aux heures de pointe).
fn auto_models(route: &Route) -> Result<(String, Vec<String>), String> {
    if let Ok(cache) = AUTO_MODELS.lock() {
        if let Some((at, base, models)) = cache.as_ref() {
            if *base == route.base_url && at.elapsed() < Duration::from_secs(30 * 60) {
                return Ok(models.clone());
            }
        }
    }
    let models = auto_models_uncached(route)?;
    if let Ok(mut cache) = AUTO_MODELS.lock() {
        *cache = Some((Instant::now(), route.base_url.clone(), models.clone()));
    }
    Ok(models)
}

fn auto_models_uncached(route: &Route) -> Result<(String, Vec<String>), String> {
    let mut models = list_models(route.provider, &route.base_url, route.api_key.as_deref())?.into_iter();
    let first = models
        .next()
        .ok_or_else(|| tr("aucun modèle gratuit disponible pour le moment", "no free model available right now").to_string())?;
    Ok((first, models.take(2).collect()))
}

/// Modèle de chaque fournisseur (choisi automatiquement si besoin) ; ceux sans modèle sont écartés.
fn resolve_routes(routes: &[Route]) -> Vec<(Route, String, Vec<String>)> {
    routes
        .iter()
        .filter_map(|route| {
            let (model, extra) = if route.model.is_empty() {
                auto_models(route).ok()?
            } else {
                (route.model.clone(), Vec::new())
            };
            Some((route.clone(), model, extra))
        })
        .collect()
}

/// Démarre le proxy de confidentialité en avance (fenêtre de l'assistant ouverte), pour que la
/// première question n'attende pas son chargement (jusqu'à une minute).
pub fn prewarm_privacy(routes: Vec<Route>, english: bool) {
    let _ = std::thread::Builder::new().name("kite-privaite-prewarm".into()).spawn(move || {
        let resolved = resolve_routes(&routes);
        let entries: Vec<super::privacy::ProxyEntry> = resolved.iter().map(|(r, m, _)| proxy_entry(r, m)).collect();
        if !entries.is_empty() {
            let _ = super::privacy::ensure_running(&entries, english, &|_| {});
        }
    });
}

/// Alias d'un fournisseur dans la configuration du proxy.
fn proxy_alias(provider: AiProvider) -> String {
    format!("kite-{}", provider.key())
}

/// Modèle exposé par le proxy pour cet essai.
fn proxy_entry(route: &Route, model: &str) -> super::privacy::ProxyEntry {
    let anthropic = route.provider == AiProvider::Anthropic;
    super::privacy::ProxyEntry {
        alias: proxy_alias(route.provider),
        litellm_model: if anthropic { format!("anthropic/{model}") } else { format!("openai/{model}") },
        api_base: (!anthropic).then(|| route.base_url.clone()),
        key: route.api_key.clone(),
        forward_params: match route.provider {
            AiProvider::OpenRouter => vec!["reasoning", "models"],
            AiProvider::Nvidia => vec!["chat_template_kwargs"],
            _ => vec![],
        },
    }
}

/// Journal de diagnostic des requêtes (`KITE_AI_DEBUG=1`) : adresses, statuts, délais, début du flux.
/// La clé n'y figure jamais.
fn debug_enabled() -> bool {
    std::env::var_os("KITE_AI_DEBUG").is_some()
}

fn run_chat(input: ChatInput, tx: &Sender<ChatEvent>, cancel: &AtomicBool, ctx: &egui::Context) {
    let send = |event: ChatEvent| {
        let _ = tx.send(event);
        ctx.request_repaint();
    };
    if !input.sources.is_empty() {
        send(ChatEvent::Status(tr("Lecture des documents…", "Reading the documents…").into()));
    }
    let mut input = input;
    let mut privacy_notes = Vec::new();
    // Le proxy ne s'arrête pas pour inactivité pendant cette demande.
    let _proxy_in_use = input.privacy.then(super::privacy::in_use);
    // Mode confidentialité : modèles résolus d'avance, proxy démarré une fois, texte seulement.
    let proxy = if input.privacy {
        if input.turns.iter().any(|t| !t.images.is_empty()) {
            for turn in &mut input.turns {
                turn.images.clear();
            }
            privacy_notes.push(tr(
                "Mode confidentialité : les images n'ont pas été envoyées, seulement le texte (dont celui des zones capturées).",
                "Privacy mode: images were not sent, only text (including the text of captured zones).",
            )
            .to_string());
        }
        input.send_page_images = false;
        let resolved = resolve_routes(&input.routes);
        let entries: Vec<super::privacy::ProxyEntry> = resolved.iter().map(|(r, m, _)| proxy_entry(r, m)).collect();
        send(ChatEvent::Status(tr("Démarrage du proxy de confidentialité…", "Starting the privacy proxy…").into()));
        let status = |detail: String| {
            send(ChatEvent::Status(trf!(
                "Démarrage du proxy de confidentialité… {}",
                "Starting the privacy proxy… {}",
                detail
            )));
        };
        match super::privacy::ensure_running(&entries, input.english, &status) {
            Ok(endpoint) => {
                input.routes = resolved
                    .iter()
                    .map(|(r, m, _)| Route { model: m.clone(), ..r.clone() })
                    .collect();
                Some((endpoint, resolved.into_iter().map(|(_, _, extra)| extra).collect::<Vec<_>>()))
            }
            Err(e) => {
                send(ChatEvent::Error(trf!(
                    "Rien n'a été envoyé : le proxy de confidentialité ne répond pas.\n{}",
                    "Nothing was sent: the privacy proxy is not responding.\n{}",
                    e
                )));
                return;
            }
        }
    } else {
        None
    };
    let conversation_chars: usize = input.turns.iter().map(|t| t.text.len()).sum();
    let has_images = input.turns.iter().any(|t| !t.images.is_empty());
    let mut failures: Vec<String> = Vec::new();
    let count = input.routes.len();
    if count == 0 {
        send(ChatEvent::Error(tr("Aucun fournisseur disponible.", "No provider available.").into()));
        return;
    }
    for (index, route) in input.routes.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let last = index + 1 == count;
        let info = providers::info(route.provider);
        let name = providers::display_name(route.provider);
        // Un modèle qui ne lit pas les images refuserait une capture : on passe au suivant s'il y en a un.
        let vision = info.vision || route.provider == AiProvider::Custom || !route.model.is_empty() && route.model != info.default_model;
        if has_images && !vision && !last {
            failures.push(trf!("{} : ne lit pas les images", "{}: does not read images", name));
            continue;
        }
        let (model, fallback_models) = if route.model.is_empty() {
            match auto_models(route) {
                Ok(models) => models,
                Err(e) => {
                    failures.push(format!("{name} : {e}"));
                    continue;
                }
            }
        } else {
            (route.model.clone(), Vec::new())
        };
        let budget = providers::document_budget_chars(route.provider, conversation_chars);
        let prepared = match sources::prepare(&input.sources, input.send_page_images && vision, budget) {
            Ok(prepared) => prepared,
            Err(e) => {
                send(ChatEvent::Error(trf!("Lecture du document impossible : {}", "Cannot read the document: {}", e)));
                return;
            }
        };
        send(ChatEvent::Status(if index == 0 {
            tr("Réflexion…", "Thinking…").to_string()
        } else {
            trf!("Essai avec {}…", "Trying {}…", name)
        }));
        let mut request = ChatRequest {
            provider: route.provider,
            model: model.clone(),
            base_url: route.base_url.clone(),
            api_key: route.api_key.clone(),
            system: super::system_prompt(input.english, input.sources.len()),
            context: prepared.blocks,
            turns: input.turns.clone(),
            fallback_models,
            via_proxy: false,
        };
        if let Some((endpoint, extras)) = &proxy {
            // Le proxy connaît le modèle sous l'alias « kite-<fournisseur> » et détient la clé.
            request.model = proxy_alias(route.provider);
            request.base_url = endpoint.base_url.clone();
            request.api_key = Some(endpoint.token.clone());
            request.fallback_models = extras.get(index).cloned().unwrap_or_default();
            request.via_proxy = true;
        }
        // Relais possible : on n'attend pas trop longtemps un fournisseur muet.
        let timeout = Duration::from_secs(if last { 120 } else { 45 });
        let started = Instant::now();
        let mut pending_notes = Some(privacy_notes.iter().cloned().chain(prepared.notes).collect::<Vec<_>>());
        let lock = if proxy.is_some() { " · 🔒" } else { "" };
        let fallback_notes = failures.clone();
        let mut served_model = model.clone();
        let result = stream(&request, cancel, timeout, |event| {
            if let ChatEvent::Via(model) = &event {
                if !model.starts_with("kite-") {
                    served_model = model.clone();
                }
                return;
            }
            let first_text = matches!(event, ChatEvent::Text(_)) && pending_notes.is_some();
            if let Some(notes) = first_text.then(|| pending_notes.take()).flatten() {
                // Premier mot : on dit qui répond, et pourquoi pas le fournisseur choisi.
                if !fallback_notes.is_empty() {
                    send(ChatEvent::Note(trf!(
                        "Relais : {}.",
                        "Fallback: {}.",
                        fallback_notes.iter().map(|n| n.trim_end_matches('.')).collect::<Vec<_>>().join(" ; ")
                    )));
                }
                for note in notes {
                    send(ChatEvent::Note(note));
                }
                let seconds = started.elapsed().as_secs_f32();
                send(ChatEvent::Via(format!("{name} · {served_model} · {seconds:.1} s{lock}")));
            }
            send(event);
        });
        match result {
            Ok(stop_reason) => {
                send(ChatEvent::Done { stop_reason });
                return;
            }
            Err(failure) if failure.answered || last || cancel.load(Ordering::Relaxed) => {
                // Échecs des fournisseurs précédents, puis celui-ci.
                let mut lines = failures;
                lines.push(if count > 1 { format!("{name} : {}", failure.message) } else { failure.message });
                send(ChatEvent::Error(lines.join("\n")));
                return;
            }
            Err(failure) => {
                let reason = failure.message.lines().next().unwrap_or_default().to_string();
                failures.push(format!("{name} : {reason}"));
            }
        }
    }
    if !failures.is_empty() {
        send(ChatEvent::Error(failures.join("\n")));
    }
}

/// Envoie la requête et transmet la réponse morceau par morceau. Renvoie la raison d'arrêt éventuelle.
pub fn stream(
    request: &ChatRequest,
    cancel: &AtomicBool,
    timeout: Duration,
    mut emit: impl FnMut(ChatEvent),
) -> Result<Option<String>, Failure> {
    // Vers le proxy, tout passe au format OpenAI (il relaie ensuite chez le vrai fournisseur).
    let provider = if request.via_proxy { AiProvider::OpenAi } else { request.provider };
    let (url, body) = match provider {
        AiProvider::Anthropic => (format!("{}/messages", request.base_url), super::anthropic_body(request)),
        _ => (format!("{}/chat/completions", request.base_url), super::openai_body(request)),
    };
    let mut call = with_auth(agent(timeout).post(&url), provider, request.api_key.as_deref())
        .set("content-type", "application/json")
        .set("accept", "text/event-stream");
    if provider == AiProvider::Anthropic {
        if let Some(beta) = super::anthropic_beta_header(&request.model) {
            call = call.set("anthropic-beta", beta);
        }
    }
    let failure = |message: String, answered: bool| Failure { message, answered };
    let debug = debug_enabled();
    let started = Instant::now();
    if debug {
        eprintln!("[ai] POST {url} modèle={} corps={} o", request.model, body.to_string().len());
    }
    let response = match call.send_string(&body.to_string()) {
        Ok(response) => response,
        Err(ureq::Error::Status(code, response)) => {
            let text = response.into_string().unwrap_or_default();
            if debug {
                eprintln!("[ai] HTTP {code} après {:.1} s : {}", started.elapsed().as_secs_f32(), text.chars().take(400).collect::<String>());
            }
            return Err(failure(http_error(code, &text), false));
        }
        Err(ureq::Error::Transport(error)) => {
            if debug {
                eprintln!("[ai] transport après {:.1} s : {error}", started.elapsed().as_secs_f32());
            }
            return Err(failure(transport_error(&error), false));
        }
    };
    if debug {
        eprintln!("[ai] HTTP {} après {:.1} s", response.status(), started.elapsed().as_secs_f32());
    }
    let mut logged_lines = 0;
    let mut thinking_reported = false;
    let mut served_model_reported = false;
    let reader = std::io::BufReader::new(response.into_reader());
    let mut stop_reason = None;
    let mut received_text = false;
    for line in reader.lines() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Some("cancelled".into()));
        }
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                let message = trf!("Réponse interrompue : {}", "Response interrupted: {}", e.to_string());
                return Err(failure(message, received_text));
            }
        };
        if debug && logged_lines < 4 && line.starts_with("data:") {
            logged_lines += 1;
            eprintln!("[ai] +{:.1} s {}", started.elapsed().as_secs_f32(), line.chars().take(240).collect::<String>());
        }
        if !served_model_reported && provider != AiProvider::Anthropic && line.starts_with("data:") {
            // Modèle qui répond vraiment (OpenRouter peut en avoir choisi un de secours).
            if let Some(model) = serde_json::from_str::<serde_json::Value>(line[5..].trim())
                .ok()
                .and_then(|v| v["model"].as_str().map(str::to_string))
                .filter(|m| !m.is_empty())
            {
                served_model_reported = true;
                emit(ChatEvent::Via(model));
            }
        }
        match parse_sse_line(provider, &line) {
            StreamEvent::Thinking => {
                if !thinking_reported {
                    thinking_reported = true;
                    if debug {
                        eprintln!("[ai] raisonnement après {:.1} s", started.elapsed().as_secs_f32());
                    }
                    emit(ChatEvent::Status(tr("Le modèle réfléchit…", "The model is thinking…").into()));
                }
            }
            StreamEvent::Text(text) => {
                if debug && !received_text {
                    eprintln!("[ai] premier mot après {:.1} s", started.elapsed().as_secs_f32());
                }
                received_text = true;
                emit(ChatEvent::Text(text));
            }
            StreamEvent::Stop(reason) => {
                if reason.is_some() {
                    stop_reason = reason;
                }
                if provider != AiProvider::Anthropic || line.contains("message_stop") {
                    break;
                }
            }
            StreamEvent::Error(message) => return Err(failure(message, received_text)),
            StreamEvent::Nothing => {}
        }
    }
    if !received_text {
        if stop_reason.as_deref() == Some("refusal") {
            return Ok(stop_reason);
        }
        return Err(failure(tr("Le modèle n'a rien répondu.", "The model returned no answer.").into(), false));
    }
    Ok(stop_reason)
}

/// Liste des modèles proposés par le fournisseur (appel bloquant).
pub fn list_models(provider: AiProvider, base_url: &str, api_key: Option<&str>) -> Result<Vec<String>, String> {
    let request = with_auth(agent(Duration::from_secs(20)).get(&format!("{base_url}/models")), provider, api_key);
    let request = if provider == AiProvider::Anthropic { request.query("limit", "100") } else { request };
    match request.call() {
        Ok(response) => Ok(super::parse_model_list(provider, &response.into_string().unwrap_or_default())),
        Err(ureq::Error::Status(code, response)) => Err(http_error(code, &response.into_string().unwrap_or_default())),
        Err(ureq::Error::Transport(error)) => Err(transport_error(&error)),
    }
}

/// Liste des modèles proposés par le fournisseur, lue dans un fil à part.
pub fn fetch_models(provider: AiProvider, base_url: String, api_key: Option<String>, ctx: egui::Context) -> Receiver<Result<Vec<String>, String>> {
    let (tx, rx) = channel();
    let _ = std::thread::Builder::new().name("kite-ai-models".into()).spawn(move || {
        let _ = tx.send(list_models(provider, &base_url, api_key.as_deref()));
        ctx.request_repaint();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Faux serveur local : lit une requête et répond `response` tel quel. Renvoie l'adresse et la requête reçue.
    fn fake_server(response: String) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut received = Vec::new();
            let mut buffer = [0u8; 65536];
            // En-têtes puis corps (Content-Length).
            loop {
                let n = socket.read(&mut buffer).unwrap();
                received.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&received).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    if received.len() >= end + 4 + length {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            socket.write_all(response.as_bytes()).unwrap();
            String::from_utf8_lossy(&received).to_string()
        });
        (address, handle)
    }

    fn sse_response(events: &[&str]) -> String {
        let body: String = events.iter().map(|e| format!("{e}\n\n")).collect();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn request(provider: AiProvider, base_url: String) -> ChatRequest {
        ChatRequest {
            provider,
            model: "claude-opus-5-5".into(),
            base_url,
            api_key: Some("sk-test".into()),
            system: "system".into(),
            context: vec![],
            turns: vec![Turn { user: true, text: "Bonjour".into(), images: vec![] }],
            fallback_models: vec![],
            via_proxy: false,
        }
    }

    #[test]
    fn test_anthropic_stream_end_to_end() {
        let (url, server) = fake_server(sse_response(&[
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Bonjour \"}}",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"[p. 2]\"}}",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}",
        ]));
        let mut events = Vec::new();
        let result = stream(&request(AiProvider::Anthropic, url), &AtomicBool::new(false), Duration::from_secs(5), |e| events.push(e));
        assert_eq!(result, Ok(None));
        assert_eq!(events, vec![ChatEvent::Text("Bonjour ".into()), ChatEvent::Text("[p. 2]".into())]);
        let received = server.join().unwrap();
        assert!(received.starts_with("POST /messages"));
        assert!(received.contains("x-api-key: sk-test"));
        assert!(received.contains("anthropic-version: 2026-10-21"));
        assert!(received.contains("anthropic-beta: server-side-fallback-2026-07-01"));
        assert!(received.contains("\"stream\":true"));
    }

    #[test]
    fn test_openai_stream_end_to_end() {
        let (url, server) = fake_server(sse_response(&[
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Salut\"},\"finish_reason\":null}]}",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}",
            "data: [DONE]",
        ]));
        let mut events = Vec::new();
        let result = stream(&request(AiProvider::OpenAi, url), &AtomicBool::new(false), Duration::from_secs(5), |e| events.push(e));
        assert_eq!(result, Ok(None));
        assert_eq!(events, vec![ChatEvent::Text("Salut".into())]);
        let received = server.join().unwrap();
        assert!(received.starts_with("POST /chat/completions"));
        assert!(received.contains("Authorization: Bearer sk-test") || received.contains("authorization: Bearer sk-test"));
    }

    #[test]
    fn test_http_error_is_readable() {
        let body = "{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}";
        let response = format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (url, server) = fake_server(response);
        let result = stream(&request(AiProvider::Anthropic, url), &AtomicBool::new(false), Duration::from_secs(5), |_| {});
        server.join().unwrap();
        let Err(failure) = result else { panic!("erreur attendue") };
        assert!(failure.message.contains("invalid x-api-key"));
        assert!(!failure.answered);
    }

    fn http_response(status: &str, body: &str) -> String {
        format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    }

    /// Le premier fournisseur est saturé (429) : la question part chez le suivant, qui répond.
    #[test]
    fn test_fallback_to_next_provider() {
        let (busy, busy_server) = fake_server(http_response("429 Too Many Requests", r#"{"error":{"message":"rate limited"}}"#));
        let (ok, ok_server) = fake_server(sse_response(&[
            "data: {\"choices\":[{\"delta\":{\"content\":\"Réponse\"},\"finish_reason\":null}]}",
            "data: [DONE]",
        ]));
        let route = |provider, base_url: String| Route { provider, model: "m".into(), base_url, api_key: Some("k".into()) };
        let input = ChatInput {
            routes: vec![route(AiProvider::Groq, busy), route(AiProvider::Gemini, ok)],
            english: false,
            sources: vec![],
            send_page_images: false,
            turns: vec![Turn { user: true, text: "Bonjour".into(), images: vec![] }],
            privacy: false,
        };
        let (tx, rx) = channel();
        run_chat(input, &tx, &AtomicBool::new(false), &egui::Context::default());
        busy_server.join().unwrap();
        ok_server.join().unwrap();
        let events: Vec<ChatEvent> = rx.try_iter().collect();
        assert!(events.iter().any(|e| matches!(e, ChatEvent::Note(n) if n.contains("Groq") && n.contains("Limite"))), "{events:?}");
        assert!(events.iter().any(|e| matches!(e, ChatEvent::Via(v) if v.starts_with("Google Gemini · m"))), "{events:?}");
        assert!(events.contains(&ChatEvent::Text("Réponse".into())));
        assert_eq!(events.last(), Some(&ChatEvent::Done { stop_reason: None }));
    }

    /// Tous les fournisseurs échouent : un seul message liste les raisons.
    #[test]
    fn test_all_providers_fail() {
        let (a, a_server) = fake_server(http_response("503 Service Unavailable", "{}"));
        let (b, b_server) = fake_server(http_response("401 Unauthorized", r#"{"error":{"message":"bad key"}}"#));
        let route = |provider, base_url: String| Route { provider, model: "m".into(), base_url, api_key: Some("k".into()) };
        let input = ChatInput {
            routes: vec![route(AiProvider::Mistral, a), route(AiProvider::Nvidia, b)],
            english: false,
            sources: vec![],
            send_page_images: false,
            turns: vec![Turn { user: true, text: "Bonjour".into(), images: vec![] }],
            privacy: false,
        };
        let (tx, rx) = channel();
        run_chat(input, &tx, &AtomicBool::new(false), &egui::Context::default());
        a_server.join().unwrap();
        b_server.join().unwrap();
        let events: Vec<ChatEvent> = rx.try_iter().collect();
        let Some(ChatEvent::Error(message)) = events.last() else { panic!("{events:?}") };
        assert!(message.contains("Mistral") && message.contains("NVIDIA NIM") && message.contains("bad key"), "{message}");
    }
}
