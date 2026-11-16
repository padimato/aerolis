//! Page Réglages (`Ctrl+,`), rangée en rubriques : Général, Documents, Texte des images, Assistant IA,
//! Confidentialité, Mises à jour, Aide. Liste des rubriques à gauche (en haut sur une fenêtre étroite).
//! Chaque changement s'applique tout de suite ; l'application l'enregistre.

use crate::i18n::tr;
use crate::settings::{AiProvider, ImageOcr, Language, Layout, OpenZoom, Preferences};
use crate::trf;
use egui::{Color32, CornerRadius, Margin, RichText, Stroke};

const MUTED: Color32 = Color32::from_rgb(150, 160, 182);
const SECTION: Color32 = Color32::from_rgb(130, 170, 255);
const NAV_SELECTED: Color32 = Color32::from_rgb(45, 62, 105);
const OK_GREEN: Color32 = Color32::from_rgb(120, 210, 150);
const WARN: Color32 = Color32::from_rgb(250, 200, 110);

/// Rubriques de la page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    General,
    Documents,
    ImageText,
    Assistant,
    Privacy,
    Updates,
    Help,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::General,
        Section::Documents,
        Section::ImageText,
        Section::Assistant,
        Section::Privacy,
        Section::Updates,
        Section::Help,
    ];

    fn icon(self) -> &'static str {
        match self {
            Section::General => "⚙",
            Section::Documents => "🗋",
            Section::ImageText => "🔍",
            Section::Assistant => "✨",
            Section::Privacy => "🔒",
            Section::Updates => "⟳",
            Section::Help => "❓",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Section::General => tr("Général", "General"),
            Section::Documents => tr("Documents", "Documents"),
            Section::ImageText => tr("Texte des images", "Text in images"),
            Section::Assistant => tr("Assistant IA", "AI assistant"),
            Section::Privacy => tr("Confidentialité", "Privacy"),
            Section::Updates => tr("Mises à jour", "Updates"),
            Section::Help => tr("Aide", "Help"),
        }
    }

    /// Nom court pour la ligne de commande QA (`--settings assistant`).
    pub fn from_key(key: &str) -> Option<Section> {
        Some(match key {
            "general" => Section::General,
            "documents" => Section::Documents,
            "ocr" | "images" => Section::ImageText,
            "assistant" | "ai" => Section::Assistant,
            "privacy" => Section::Privacy,
            "updates" => Section::Updates,
            "help" => Section::Help,
            _ => return None,
        })
    }
}

/// État de la lecture des images, affiché dans la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrStatus {
    Installed,
    Downloading,
    Missing,
}

/// Liste des modèles proposés par le fournisseur d'IA.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ModelList {
    #[default]
    Unknown,
    Loading,
    Loaded(Vec<String>),
    Failed(String),
}

/// État de la page gardé d'une image à l'autre (rubrique ouverte, saisies en cours).
#[derive(Debug, Default)]
pub struct PageState {
    pub section: Section,
    /// Clé API en cours de saisie (jamais enregistrée sur le disque)
    pub key_draft: String,
    /// « Effacer l'historique » cliqué une première fois
    pub confirm_clear_history: bool,
    /// « Supprimer les conversations » cliqué une première fois
    pub confirm_clear_conversations: bool,
    /// « Désinstaller » le proxy de confidentialité cliqué une première fois
    pub confirm_uninstall_privacy: bool,
}

pub struct PageInfo {
    pub ocr: OcrStatus,
    pub history_len: usize,
    /// Résultat de « Rechercher une mise à jour »
    pub update_check: crate::updates::ManualCheck,
    /// Une clé API est rangée pour le fournisseur choisi
    pub ai_key_saved: bool,
    /// Fournisseurs dont une clé est rangée
    pub ai_saved_keys: Vec<AiProvider>,
    pub ai_models: ModelList,
    pub ai_conversations: usize,
}

