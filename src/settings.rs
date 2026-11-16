//! Réglages gardés d'un lancement à l'autre : fenêtre, barre latérale et préférences de la page Réglages.
//! Fichier texte `clé=valeur` dans `%APPDATA%\KitePDF\settings.txt`.

use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    /// Position (coin haut-gauche) et taille intérieure de la fenêtre, en points
    pub window: Option<[f32; 4]>,
    pub maximized: bool,
    pub sidebar_pinned: Option<bool>,
    pub sidebar_width: Option<f32>,
    pub drawer_height: Option<f32>,
    /// Écran d'accueil déjà montré (premier lancement)
    pub welcome_seen: bool,
    /// Choix de la page Réglages
    pub prefs: Preferences,
}

/// Lecture du texte des images (OCR) pendant la lecture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageOcr {
    /// En arrière-plan, page après page
    #[default]
    Auto,
    /// Seulement pendant une recherche (`Ctrl+F`)
    OnSearch,
    Never,
}

/// Zoom d'un PDF ouvert pour la première fois.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OpenZoom {
    #[default]
    Actual,
    FitWidth,
    FitPage,
}

/// Place des commandes : barre latérale, ou barre en haut (écran portrait).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Layout {
    /// Barre en haut quand la fenêtre est plus haute que large, barre latérale sinon
    #[default]
    Auto,
    Sidebar,
    TopBar,
}

impl Layout {
    /// Barre en haut pour une fenêtre de cette taille. `current` (disposition affichée) évite les
    /// bascules répétées quand la fenêtre est presque carrée.
    pub fn top_bar(self, size: [f32; 2], current: bool) -> bool {
        let [width, height] = size;
        match self {
            Layout::Sidebar => false,
            Layout::TopBar => true,
            Layout::Auto if current => height > width * 0.95,
            Layout::Auto => height > width * 1.05,
        }
    }
}

/// Langue de l'interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Language {
    /// Celle de Windows (français si Windows est en français, anglais sinon)
    #[default]
    Auto,
    Fr,
    En,
}

impl Language {
    pub fn resolve(self) -> crate::i18n::Lang {
        match self {
            Language::Auto => crate::i18n::system_lang(),
            Language::Fr => crate::i18n::Lang::Fr,
            Language::En => crate::i18n::Lang::En,
        }
    }
}

/// Fournisseur de l'assistant IA. Les métadonnées (adresse, modèle par défaut, offre gratuite...) sont
/// dans `ai::providers`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AiProvider {
    /// Google Gemini : offre gratuite, lit les longs PDF
    #[default]
    Gemini,
    /// Groq : le plus rapide, offre gratuite limitée à de petits textes
    Groq,
    /// Mistral (La Plateforme) : offre gratuite, hébergé en Europe
    Mistral,
    /// OpenRouter : modèles gratuits « :free »
    OpenRouter,
    /// NVIDIA NIM (build.nvidia.com) : crédits gratuits
    Nvidia,
    /// Cerebras : essai
    Cerebras,
    Anthropic,
    OpenAi,
    /// Service compatible avec l'API d'OpenAI à une adresse choisie (Ollama en local...)
    Custom,
}