/// Demandes faites depuis la page, exécutées par l'application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Close,
    ConfirmClearHistory,
    InstallOcr,
    MakeDefaultReader,
    ShowShortcuts,
    ShowWelcome,
    OpenProjectPage,
    CheckUpdatesNow,
    OpenNewRelease,
    /// Range `key_draft` dans le coffre de Windows pour le fournisseur choisi
    SaveApiKey,
    DeleteApiKey,
    RefreshModels,
    ConfirmClearConversations,
    OpenAssistant,
    InstallPrivacy,
    CancelPrivacyInstall,
    UninstallPrivacy,
    OpenPrivacyLog,
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).size(14.0).strong().color(SECTION));
    ui.add_space(2.0);
}

fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.5).color(MUTED));
}

fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.5).color(Color32::WHITE));
}

/// Affiche la page. `prefs` est modifié directement ; renvoie les autres demandes.
pub fn show(ctx: &egui::Context, prefs: &mut Preferences, state: &mut PageState, info: &PageInfo) -> Vec<Action> {
    let mut actions = Vec::new();
    let screen = ctx.screen_rect();
    let width = 780.0_f32.min(screen.width() - 32.0);
    // Fenêtre étroite (écran portrait) : rubriques en haut plutôt qu'à gauche.
    let narrow = width < 600.0;
    let frame = egui::Frame::NONE
        .fill(Color32::from_rgb(24, 28, 39))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(70, 100, 170)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(20));
    let modal = egui::Modal::new(egui::Id::new("aerolis_settings")).frame(frame).show(ctx, |ui| {
        let inner_width = width - 40.0;
        ui.set_width(inner_width);
        ui.set_max_width(inner_width);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Réglages", "Settings")).size(19.0).strong().color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Fermer", "Close")).clicked() {
                    actions.push(Action::Close);
                }
            });
        });
        ui.add_space(8.0);
        let body_height = (screen.height() - 150.0).clamp(240.0, 560.0);
        if narrow {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for section in Section::ALL {
                    let text = RichText::new(format!("{} {}", section.icon(), section.title())).size(12.0);
                    if ui.selectable_label(state.section == section, text).clicked() {
                        state.section = section;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt(("settings_body", state.section as u8))
                .max_height(body_height)
                .min_scrolled_height(body_height)
                .show(ui, |ui| {
                    ui.set_width(inner_width - 12.0);
                    show_section(ui, prefs, state, info, &mut actions);
                });
        } else {
            let nav_width = 176.0;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(nav_width, body_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(nav_width);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for section in Section::ALL {
                            if nav_button(ui, section, state.section == section, nav_width) {
                                state.section = section;
                            }
                        }
                    },
                );
                ui.add_space(6.0);
                let (line_rect, _) = ui.allocate_exact_size(egui::vec2(1.0, body_height), egui::Sense::hover());
                ui.painter().rect_filled(line_rect, 0.0, Color32::from_rgb(48, 56, 78));
                ui.add_space(14.0);
                let content_width = inner_width - nav_width - 36.0;
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("settings_body", state.section as u8))
                        .max_height(body_height)
                        .min_scrolled_height(body_height)
                        .show(ui, |ui| {
                            ui.set_width(content_width);
                            show_section(ui, prefs, state, info, &mut actions);
                        });
                });
            });
        }
    });
    if modal.should_close() && !actions.contains(&Action::Close) {
        actions.push(Action::Close);
    }
    if actions.contains(&Action::Close) {
        state.confirm_clear_history = false;
        state.confirm_clear_conversations = false;
        state.key_draft.clear();
    }
    actions
}

/// Entrée de la liste des rubriques : icône et nom, fond bleu quand elle est ouverte.
fn nav_button(ui: &mut egui::Ui, section: Section, selected: bool, width: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 32.0), egui::Sense::click());
    let fill = if selected {
        NAV_SELECTED
    } else if response.hovered() {
        Color32::from_rgb(36, 42, 58)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 8.0, fill);
    let color = if selected { Color32::WHITE } else { Color32::from_rgb(190, 198, 215) };
    ui.painter().text(
        rect.left_center() + egui::vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        section.icon(),
        egui::FontId::proportional(14.0),
        color,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(36.0, 0.0),
        egui::Align2::LEFT_CENTER,
        section.title(),
        egui::FontId::proportional(13.0),
        color,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

fn show_section(
    ui: &mut egui::Ui,
    prefs: &mut Preferences,
    state: &mut PageState,
    info: &PageInfo,
    actions: &mut Vec<Action>,
) {
    ui.label(RichText::new(state.section.title()).size(16.0).strong().color(Color32::WHITE));
    match state.section {
        Section::General => general(ui, prefs, actions),
        Section::Documents => documents(ui, prefs),
        Section::ImageText => image_text(ui, prefs, info, actions),
        Section::Assistant => assistant(ui, prefs, state, info, actions),
        Section::Privacy => privacy(ui, prefs, state, info, actions),
        Section::Updates => updates(ui, prefs, info, actions),
        Section::Help => help(ui, actions),
    }
    ui.add_space(8.0);
}

fn general(ui: &mut egui::Ui, prefs: &mut Preferences, actions: &mut Vec<Action>) {
    section_title(ui, tr("Langue", "Language"));
    ui.radio_value(&mut prefs.language, Language::Auto, tr("Celle de Windows", "Same as Windows"));
    ui.radio_value(&mut prefs.language, Language::Fr, "Français");
    ui.radio_value(&mut prefs.language, Language::En, "English");

    section_title(ui, tr("Disposition", "Layout"));
    ui.radio_value(&mut prefs.layout, Layout::Auto, tr("Automatique (recommandé)", "Automatic (recommended)"));
    note(ui, tr(
        "Barre latérale quand la fenêtre est plus large que haute, barre en haut sur un écran en portrait.",
        "Sidebar when the window is wider than tall, top bar on a portrait screen.",
    ));
    ui.radio_value(&mut prefs.layout, Layout::Sidebar, tr("Toujours la barre latérale", "Always the sidebar"));
    ui.radio_value(&mut prefs.layout, Layout::TopBar, tr("Toujours la barre en haut", "Always the top bar"));

    section_title(ui, "Windows");
    if ui.button(tr("Faire d'Aerolis le lecteur PDF par défaut…", "Make Aerolis the default PDF reader…")).clicked() {
        actions.push(Action::MakeDefaultReader);
    }
    note(ui, tr(
        "Ouvre les Paramètres de Windows : choisissez Aerolis pour les fichiers .pdf.",
        "Opens Windows Settings: choose Aerolis for .pdf files.",
    ));
}

fn documents(ui: &mut egui::Ui, prefs: &mut Preferences) {
    section_title(ui, tr("Ouverture", "Opening"));
    label(ui, tr("Zoom d'un PDF ouvert pour la première fois", "Zoom of a PDF opened for the first time"));
    ui.horizontal_wrapped(|ui| {
        ui.radio_value(&mut prefs.open_zoom, OpenZoom::Actual, "100 %");
        ui.radio_value(&mut prefs.open_zoom, OpenZoom::FitWidth, tr("Toute la largeur", "Fit width"));
        ui.radio_value(&mut prefs.open_zoom, OpenZoom::FitPage, tr("Page entière", "Whole page"));
    });
    ui.add_space(8.0);
    ui.add_enabled(
        prefs.keep_history,
        egui::Checkbox::new(
            &mut prefs.resume_reading,
            tr("Reprendre la lecture là où je l'avais laissée", "Resume reading where I left off"),
        ),
    );
    note(ui, if prefs.keep_history {
        tr(
            "Un PDF déjà lu se rouvre à la même page et au même zoom.",
            "A PDF you already read reopens at the same page and zoom.",
        )
    } else {
        tr(
            "Nécessite l'historique des PDF récents (rubrique Confidentialité).",
            "Requires the recent PDF history (Privacy section).",
        )
    });
}

fn image_text(ui: &mut egui::Ui, prefs: &mut Preferences, info: &PageInfo, actions: &mut Vec<Action>) {
    ui.add_space(4.0);
    note(ui, tr(
        "Aerolis lit le texte des images et des pages scannées pour que la recherche (Ctrl+F) le trouve. Ce travail se fait sur l'ordinateur, sans rien envoyer.",
        "Aerolis reads the text of images and scanned pages so that search (Ctrl+F) finds it. This happens on your computer; nothing is sent.",
    ));
    section_title(ui, tr("Quand lire les images", "When to read images"));
    ui.radio_value(
        &mut prefs.image_ocr,
        ImageOcr::Auto,
        tr("Lire les images en arrière-plan (recommandé)", "Read images in the background (recommended)"),
    );
    ui.radio_value(&mut prefs.image_ocr, ImageOcr::OnSearch, tr("Seulement quand je cherche un mot", "Only when I search for a word"));
    ui.radio_value(&mut prefs.image_ocr, ImageOcr::Never, tr("Jamais", "Never"));
    section_title(ui, tr("Module de lecture", "Reading module"));
    ui.horizontal_wrapped(|ui| match info.ocr {
        OcrStatus::Installed => {
            ui.label(RichText::new(tr("✔ Installé", "✔ Installed")).size(12.0).color(OK_GREEN));
        }
        OcrStatus::Downloading => {
            ui.add(egui::Spinner::new().size(12.0));
            note(ui, tr("Téléchargement en cours…", "Downloading…"));
        }
        OcrStatus::Missing => {
            note(ui, tr("Pas encore installé.", "Not installed yet."));
            if ui.small_button(tr("Installer (environ 30 Mo)", "Install (about 30 MB)")).clicked() {
                actions.push(Action::InstallOcr);
            }
        }
    });
}

fn assistant(ui: &mut egui::Ui, prefs: &mut Preferences, state: &mut PageState, info: &PageInfo, actions: &mut Vec<Action>) {
    let ai = &mut prefs.ai;
    ui.add_space(4.0);
    note(ui, tr(
        "Posez des questions sur vos PDF ouverts. Les pages choisies comme sources sont envoyées au fournisseur d'IA avec votre question, avec votre propre clé API.",
        "Ask questions about your open PDFs. The pages chosen as sources are sent to the AI provider with your question, using your own API key.",
    ));
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut ai.show_button, tr("Afficher le bouton de l'assistant", "Show the assistant button"));
        if ui.small_button(tr("Ouvrir l'assistant", "Open the assistant")).clicked() {
            actions.push(Action::OpenAssistant);
        }
    });
    if crate::ai::privacy::VISIBLE {
        let privacy_label = if ai.privacy_proxy {
            tr("🔒 Envois anonymisés par PrivAiTe (rubrique Confidentialité)", "🔒 Messages anonymized by PrivAiTe (Privacy section)")
        } else {
            tr("🔒 Anonymiser les envois : rubrique Confidentialité", "🔒 Anonymize what is sent: Privacy section")
        };
        if ui.link(RichText::new(privacy_label).size(11.5)).clicked() {
            state.section = Section::Privacy;
        }
    }

    section_title(ui, tr("Fournisseur", "Provider"));
    if crate::ai::panel::provider_picker(ui, &mut ai.provider, &info.ai_saved_keys) {
        state.key_draft.clear();
        actions.push(Action::RefreshModels);
    }
    crate::ai::panel::untested_notice(ui, ai.provider);
    if ai.provider == AiProvider::Custom {
        ui.add_space(4.0);
        label(ui, tr("Adresse du service", "Service address"));
        ui.add(
            egui::TextEdit::singleline(&mut ai.custom_url)
                .hint_text("http://localhost:11434/v1")
                .desired_width(f32::INFINITY),
        );
        note(ui, tr(
            "Ollama ou LM Studio sur cet ordinateur, ou un autre service compatible OpenAI.",
            "Ollama or LM Studio on this computer, or another OpenAI-compatible service.",
        ));
    }

    section_title(ui, tr("Clé API", "API key"));
    if info.ai_key_saved {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(tr("✔ Clé rangée dans le coffre de Windows", "✔ Key stored in the Windows vault"))
                    .size(12.0)
                    .color(OK_GREEN),
            );
            if ui.small_button(tr("Supprimer la clé", "Delete the key")).clicked() {
                actions.push(Action::DeleteApiKey);
            }
        });
    } else {
        crate::ai::panel::key_link(ui, ai.provider);
    }
    ui.horizontal(|ui| {
        let hint = if info.ai_key_saved {
            tr("Remplacer par une nouvelle clé", "Replace with a new key")
        } else {
            tr("Collez votre clé ici", "Paste your key here")
        };
        let edit = ui.add(
            egui::TextEdit::singleline(&mut state.key_draft)
                .password(true)
                .hint_text(hint)
                .desired_width((ui.available_width() - 110.0).max(120.0)),
        );
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let save = ui.add_enabled(!state.key_draft.trim().is_empty(), egui::Button::new(tr("Enregistrer", "Save")));
        if (save.clicked() || enter) && !state.key_draft.trim().is_empty() {
            actions.push(Action::SaveApiKey);
        }
    });
    note(ui, tr(
        "La clé est chiffrée par Windows pour votre compte. Elle n'est jamais écrite en clair sur le disque.",
        "Windows encrypts the key for your account. It is never written to disk in plain text.",
    ));

    section_title(ui, tr("Modèle", "Model"));
    let current = ai.model(ai.provider).to_string();
    let default_model = crate::ai::default_model(ai.provider);
    let shown = if current.is_empty() {
        if default_model.is_empty() {
            tr("(à choisir)", "(to choose)").to_string()
        } else {
            trf!("{} (par défaut)", "{} (default)", default_model)
        }
    } else {
        current.clone()
    };
    ui.horizontal_wrapped(|ui| {
        let mut chosen: Option<String> = None;
        egui::ComboBox::from_id_salt("ai_model")
            .selected_text(shown)
            .width(260.0)
            .height(320.0)
            .show_ui(ui, |ui| {
                if !default_model.is_empty()
                    && ui.selectable_label(current.is_empty(), trf!("{} (par défaut)", "{} (default)", default_model)).clicked()
                {
                    chosen = Some(String::new());
                }
                if let ModelList::Loaded(models) = &info.ai_models {
                    for model in models {
                        if model == default_model {
                            continue;
                        }
                        if ui.selectable_label(&current == model, model).clicked() {
                            chosen = Some(model.clone());
                        }
                    }
                }
            });
        if let Some(model) = chosen {
            ai.set_model(ai.provider, model);
        }
        match &info.ai_models {
            ModelList::Loading => {
                ui.add(egui::Spinner::new().size(12.0));
            }
            ModelList::Failed(reason) => {
                ui.label(RichText::new(reason).size(11.5).color(WARN));
            }
            _ => {}
        }
        if ui.small_button(tr("Actualiser la liste", "Refresh the list")).clicked() {
            actions.push(Action::RefreshModels);
        }
    });
    ui.horizontal(|ui| {
        note(ui, tr("Autre modèle :", "Other model:"));
        let mut typed = current.clone();
        let edit = egui::TextEdit::singleline(&mut typed)
            .hint_text(tr("nom exact du modèle", "exact model name"))
            .desired_width(220.0);
        if ui.add(edit).changed() {
            ai.set_model(ai.provider, typed.trim().to_string());
        }
    });

    section_title(ui, tr("Relais", "Fallback"));
    ui.checkbox(
        &mut ai.fallback,
        tr(
            "Si le fournisseur ne répond pas, essayer les autres dont la clé est enregistrée",
            "If the provider does not answer, try the others whose key is saved",
        ),
    );
    note(ui, tr(
        "Les offres gratuites saturent parfois (limite par minute ou par jour). Enregistrez plusieurs clés gratuites : Aerolis passe au suivant, les plus rapides d'abord, et indique sous la réponse qui a répondu.",
        "Free plans sometimes run out (per-minute or per-day limits). Save several free keys: Aerolis moves on to the next one, fastest first, and shows under the answer who replied.",
    ));

    section_title(ui, tr("Envoi des documents", "Sending documents"));
    ui.checkbox(
        &mut ai.send_scanned_pages,
        tr("Envoyer en image les pages scannées (sans texte)", "Send scanned pages (without text) as images"),
    );
    note(ui, tr(
        "L'IA peut alors lire les pages scannées. Au plus 20 pages par question, ce qui coûte plus cher qu'une page de texte.",
        "The AI can then read scanned pages. At most 20 pages per question; they cost more than a page of text.",
    ));

    section_title(ui, tr("Conversations", "Conversations"));
    ui.checkbox(
        &mut ai.keep_conversations,
        tr("Garder l'historique des conversations sur cet ordinateur", "Keep the conversation history on this computer"),
    );
    ui.horizontal_wrapped(|ui| {
        if state.confirm_clear_conversations {
            note(ui, &trf!(
                "Supprimer les {} conversations ?",
                "Delete the {} conversations?",
                info.ai_conversations
            ));
            if ui.small_button(tr("Oui, supprimer", "Yes, delete")).clicked() {
                actions.push(Action::ConfirmClearConversations);
                state.confirm_clear_conversations = false;
            }
            if ui.small_button(tr("Annuler", "Cancel")).clicked() {
                state.confirm_clear_conversations = false;
            }
        } else {
            let button = egui::Button::new(tr("Supprimer toutes les conversations", "Delete all conversations")).small();
            if ui.add_enabled(info.ai_conversations > 0, button).clicked() {
                state.confirm_clear_conversations = true;
            }
            if info.ai_conversations == 0 {
                note(ui, tr("Aucune conversation enregistrée.", "No saved conversation."));
            }
        }
    });
}