impl AiProvider {
    pub const ALL: [AiProvider; 9] = [
        AiProvider::Gemini,
        AiProvider::Groq,
        AiProvider::Mistral,
        AiProvider::OpenRouter,
        AiProvider::Nvidia,
        AiProvider::Cerebras,
        AiProvider::Anthropic,
        AiProvider::OpenAi,
        AiProvider::Custom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            AiProvider::Gemini => "gemini",
            AiProvider::Groq => "groq",
            AiProvider::Mistral => "mistral",
            AiProvider::OpenRouter => "openrouter",
            AiProvider::Nvidia => "nvidia",
            AiProvider::Cerebras => "cerebras",
            AiProvider::Anthropic => "anthropic",
            AiProvider::OpenAi => "openai",
            AiProvider::Custom => "custom",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        AiProvider::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// Réglages de l'assistant IA (les clés API sont dans le coffre de Windows, jamais ici).
#[derive(Debug, Clone, PartialEq)]
pub struct AiPrefs {
    /// Bouton rond de l'assistant en bas à droite
    pub show_button: bool,
    pub provider: AiProvider,
    /// Modèle choisi pour chaque fournisseur, dans l'ordre de `AiProvider::ALL` (vide : modèle par défaut)
    pub models: [String; 9],
    /// Adresse du service compatible OpenAI (ex. `http://localhost:11434/v1`)
    pub custom_url: String,
    /// Envoyer en image les pages sans texte (scannées)
    pub send_scanned_pages: bool,
    /// Garder les conversations sur cet ordinateur
    pub keep_conversations: bool,
    /// Si le fournisseur choisi ne répond pas (limite atteinte, panne), essayer les autres dont la clé est rangée
    pub fallback: bool,
    /// Passer par le proxy de confidentialité PrivAiTe (installé à la demande)
    pub privacy_proxy: bool,
}

impl Default for AiPrefs {
    fn default() -> Self {
        Self {
            show_button: true,
            provider: AiProvider::default(),
            models: Default::default(),
            custom_url: String::new(),
            send_scanned_pages: true,
            keep_conversations: true,
            fallback: true,
            privacy_proxy: false,
        }
    }
}

impl AiPrefs {
    pub fn model(&self, provider: AiProvider) -> &str {
        &self.models[provider as usize]
    }

    pub fn set_model(&mut self, provider: AiProvider, model: String) {
        self.models[provider as usize] = model;
    }
}

/// Préférences modifiables dans la page Réglages.
#[derive(Debug, Clone, PartialEq)]
pub struct Preferences {
    pub image_ocr: ImageOcr,
    pub open_zoom: OpenZoom,
    pub layout: Layout,
    /// Rouvrir un PDF à la page et au zoom où on l'avait laissé
    pub resume_reading: bool,
    /// Garder la liste des PDF ouverts récemment
    pub keep_history: bool,
    /// Consulter GitHub une fois par jour pour signaler une nouvelle version
    pub check_updates: bool,
    pub language: Language,
    pub ai: AiPrefs,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            ai: AiPrefs::default(),
            image_ocr: ImageOcr::Auto,
            open_zoom: OpenZoom::Actual,
            layout: Layout::Auto,
            resume_reading: true,
            keep_history: true,
            check_updates: true,
        }
    }
}

/// Variable d'environnement : dossier des réglages et de l'historique à la place de
/// `%APPDATA%\KitePDF` (essais automatisés, installation portable).
pub const DATA_DIR_ENV: &str = "KITE_DATA_DIR";

/// Nom des dossiers de l'application dans `%APPDATA%` et `%LOCALAPPDATA%`.
pub const APP_DIR_NAME: &str = "KitePDF";
/// Nom des dossiers avant que l'application s'appelle Kite PDF.
const LEGACY_APP_DIR_NAME: &str = "SuperLecteurPDF";

fn known_folder(var: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// Dossier des réglages, de l'historique et des préférences de l'utilisateur.
pub fn app_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return PathBuf::from(dir);
    }
    known_folder("APPDATA").join(APP_DIR_NAME)
}

/// Dossier local (non synchronisé) : composants téléchargés.
pub fn local_data_dir() -> PathBuf {
    known_folder("LOCALAPPDATA").join(APP_DIR_NAME)
}