/// Proxy de confidentialité PrivAiTe : installation en un clic, puis interrupteur.
fn privacy_proxy(ui: &mut egui::Ui, ai: &mut crate::settings::AiPrefs, state: &mut PageState, actions: &mut Vec<Action>) {
    use crate::ai::privacy::{self, InstallState};
    section_title(ui, tr("Confidentialité renforcée (expérimental)", "Enhanced privacy (experimental)"));
    note(ui, tr(
        "PrivAiTe, un proxy libre qui tourne sur cet ordinateur, remplace les noms, e-mails, numéros de téléphone ou de carte et les secrets par des étiquettes (<PERSON_1>…) avant l'envoi, puis remet les vraies valeurs dans la réponse. La détection est automatique, pas infaillible. Dans ce mode, seul le texte est envoyé : ni images ni pages scannées.",
        "PrivAiTe, a free proxy running on this computer, replaces names, emails, phone or card numbers and secrets with labels (<PERSON_1>…) before sending, then puts the real values back in the answer. Detection is automatic, not foolproof. In this mode only text is sent: no images or scanned pages.",
    ));
    ui.add_space(4.0);
    match privacy::install_state() {
        InstallState::Running { step, label, detail } => {
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::Spinner::new().size(13.0));
                label_text(ui, &format!("{step}/{} · {label}", privacy::INSTALL_STEPS));
            });
            ui.add(egui::ProgressBar::new(step as f32 / (privacy::INSTALL_STEPS as f32 + 0.5)).desired_height(6.0));
            if !detail.is_empty() {
                ui.add(egui::Label::new(RichText::new(detail).size(10.5).color(MUTED)).truncate());
            }
            ui.horizontal(|ui| {
                if ui.small_button(tr("Annuler", "Cancel")).clicked() {
                    actions.push(Action::CancelPrivacyInstall);
                }
                note(ui, tr("Vous pouvez continuer à lire pendant ce temps.", "You can keep reading meanwhile."));
            });
            return;
        }
        InstallState::Failed(error) => {
            ui.label(RichText::new(error).size(11.5).color(WARN));
            ui.horizontal(|ui| {
                if ui.small_button(tr("Réessayer", "Try again")).clicked() {
                    actions.push(Action::InstallPrivacy);
                }
                if ui.small_button(tr("Voir le journal", "Show the log")).clicked() {
                    actions.push(Action::OpenPrivacyLog);
                }
            });
            return;
        }
        InstallState::Idle => {}
    }
    if !privacy::is_installed() {
        ai.privacy_proxy = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button(trf!("Installer PrivAiTe (environ {} Go)", "Install PrivAiTe (about {} GB)", privacy::DISK_SIZE)).clicked() {
                actions.push(Action::InstallPrivacy);
            }
            if ui.link(RichText::new(tr("Qu'est-ce que PrivAiTe ? ↗", "What is PrivAiTe? ↗")).size(11.5)).clicked() {
                let _ = std::process::Command::new("explorer").arg(privacy::PROJECT_URL).spawn();
            }
        });
        note(ui, tr(
            "Téléchargé une seule fois, depuis les sources officielles (Python, PrivAiTe, modèles de détection), dans le dossier d'Aerolis. Rien d'autre n'est modifié sur l'ordinateur.",
            "Downloaded once, from official sources (Python, PrivAiTe, detection models), into the Aerolis folder. Nothing else on the computer is changed.",
        ));
        return;
    }
    ui.checkbox(
        &mut ai.privacy_proxy,
        tr("Anonymiser ce qui est envoyé à l'IA (PrivAiTe)", "Anonymize what is sent to the AI (PrivAiTe)"),
    );
    note(ui, tr(
        "Le proxy démarre à l'ouverture de l'assistant (jusqu'à une minute) et s'arrête après 15 minutes sans question pour libérer la mémoire (environ 1,2 Go). Si le proxy ne répond pas, rien n'est envoyé.",
        "The proxy starts when the assistant opens (up to a minute) and stops after 15 minutes without a question to free memory (about 1.2 GB). If the proxy does not answer, nothing is sent.",
    ));
    ui.horizontal_wrapped(|ui| {
        let size = privacy::installed_size()
            .map(|s| trf!(" · {} Go", " · {} GB", if crate::i18n::is_en() { s } else { s.replace('.', ",") }))
            .unwrap_or_default();
        note(ui, &trf!("PrivAiTe {} installé{}", "PrivAiTe {} installed{}", privacy::PRIVAITE_VERSION, size));
        if state.confirm_uninstall_privacy {
            if ui.small_button(tr("Oui, désinstaller", "Yes, uninstall")).clicked() {
                actions.push(Action::UninstallPrivacy);
                state.confirm_uninstall_privacy = false;
            }
            if ui.small_button(tr("Annuler", "Cancel")).clicked() {
                state.confirm_uninstall_privacy = false;
            }
        } else if ui.small_button(tr("Désinstaller", "Uninstall")).clicked() {
            state.confirm_uninstall_privacy = true;
        }
        if ui.link(RichText::new(tr("Journal", "Log")).size(11.0)).clicked() {
            actions.push(Action::OpenPrivacyLog);
        }
    });
}

fn label_text(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(Color32::WHITE));
}

fn privacy(ui: &mut egui::Ui, prefs: &mut Preferences, state: &mut PageState, info: &PageInfo, actions: &mut Vec<Action>) {
    section_title(ui, tr("PDF ouverts récemment", "Recently opened PDFs"));
    ui.checkbox(&mut prefs.keep_history, tr("Garder la liste des PDF ouverts récemment", "Keep the list of recently opened PDFs"));
    note(ui, tr(
        "Elle sert à les retrouver avec Ctrl+T. Elle reste sur cet ordinateur.",
        "It lets you find them again with Ctrl+T. It stays on this computer.",
    ));
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if state.confirm_clear_history {
            note(ui, &trf!(
                "Effacer les {} documents de l'historique ?",
                "Clear the {} documents from the history?",
                info.history_len
            ));
            if ui.small_button(tr("Oui, effacer", "Yes, clear")).clicked() {
                actions.push(Action::ConfirmClearHistory);
                state.confirm_clear_history = false;
            }
            if ui.small_button(tr("Annuler", "Cancel")).clicked() {
                state.confirm_clear_history = false;
            }
        } else {
            let clear = egui::Button::new(tr("Effacer l'historique", "Clear the history")).small();
            if ui.add_enabled(info.history_len > 0, clear).clicked() {
                state.confirm_clear_history = true;
            }
            if info.history_len == 0 {
                note(ui, tr("L'historique est vide.", "The history is empty."));
            }
        }
    });
    section_title(ui, tr("Assistant IA", "AI assistant"));
    note(ui, tr(
        "Rien n'est envoyé tant que vous ne posez pas de question. Les conversations se gèrent dans la rubrique Assistant IA.",
        "Nothing is sent until you ask a question. Conversations are managed in the AI assistant section.",
    ));
    if crate::ai::privacy::VISIBLE {
        privacy_proxy(ui, &mut prefs.ai, state, actions);
    }
}