/// Reprend les dossiers de l'ancien nom (« Super Lecteur PDF ») : historique, réglages et composants
/// déjà téléchargés restent ainsi disponibles après le changement de nom.
pub fn migrate_legacy_dirs() {
    for var in ["APPDATA", "LOCALAPPDATA"] {
        let base = known_folder(var);
        let (old, new) = (base.join(LEGACY_APP_DIR_NAME), base.join(APP_DIR_NAME));
        if old.is_dir() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}

/// Les captures automatisées n'écrivent pas dans les réglages réels, sauf dossier de réglages dédié.
pub fn persistence_enabled(automated: bool) -> bool {
    !automated || std::env::var_os(DATA_DIR_ENV).is_some()
}

fn storage() -> PathBuf {
    app_data_dir().join("settings.txt")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(storage()).map(|t| Self::parse(&t)).unwrap_or_default()
    }

    fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            match key.trim() {
                "window" => {
                    let numbers: Vec<f32> = value.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                    if let [x, y, w, h] = numbers[..] {
                        settings.window = Some([x, y, w, h]);
                    }
                }
                "maximized" => settings.maximized = value.trim() == "1",
                "sidebar_pinned" => settings.sidebar_pinned = Some(value.trim() == "1"),
                "sidebar_width" => settings.sidebar_width = value.trim().parse().ok(),
                "drawer_height" => settings.drawer_height = value.trim().parse().ok(),
                "welcome_seen" => settings.welcome_seen = value.trim() == "1",
                "image_ocr" => {
                    settings.prefs.image_ocr = match value.trim() {
                        "search" => ImageOcr::OnSearch,
                        "never" => ImageOcr::Never,
                        _ => ImageOcr::Auto,
                    }
                }
                "open_zoom" => {
                    settings.prefs.open_zoom = match value.trim() {
                        "width" => OpenZoom::FitWidth,
                        "page" => OpenZoom::FitPage,
                        _ => OpenZoom::Actual,
                    }
                }
                "layout" => {
                    settings.prefs.layout = match value.trim() {
                        "sidebar" => Layout::Sidebar,
                        "top" => Layout::TopBar,
                        _ => Layout::Auto,
                    }
                }
                "resume_reading" => settings.prefs.resume_reading = value.trim() != "0",
                "keep_history" => settings.prefs.keep_history = value.trim() != "0",
                "check_updates" => settings.prefs.check_updates = value.trim() != "0",
                "language" => {
                    settings.prefs.language = match value.trim() {
                        "fr" => Language::Fr,
                        "en" => Language::En,
                        _ => Language::Auto,
                    }
                }
                "ai_button" => settings.prefs.ai.show_button = value.trim() != "0",
                "ai_provider" => settings.prefs.ai.provider = AiProvider::from_key(value.trim()).unwrap_or_default(),
                "ai_fallback" => settings.prefs.ai.fallback = value.trim() != "0",
                "ai_privacy" => settings.prefs.ai.privacy_proxy = value.trim() == "1",
                "ai_custom_url" => settings.prefs.ai.custom_url = value.trim().to_string(),
                "ai_scanned_pages" => settings.prefs.ai.send_scanned_pages = value.trim() != "0",
                "ai_keep_conversations" => settings.prefs.ai.keep_conversations = value.trim() != "0",
                other => {
                    if let Some(provider) = other.strip_prefix("ai_model_").and_then(AiProvider::from_key) {
                        settings.prefs.ai.set_model(provider, value.trim().to_string());
                    }
                }
            }
        }
        settings
    }

    fn render(&self) -> String {
        let mut text = String::new();
        if let Some([x, y, w, h]) = self.window {
            text.push_str(&format!("window={x:.0},{y:.0},{w:.0},{h:.0}\n"));
        }
        text.push_str(&format!("maximized={}\n", u8::from(self.maximized)));
        if let Some(pinned) = self.sidebar_pinned {
            text.push_str(&format!("sidebar_pinned={}\n", u8::from(pinned)));
        }
        if let Some(width) = self.sidebar_width {
            text.push_str(&format!("sidebar_width={width:.0}\n"));
        }
        if let Some(height) = self.drawer_height {
            text.push_str(&format!("drawer_height={height:.0}\n"));
        }
        if self.welcome_seen {
            text.push_str("welcome_seen=1\n");
        }
        let prefs = &self.prefs;
        let image_ocr = match prefs.image_ocr {
            ImageOcr::Auto => "auto",
            ImageOcr::OnSearch => "search",
            ImageOcr::Never => "never",
        };
        let open_zoom = match prefs.open_zoom {
            OpenZoom::Actual => "100",
            OpenZoom::FitWidth => "width",
            OpenZoom::FitPage => "page",
        };
        let layout = match prefs.layout {
            Layout::Auto => "auto",
            Layout::Sidebar => "sidebar",
            Layout::TopBar => "top",
        };
        text.push_str(&format!("image_ocr={image_ocr}\nopen_zoom={open_zoom}\nlayout={layout}\n"));
        for (key, value) in [
            ("resume_reading", prefs.resume_reading),
            ("keep_history", prefs.keep_history),
            ("check_updates", prefs.check_updates),
        ] {
            text.push_str(&format!("{key}={}\n", u8::from(value)));
        }
        let language = match prefs.language {
            Language::Auto => "auto",
            Language::Fr => "fr",
            Language::En => "en",
        };
        text.push_str(&format!("language={language}\n"));
        let ai = &prefs.ai;
        text.push_str(&format!("ai_provider={}\n", ai.provider.key()));
        for (key, value) in [
            ("ai_button", ai.show_button),
            ("ai_scanned_pages", ai.send_scanned_pages),
            ("ai_keep_conversations", ai.keep_conversations),
            ("ai_fallback", ai.fallback),
            ("ai_privacy", ai.privacy_proxy),
        ] {
            text.push_str(&format!("{key}={}\n", u8::from(value)));
        }
        for provider in AiProvider::ALL {
            let model = ai.model(provider);
            if !model.is_empty() {
                text.push_str(&format!("ai_model_{}={model}\n", provider.key()));
            }
        }
        if !ai.custom_url.is_empty() {
            text.push_str(&format!("ai_custom_url={}\n", ai.custom_url));
        }
        text
    }

    pub fn save(&self) {
        let path = storage();
        // Rien à écrire si le fichier dit déjà la même chose (enregistrement régulier).
        if std::fs::read_to_string(&path).is_ok_and(|current| current == self.render()) {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, self.render());
    }

    /// Fenêtre visible sur au moins un écran (un écran débranché ne doit pas la cacher).
    pub fn window_is_plausible(&self) -> bool {
        self.window.is_some_and(|[x, y, w, h]| {
            w >= 400.0 && h >= 300.0 && x > -w + 80.0 && y > -40.0 && x < 16_000.0 && y < 16_000.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_roundtrip() {
        let settings = Settings {
            window: Some([120.0, 80.0, 1280.0, 820.0]),
            maximized: true,
            sidebar_pinned: Some(false),
            sidebar_width: Some(320.0),
            drawer_height: Some(180.0),
            welcome_seen: true,
            prefs: Preferences {
                image_ocr: ImageOcr::OnSearch,
                open_zoom: OpenZoom::FitWidth,
                layout: Layout::TopBar,
                resume_reading: false,
                keep_history: true,
                check_updates: false,
                language: Language::En,
                ai: AiPrefs {
                    show_button: false,
                    provider: AiProvider::Custom,
                    models: {
                        let mut models: [String; 9] = Default::default();
                        models[AiProvider::Anthropic as usize] = "claude-sonnet-5-5".into();
                        models[AiProvider::Groq as usize] = "openai/gpt-oss-20b".into();
                        models[AiProvider::Custom as usize] = "llama3".into();
                        models
                    },
                    custom_url: "http://localhost:11434/v1".into(),
                    send_scanned_pages: false,
                    keep_conversations: false,
                    fallback: false,
                    privacy_proxy: true,
                },
            },
        };
        assert_eq!(Settings::parse(&settings.render()), settings);
        assert!(settings.window_is_plausible());
        // Ancien fichier sans préférences : valeurs par défaut (OCR automatique, reprise, historique).
        assert_eq!(Settings::parse("maximized=1\n").prefs, Preferences::default());
        // Fenêtre restée sur un écran débranché : ignorée.
        let lost = Settings { window: Some([-5000.0, 80.0, 1280.0, 820.0]), ..Default::default() };
        assert!(!lost.window_is_plausible());
    }

    #[test]
    fn test_layout_follows_window_shape() {
        // Écran portrait : barre en haut ; paysage : barre latérale.
        assert!(Layout::Auto.top_bar([1080.0, 1870.0], false));
        assert!(!Layout::Auto.top_bar([1920.0, 1040.0], true));
        // Fenêtre presque carrée : la disposition affichée est gardée.
        assert!(Layout::Auto.top_bar([1000.0, 1000.0], true));
        assert!(!Layout::Auto.top_bar([1000.0, 1000.0], false));
        // Choix imposé dans les Réglages.
        assert!(Layout::TopBar.top_bar([1920.0, 1040.0], false));
        assert!(!Layout::Sidebar.top_bar([1080.0, 1870.0], true));
    }
}