fn updates(ui: &mut egui::Ui, prefs: &mut Preferences, info: &PageInfo, actions: &mut Vec<Action>) {
    ui.add_space(4.0);
    note(ui, &trf!("Version installée : {}", "Installed version: {}", env!("CARGO_PKG_VERSION")));
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        use crate::updates::ManualCheck;
        let checking = info.update_check == ManualCheck::Checking;
        if ui
            .add_enabled(!checking, egui::Button::new(tr("Rechercher une mise à jour", "Check for updates")))
            .clicked()
        {
            actions.push(Action::CheckUpdatesNow);
        }
        match &info.update_check {
            ManualCheck::Idle => {}
            ManualCheck::Checking => {
                ui.add(egui::Spinner::new().size(12.0));
                note(ui, tr("Recherche…", "Checking…"));
            }
            ManualCheck::UpToDate => {
                ui.label(RichText::new(tr("✔ Aerolis est à jour.", "✔ Aerolis is up to date.")).size(12.0).color(OK_GREEN));
            }
            ManualCheck::Available(release) => {
                ui.label(
                    RichText::new(trf!("Aerolis {} est disponible.", "Aerolis {} is available.", release.version))
                        .size(12.0)
                        .color(Color32::WHITE),
                );
                if ui.small_button(tr("Voir la nouvelle version", "See the new version")).clicked() {
                    actions.push(Action::OpenNewRelease);
                }
            }
            ManualCheck::Failed(reason) => {
                ui.label(RichText::new(reason).size(12.0).color(WARN));
            }
        }
    });
    ui.add_space(8.0);
    ui.checkbox(&mut prefs.check_updates, tr("Vérifier automatiquement (une fois par jour)", "Check automatically (once a day)"));
    note(ui, tr(
        "Aerolis consulte la page des versions sur GitHub et vous prévient. Rien n'est installé sans vous.",
        "Aerolis looks at the releases page on GitHub and lets you know. Nothing is installed without you.",
    ));
}

fn help(ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    section_title(ui, tr("Prise en main", "Getting started"));
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("Raccourcis clavier (F1)", "Keyboard shortcuts (F1)")).clicked() {
            actions.push(Action::ShowShortcuts);
        }
        if ui.button(tr("Revoir l'accueil", "Show the welcome screen")).clicked() {
            actions.push(Action::ShowWelcome);
        }
    });
    section_title(ui, tr("À propos", "About"));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        note(ui, &trf!(
            "Aerolis {} · logiciel libre (licence MIT) ·",
            "Aerolis {} · free software (MIT license) ·",
            env!("CARGO_PKG_VERSION")
        ));
        if ui.link(RichText::new(tr("page du projet", "project page")).size(11.5)).clicked() {
            actions.push(Action::OpenProjectPage);
        }
    });
}
