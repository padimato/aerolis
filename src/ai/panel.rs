//! Assistant IA à l'écran : bouton rond en bas à droite (`Ctrl+J`) qui ouvre une fenêtre de discussion.
//!
//! La première ouverture demande le fournisseur et la clé API. Ensuite : choix des PDF ouverts comme
//! sources, questions avec images jointes (zone capturée sur la page, image collée avec `Ctrl+V` ou
//! glissée), boutons rapides (résumé, fiche de révision, QCM...), renvois de page cliquables et historique.

use super::history::{self, Conversation, ImageData, Message, SourceRef, Summary};
use super::net::{self, ChatEvent, ChatInput, ChatJob, Route};
use super::providers;
use super::sources::SourceSpec;
use super::{keystore, Turn};
use crate::i18n::{is_en, tr};
use crate::settings::{AiPrefs, AiProvider};
use crate::trf;
use egui::{Color32, CornerRadius, Margin, Pos2, Rect, RichText, Stroke, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

const PANEL_BG: Color32 = Color32::from_rgb(22, 26, 37);
const PANEL_STROKE: Color32 = Color32::from_rgb(70, 100, 170);
const ACCENT: Color32 = Color32::from_rgb(92, 128, 255);
const USER_BUBBLE: Color32 = Color32::from_rgb(44, 62, 110);
const MUTED: Color32 = Color32::from_rgb(150, 160, 182);
const WARN: Color32 = Color32::from_rgb(250, 200, 110);
const CHIP_ON: Color32 = Color32::from_rgb(48, 74, 140);
const CHIP_OFF: Color32 = Color32::from_rgb(36, 41, 56);
/// Diamètre du bouton rond
const BUTTON_SIZE: f32 = 52.0;
const MARGIN: f32 = 18.0;

/// Onglet ouvert, tel que l'assistant le voit.
#[derive(Debug, Clone)]
pub struct TabInfo {
    pub id: u64,
    pub title: String,
    pub path: PathBuf,
    /// Page affichée, à partir de 0
    pub current_page: usize,
}

/// Demandes faites à l'application.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelAction {
    /// Tracer un rectangle sur la page pour le joindre à la question
    StartCapture,
    /// Ouvrir le document et aller à la page (à partir de 1)
    OpenCitation { path: PathBuf, page: usize },
    OpenSettings,
    /// Envoyer le brouillon : l'application fournit les sources et appelle [`AiPanel::send`]
    Send,
}

/// Action rapide sur un texte sélectionné (clic droit sur la sélection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionAsk {
    Explain,
    Translate,
    Summarize,
    /// Le passage est mis dans la question, l'utilisateur écrit la suite
    Question,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Chat,
    History,
}

struct Running {
    job: ChatJob,
    status: String,
    text: String,
    notes: Vec<String>,
    sources: Vec<SourceRef>,
    /// Fournisseur qui répond (« Groq · modèle · 0,4 s »)
    via: Option<String>,
}

pub struct AiPanel {
    pub open: bool,
    view: View,
    pub conversation: Conversation,
    pub draft: String,
    pub draft_images: Vec<ImageData>,
    /// Onglets choisis comme sources. `None` : l'onglet affiché (tant que l'utilisateur n'a rien choisi).
    chosen_sources: Option<Vec<u64>>,
    running: Option<Running>,
    /// Tracé d'une zone à capturer en cours
    pub capture_mode: bool,
    textures: HashMap<u64, egui::TextureHandle>,
    history: Option<Vec<Summary>>,
    focus_input: bool,
    send_requested: bool,
    setup_key: String,
    setup_error: Option<String>,
    /// Présence d'une clé pour chaque fournisseur (lue une fois dans le coffre)
    key_known: HashMap<AiProvider, bool>,
    paste_down: bool,
    /// Enregistrer les conversations (faux pendant les captures automatisées)
    pub persistence: bool,
    /// Dossier des conversations
    history_dir: PathBuf,
    error_banner: Option<String>,
    /// Proxy de confidentialité déjà démarré en avance pour ces réglages
    privacy_prewarmed: Option<AiPrefs>,
}

impl Default for AiPanel {
    fn default() -> Self {
        Self {
            open: false,
            view: View::Chat,
            conversation: Conversation::new(),
            draft: String::new(),
            draft_images: Vec::new(),
            chosen_sources: None,
            running: None,
            capture_mode: false,
            textures: HashMap::new(),
            history: None,
            focus_input: false,
            send_requested: false,
            setup_key: String::new(),
            setup_error: None,
            key_known: HashMap::new(),
            paste_down: false,
            persistence: true,
            history_dir: history::dir(),
            error_banner: None,
            privacy_prewarmed: None,
        }
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Image PNG à partir de pixels RGBA, réduite à 2000 pixels de côté au plus.
pub fn png_from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Option<Vec<u8>> {
    let mut image = image::RgbaImage::from_raw(width, height, rgba)?;
    if width.max(height) > 2000 {
        let scale = 2000.0 / width.max(height) as f32;
        image = image::imageops::resize(
            &image,
            ((width as f32 * scale) as u32).max(1),
            ((height as f32 * scale) as u32).max(1),
            image::imageops::FilterType::Triangle,
        );
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}

/// Image jointe à partir d'un fichier PNG ou JPEG.
pub fn image_from_file(path: &std::path::Path) -> Option<ImageData> {
    let bytes = std::fs::read(path).ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (w, h) = decoded.dimensions();
    let png = png_from_rgba(w, h, decoded.into_raw())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some(ImageData { png, label: name, zone_text: None })
}

pub fn is_image_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["png", "jpg", "jpeg"].contains(&e.to_ascii_lowercase().as_str()))
}

/// Image du presse-papiers (capture faite avec Win+Maj+S, par exemple).
fn clipboard_image() -> Option<ImageData> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let image = clipboard.get_image().ok()?;
    let png = png_from_rgba(image.width as u32, image.height as u32, image.bytes.into_owned())?;
    Some(ImageData { png, label: tr("Image collée", "Pasted image").into(), zone_text: None })
}

/// `Ctrl+V` enfoncé (egui ne signale pas ce raccourci quand le presse-papiers ne contient qu'une image).
fn paste_shortcut_down() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        extern "system" {
            fn GetAsyncKeyState(key: i32) -> i16;
        }
        // SAFETY : simple lecture de l'état du clavier.
        unsafe { GetAsyncKeyState(0x11) < 0 && GetAsyncKeyState(0x56) < 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn quick_prompts(page: usize) -> [(&'static str, String); 5] {
    [
        (
            tr("📝 Résumer", "📝 Summarize"),
            tr(
                "Fais un résumé clair du document : les idées principales en quelques paragraphes courts.",
                "Write a clear summary of the document: the main ideas in a few short paragraphs.",
            )
            .into(),
        ),
        (
            tr("🔑 Points clés", "🔑 Key points"),
            tr("Quels sont les points clés à retenir ? Fais une liste.", "What are the key points to remember? Make a list.").into(),
        ),
        (
            tr("🗂 Fiche de révision", "🗂 Study sheet"),
            tr(
                "Fais une fiche de révision du document : définitions, formules et idées importantes, bien organisées.",
                "Make a study sheet of the document: definitions, formulas and important ideas, well organized.",
            )
            .into(),
        ),
        (
            tr("✅ QCM d'entraînement", "✅ Practice quiz"),
            tr(
                "Pose-moi un QCM de 5 questions sur le document pour m'entraîner. Donne les réponses à la fin, avec la page où trouver chaque réponse.",
                "Give me a 5-question multiple-choice quiz on the document to practice. Put the answers at the end, with the page where each answer is found.",
            )
            .into(),
        ),
        (
            tr("📄 Expliquer cette page", "📄 Explain this page"),
            trf!("Explique-moi la page {} simplement.", "Explain page {} to me simply.", page + 1),
        ),
    ]
}

/// Bouton rond avec une icône, dessiné à la main (cercle plein, pas de cadre de bouton).
fn round_icon_button(ui: &mut egui::Ui, icon: &str, size: f32, hint: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    let fill = if response.hovered() { Color32::from_rgb(48, 56, 78) } else { Color32::TRANSPARENT };
    ui.painter().circle_filled(rect.center(), size / 2.0, fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(size * 0.52), Color32::from_rgb(210, 218, 235));
    response.on_hover_text(hint)
}

impl AiPanel {
    /// `persistence` : enregistrer les conversations (faux pendant les captures automatisées).
    pub fn new(persistence: bool) -> Self {
        Self { persistence, ..Default::default() }
    }

    pub fn key_saved(&mut self, provider: AiProvider) -> bool {
        *self.key_known.entry(provider).or_insert_with(|| keystore::load(provider).is_some())
    }

    /// La clé vient d'être changée dans les Réglages : relire le coffre.
    pub fn forget_key_state(&mut self) {
        self.key_known.clear();
    }

    /// Clé considérée comme rangée sans passer par le coffre (captures de démonstration).
    pub fn pretend_key_saved(&mut self, provider: AiProvider) {
        self.key_known.insert(provider, true);
    }

    fn needs_setup(&mut self, prefs: &AiPrefs) -> bool {
        match prefs.provider {
            AiProvider::Custom => prefs.custom_url.trim().is_empty(),
            provider => !self.key_saved(provider),
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.focus_input = true;
        } else {
            self.capture_mode = false;
        }
    }

    /// Ouvre la fenêtre, curseur dans le champ de la question.
    pub fn open_and_focus(&mut self) {
        self.open = true;
        self.view = View::Chat;
        self.focus_input = true;
    }

    pub fn add_image(&mut self, image: ImageData) {
        self.draft_images.push(image);
        self.open_and_focus();
    }

    pub fn attach_files(&mut self, paths: &[PathBuf]) -> usize {
        let mut added = 0;
        for path in paths {
            if let Some(image) = image_from_file(path) {
                self.draft_images.push(image);
                added += 1;
            }
        }
        if added > 0 {
            self.open_and_focus();
        }
        added
    }

    /// Question sur un passage sélectionné dans le document.
    pub fn ask_about_selection(&mut self, kind: SelectionAsk, text: &str, page: usize) {
        let quote: String = text.trim().chars().take(4000).collect();
        let page = page + 1;
        self.draft = match kind {
            SelectionAsk::Explain => trf!(
                "Explique ce passage de la page {} :\n« {} »",
                "Explain this passage from page {}:\n“{}”",
                page,
                quote
            ),
            SelectionAsk::Translate => trf!(
                "Traduis ce passage de la page {} en français (en anglais s'il est déjà en français) :\n« {} »",
                "Translate this passage from page {} into English (into French if it is already in English):\n“{}”",
                page,
                quote
            ),
            SelectionAsk::Summarize => trf!(
                "Résume ce passage de la page {} :\n« {} »",
                "Summarize this passage from page {}:\n“{}”",
                page,
                quote
            ),
            SelectionAsk::Question => trf!("À propos de ce passage de la page {} :\n« {} »\n\n", "About this passage from page {}:\n“{}”\n\n", page, quote),
        };
        self.open_and_focus();
        if kind != SelectionAsk::Question {
            self.send_requested = true;
        }
    }

    /// Pose `question` dès la prochaine image (essais automatisés).
    pub fn ask(&mut self, question: &str) {
        self.draft = question.to_string();
        self.open = true;
        self.send_requested = true;
    }

    /// Démarre une nouvelle conversation (la précédente reste dans l'historique).
    pub fn new_conversation(&mut self) {
        if let Some(running) = self.running.take() {
            running.job.cancel.store(true, Ordering::Relaxed);
        }
        self.conversation = Conversation::new();
        self.view = View::Chat;
        self.focus_input = true;
        self.error_banner = None;
    }

    fn save_conversation(&mut self, prefs: &AiPrefs) {
        if self.persistence && prefs.keep_conversations && !self.conversation.is_empty() {
            self.conversation.updated = chrono::Local::now().timestamp();
            self.conversation.refresh_title();
            if history::save_in(&self.history_dir, &self.conversation).is_err() {
                self.error_banner = Some(tr("Conversation non enregistrée (dossier inaccessible).", "Conversation not saved (folder not accessible).").into());
            }
        }
    }

    pub fn conversation_count(&self) -> usize {
        history::count_in(&self.history_dir)
    }

    pub fn clear_history(&mut self) {
        history::clear_in(&self.history_dir);
        self.history = None;
    }

    /// Onglets sources parmi ceux ouverts.
    pub fn source_ids(&self, tabs: &[TabInfo], active: Option<u64>) -> Vec<u64> {
        match &self.chosen_sources {
            Some(chosen) => tabs.iter().filter(|t| chosen.contains(&t.id)).map(|t| t.id).collect(),
            None => active.filter(|id| tabs.iter().any(|t| t.id == *id)).into_iter().collect(),
        }
    }

    /// Envoie le brouillon avec les `sources` préparées par l'application.
    pub fn send(&mut self, sources: Vec<SourceSpec>, prefs: &AiPrefs, ctx: &egui::Context) {
        if self.running.is_some() {
            return;
        }
        let text = self.draft.trim().to_string();
        if text.is_empty() && self.draft_images.is_empty() {
            return;
        }
        let text = if text.is_empty() {
            tr("Que montre cette image ?", "What does this image show?").to_string()
        } else {
            text
        };
        self.conversation.messages.push(Message {
            user: true,
            text,
            images: std::mem::take(&mut self.draft_images),
            notes: Vec::new(),
            error: false,
            sources: Vec::new(),
            via: None,
        });
        self.draft.clear();
        self.conversation.refresh_title();
        self.error_banner = None;
        let turns: Vec<Turn> = self
            .conversation
            .messages
            .iter()
            .filter(|m| !m.error)
            .map(|m| {
                let mut text = m.text.clone();
                for image in &m.images {
                    if let Some(zone) = image.zone_text.as_deref().filter(|z| !z.trim().is_empty()) {
                        text.push_str(&format!("\n\n[{} — text found in this zone: \"{}\"]", image.label, zone.trim()));
                    }
                }
                Turn { user: m.user, text, images: m.images.iter().map(|i| Arc::new(i.png.clone())).collect() }
            })
            .collect();
        let source_refs = sources.iter().map(|s| SourceRef { title: s.title.clone(), path: s.path.clone() }).collect();
        let routes = routes(prefs);
        if prefs.provider == AiProvider::Custom && routes[0].model.is_empty() {
            self.finish_with_error(
                tr("Choisissez un modèle dans Réglages › Assistant IA.", "Choose a model in Settings › AI assistant.").into(),
                prefs,
            );
            return;
        }
        let input = ChatInput {
            privacy: super::privacy::active(prefs),
            routes,
            english: is_en(),
            sources,
            send_page_images: prefs.send_scanned_pages,
            turns,
        };
        let job = net::start_chat(input, ctx.clone());
        self.running = Some(Running {
            job,
            status: tr("Envoi…", "Sending…").into(),
            text: String::new(),
            notes: Vec::new(),
            sources: source_refs,
            via: None,
        });
    }

    fn finish_with_error(&mut self, message: String, prefs: &AiPrefs) {
        self.conversation.messages.push(Message {
            user: false,
            text: message,
            images: Vec::new(),
            notes: Vec::new(),
            error: true,
            sources: Vec::new(),
            via: None,
        });
        self.save_conversation(prefs);
    }

    /// Lit les nouvelles de la requête en cours.
    fn poll(&mut self, prefs: &AiPrefs) {
        let Some(running) = self.running.as_mut() else { return };
        let mut finished: Option<Result<Option<String>, String>> = None;
        while let Ok(event) = running.job.events.try_recv() {
            match event {
                ChatEvent::Status(status) => running.status = status,
                ChatEvent::Note(note) => running.notes.push(note),
                ChatEvent::Via(via) => running.via = Some(via),
                ChatEvent::Text(text) => running.text.push_str(&text),
                ChatEvent::Done { stop_reason } => finished = Some(Ok(stop_reason)),
                ChatEvent::Error(error) => finished = Some(Err(error)),
            }
        }
        let Some(outcome) = finished else { return };
        let running = self.running.take().expect("requête en cours");
        match outcome {
            Ok(stop_reason) => {
                let mut notes = running.notes;
                match stop_reason.as_deref() {
                    Some("max_tokens") => notes.push(tr("Réponse coupée : trop longue.", "Answer cut off: too long.").into()),
                    Some("refusal") => notes.push(tr("Le modèle a refusé de répondre à cette demande.", "The model declined this request.").into()),
                    Some("empty") => notes.push(tr("Le modèle n'a rien répondu.", "The model returned no answer.").into()),
                    _ => {}
                }
                self.conversation.messages.push(Message {
                    user: false,
                    text: running.text,
                    images: Vec::new(),
                    notes,
                    error: false,
                    sources: running.sources,
                    via: running.via.clone(),
                });
                self.save_conversation(prefs);
            }
            Err(error) => {
                if !running.text.is_empty() {
                    self.conversation.messages.push(Message {
                        user: false,
                        text: running.text,
                        images: Vec::new(),
                        notes: running.notes,
                        error: false,
                        sources: running.sources,
                        via: running.via.clone(),
                    });
                }
                self.finish_with_error(error, prefs);
            }
        }
    }

    fn stop(&mut self, prefs: &AiPrefs) {
        let Some(running) = self.running.take() else { return };
        running.job.cancel.store(true, Ordering::Relaxed);
        let mut notes = running.notes;
        notes.push(tr("Réponse interrompue.", "Answer stopped.").into());
        self.conversation.messages.push(Message {
            user: false,
            text: running.text,
            images: Vec::new(),
            notes,
            error: false,
            sources: running.sources,
            via: running.via,
        });
        self.save_conversation(prefs);
    }

    fn texture(&mut self, ctx: &egui::Context, png: &[u8]) -> Option<egui::TextureHandle> {
        let key = hash_bytes(png);
        if let Some(texture) = self.textures.get(&key) {
            return Some(texture.clone());
        }
        let image = image::load_from_memory(png).ok()?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        let texture = ctx.load_texture(format!("kite_ai_{key}"), color, egui::TextureOptions::LINEAR);
        if self.textures.len() > 64 {
            self.textures.clear();
        }
        self.textures.insert(key, texture.clone());
        Some(texture)
    }

    /// Bouton rond et fenêtre de l'assistant, au-dessus du document (`area` : zone du document).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        area: Rect,
        tabs: &[TabInfo],
        active: Option<u64>,
        prefs: &mut AiPrefs,
    ) -> Vec<PanelAction> {
        let mut actions = Vec::new();
        self.poll(prefs);
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::J)) {
            self.toggle();
        }
        if self.send_requested {
            self.send_requested = false;
            actions.push(PanelAction::Send);
        }
        if prefs.show_button || self.open {
            self.floating_button(ctx, area);
        }
        if !self.open {
            // À la prochaine ouverture, le proxy (peut-être arrêté pour inactivité) est relancé en avance.
            self.privacy_prewarmed = None;
            return actions;
        }
        // Mode confidentialité : le proxy démarre pendant que l'utilisateur écrit sa question.
        if super::privacy::active(prefs)
            && self.privacy_prewarmed.as_ref() != Some(prefs)
            && super::privacy::is_installed()
            && !self.needs_setup(prefs)
        {
            self.privacy_prewarmed = Some(prefs.clone());
            net::prewarm_privacy(routes(prefs), is_en());
        }
        // Image collée (Ctrl+V) pendant que la fenêtre est ouverte.
        let down = ctx.input(|i| i.focused) && paste_shortcut_down();
        if down && !self.paste_down {
            if let Some(image) = clipboard_image() {
                self.draft_images.push(image);
                self.focus_input = true;
            }
        }
        self.paste_down = down;

        let width = 430.0_f32.min(area.width() - 2.0 * MARGIN).max(260.0);
        let height = 660.0_f32.min(area.height() - BUTTON_SIZE - 3.0 * MARGIN).max(240.0);
        let origin = Pos2::new(area.right() - MARGIN - width, area.bottom() - 2.0 * MARGIN - BUTTON_SIZE - height + 6.0);
        egui::Area::new(egui::Id::new("kite_ai_panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(origin)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(PANEL_BG)
                    .stroke(Stroke::new(1.0_f32, PANEL_STROKE))
                    .corner_radius(CornerRadius::same(16))
                    .shadow(egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(140) })
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(width - 24.0);
                        ui.set_height(height - 24.0);
                        self.header(ui, prefs, &mut actions);
                        ui.add_space(4.0);
                        if self.needs_setup(prefs) {
                            self.setup(ui, prefs, &mut actions);
                        } else if self.view == View::History {
                            self.history_view(ui, prefs);
                        } else {
                            self.chat(ui, tabs, active, prefs, &mut actions);
                        }
                    });
            });
        actions
    }

    fn floating_button(&mut self, ctx: &egui::Context, area: Rect) {
        let center = Pos2::new(area.right() - MARGIN - BUTTON_SIZE / 2.0, area.bottom() - MARGIN - BUTTON_SIZE / 2.0);
        egui::Area::new(egui::Id::new("kite_ai_button"))
            .order(egui::Order::Foreground)
            .fixed_pos(center - Vec2::splat(BUTTON_SIZE / 2.0))
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(BUTTON_SIZE), egui::Sense::click());
                let painter = ui.painter();
                painter.circle_filled(rect.center() + Vec2::new(0.0, 3.0), BUTTON_SIZE / 2.0, Color32::from_black_alpha(90));
                let fill = if response.hovered() { Color32::from_rgb(112, 146, 255) } else { ACCENT };
                painter.circle_filled(rect.center(), BUTTON_SIZE / 2.0, fill);
                painter.circle_stroke(rect.center(), BUTTON_SIZE / 2.0 - 1.0, Stroke::new(1.5_f32, Color32::from_rgb(160, 186, 255)));
                let icon = if self.open { "✖" } else { "✨" };
                painter.text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(22.0), Color32::WHITE);
                if self.running.is_some() && !self.open {
                    // Réponse en cours, fenêtre fermée : petit point animé.
                    let t = ui.input(|i| i.time) as f32;
                    let dot = rect.right_top() + Vec2::new(-8.0, 8.0);
                    painter.circle_filled(dot, 5.0 + (t * 4.0).sin(), Color32::from_rgb(120, 220, 160));
                    ui.ctx().request_repaint();
                }
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let response = response.on_hover_text(tr("Assistant IA (Ctrl+J)", "AI assistant (Ctrl+J)"));
                if response.clicked() {
                    self.toggle();
                }
            });
    }

    fn header(&mut self, ui: &mut egui::Ui, prefs: &AiPrefs, actions: &mut Vec<PanelAction>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("✨").size(17.0).color(ACCENT));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(tr("Assistant", "Assistant")).size(15.5).strong().color(Color32::WHITE));
                let model = match prefs.model(prefs.provider) {
                    "" => super::default_model(prefs.provider),
                    chosen => chosen,
                };
                let model = if model.is_empty() { tr("modèle gratuit automatique", "automatic free model") } else { model };
                let lock = if super::privacy::active(prefs) { tr(" · 🔒 anonymisé", " · 🔒 anonymized") } else { "" };
                ui.label(RichText::new(format!("{} · {model}{lock}", providers::display_name(prefs.provider))).size(10.5).color(MUTED))
                    .on_hover_text(if super::privacy::active(prefs) {
                        tr(
                            "Proxy de confidentialité actif : noms, e-mails, numéros et secrets sont remplacés par des étiquettes avant l'envoi. Seul le texte est envoyé.",
                            "Privacy proxy on: names, emails, numbers and secrets are replaced with labels before sending. Only text is sent.",
                        )
                    } else {
                        ""
                    });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if round_icon_button(ui, "✖", 28.0, tr("Fermer (Ctrl+J)", "Close (Ctrl+J)")).clicked() {
                    self.open = false;
                    self.capture_mode = false;
                }
                if round_icon_button(ui, "⚙", 28.0, tr("Réglages de l'assistant", "Assistant settings")).clicked() {
                    actions.push(PanelAction::OpenSettings);
                }
                let history_hint = tr("Conversations précédentes", "Previous conversations");
                if round_icon_button(ui, "🕘", 28.0, history_hint).clicked() {
                    self.view = if self.view == View::History { View::Chat } else { View::History };
                    self.history = None;
                }
                if round_icon_button(ui, "➕", 28.0, tr("Nouvelle conversation", "New conversation")).clicked() {
                    self.new_conversation();
                }
            });
        });
        ui.add_space(2.0);
        let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().rect_filled(line, 0.0, Color32::from_rgb(44, 52, 72));
    }

    /// Premier lancement : fournisseur et clé API.
    fn setup(&mut self, ui: &mut egui::Ui, prefs: &mut AiPrefs, actions: &mut Vec<PanelAction>) {
        egui::ScrollArea::vertical().id_salt("ai_setup").show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(tr("Bienvenue !", "Welcome!")).size(17.0).strong().color(Color32::WHITE));
            ui.add_space(2.0);
            ui.label(
                RichText::new(tr(
                    "Posez des questions sur vos PDF, demandez un résumé, une fiche de révision ou l'explication d'un passage. Il faut une clé API d'un fournisseur d'IA : plusieurs en offrent une gratuite.",
                    "Ask questions about your PDFs, get a summary, a study sheet or the explanation of a passage. You need an API key from an AI provider: several offer a free one.",
                ))
                .size(12.5)
                .color(Color32::from_rgb(205, 212, 228)),
            );
            ui.add_space(12.0);
            ui.label(RichText::new(tr("1. Fournisseur", "1. Provider")).size(13.0).strong().color(Color32::WHITE));
            let known = self.saved_keys();
            if provider_picker(ui, &mut prefs.provider, &known) {
                self.setup_error = None;
            }
            ui.add_space(10.0);
            if prefs.provider == AiProvider::Custom {
                ui.label(RichText::new(tr("2. Adresse du service", "2. Service address")).size(13.0).strong().color(Color32::WHITE));
                ui.add(egui::TextEdit::singleline(&mut prefs.custom_url).hint_text("http://localhost:11434/v1").desired_width(f32::INFINITY));
                ui.add_space(10.0);
            }
            let step = if prefs.provider == AiProvider::Custom { "3" } else { "2" };
            ui.label(RichText::new(trf!("{}. Clé API", "{}. API key", step)).size(13.0).strong().color(Color32::WHITE));
            key_link(ui, prefs.provider);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.setup_key)
                    .password(true)
                    .hint_text(tr("Collez votre clé ici", "Paste your key here"))
                    .desired_width(f32::INFINITY),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(8.0);
            let key = self.setup_key.trim().to_string();
            let ready = !key.is_empty() || prefs.provider == AiProvider::Custom && !prefs.custom_url.trim().is_empty();
            let start = ui.add_enabled(
                ready,
                egui::Button::new(RichText::new(tr("Commencer", "Get started")).size(13.5).color(Color32::WHITE))
                    .fill(ACCENT)
                    .min_size(Vec2::new(140.0, 32.0))
                    .corner_radius(8.0),
            );
            if (start.clicked() || enter) && ready {
                if key.is_empty() {
                    self.key_known.insert(prefs.provider, false);
                } else {
                    match keystore::save(prefs.provider, &key) {
                        Ok(()) => {
                            self.key_known.insert(prefs.provider, true);
                            self.setup_key.clear();
                            self.focus_input = true;
                        }
                        Err(e) => {
                            self.setup_error = Some(trf!("Clé non enregistrée : {}", "Key not saved: {}", e));
                        }
                    }
                }
            }
            if let Some(error) = &self.setup_error {
                ui.label(RichText::new(error).size(12.0).color(WARN));
            }
            ui.add_space(10.0);
            untested_notice(ui, prefs.provider);
            ui.add_space(8.0);
            ui.label(
                RichText::new(tr(
                    "🔒 La clé est rangée dans le coffre de Windows, chiffrée pour votre compte. Le texte des PDF choisis comme sources n'est envoyé qu'au moment où vous posez une question. Astuce : enregistrez plusieurs clés gratuites (Réglages › Assistant IA) ; si un fournisseur est saturé, Kite essaie le suivant.",
                    "🔒 The key is kept in the Windows vault, encrypted for your account. The text of the PDFs chosen as sources is only sent when you ask a question. Tip: save several free keys (Settings › AI assistant); if one provider is busy, Kite tries the next.",
                ))
                .size(11.5)
                .color(MUTED),
            );
            ui.add_space(4.0);
            if ui.link(RichText::new(tr("Plus de réglages…", "More settings…")).size(11.5)).clicked() {
                actions.push(PanelAction::OpenSettings);
            }
        });
    }

    /// Fournisseurs dont une clé est rangée (lue une fois dans le coffre).
    pub fn saved_keys(&mut self) -> Vec<AiProvider> {
        AiProvider::ALL.into_iter().filter(|p| self.key_saved(*p)).collect()
    }

    fn history_view(&mut self, ui: &mut egui::Ui, prefs: &AiPrefs) {
        if self.history.is_none() {
            self.history = Some(history::list_in(&self.history_dir));
        }
        ui.horizontal(|ui| {
            if ui.small_button(tr("← Retour", "← Back")).clicked() {
                self.view = View::Chat;
            }
            ui.label(RichText::new(tr("Conversations", "Conversations")).size(14.0).strong().color(Color32::WHITE));
        });
        ui.add_space(4.0);
        if !prefs.keep_conversations {
            ui.label(
                RichText::new(tr(
                    "L'historique est désactivé (Réglages › Assistant IA).",
                    "History is turned off (Settings › AI assistant).",
                ))
                .size(11.5)
                .color(MUTED),
            );
        }
        let list = self.history.clone().unwrap_or_default();
        if list.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr("Aucune conversation enregistrée.", "No saved conversation.")).color(MUTED));
            });
            return;
        }
        let mut open: Option<String> = None;
        let mut delete: Option<String> = None;
        egui::ScrollArea::vertical().id_salt("ai_history").show(ui, |ui| {
            for summary in &list {
                let current = summary.id == self.conversation.id;
                egui::Frame::NONE
                    .fill(if current { CHIP_ON } else { Color32::from_rgb(30, 35, 49) })
                    .corner_radius(8.0)
                    .inner_margin(Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let date = chrono::DateTime::from_timestamp(summary.updated, 0)
                                .map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string())
                                .unwrap_or_default();
                            let title = if summary.title.is_empty() { tr("(sans titre)", "(untitled)").to_string() } else { summary.title.clone() };
                            let response = ui
                                .vertical(|ui| {
                                    ui.set_width(ui.available_width() - 34.0);
                                    ui.add(egui::Label::new(RichText::new(title).size(12.5).color(Color32::WHITE)).truncate());
                                    ui.label(
                                        RichText::new(format!("{date} · {}", crate::i18n::plural(summary.message_count, "message", "messages")))
                                            .size(10.5)
                                            .color(MUTED),
                                    );
                                })
                                .response
                                .interact(egui::Sense::click());
                            if response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if response.clicked() {
                                open = Some(summary.id.clone());
                            }
                            if round_icon_button(ui, "🗑", 26.0, tr("Supprimer", "Delete")).clicked() {
                                delete = Some(summary.id.clone());
                            }
                        });
                    });
                ui.add_space(4.0);
            }
        });
        if let Some(id) = delete {
            history::delete_in(&self.history_dir, &id);
            if id == self.conversation.id {
                self.conversation = Conversation::new();
            }
            self.history = None;
        }
        if let Some(id) = open {
            if let Some(conversation) = history::load_in(&self.history_dir, &id) {
                if let Some(running) = self.running.take() {
                    running.job.cancel.store(true, Ordering::Relaxed);
                }
                self.conversation = conversation;
                self.view = View::Chat;
            }
        }
    }

    fn chat(&mut self, ui: &mut egui::Ui, tabs: &[TabInfo], active: Option<u64>, prefs: &mut AiPrefs, actions: &mut Vec<PanelAction>) {
        // Sources : un bouton par PDF ouvert.
        let selected = self.source_ids(tabs, active);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            ui.label(RichText::new(tr("Sources :", "Sources:")).size(11.5).color(MUTED));
            if tabs.is_empty() {
                ui.label(RichText::new(tr("aucun PDF ouvert", "no PDF open")).size(11.5).color(MUTED));
            }
            for tab in tabs {
                let on = selected.contains(&tab.id);
                let mut title: String = tab.title.chars().take(22).collect();
                if tab.title.chars().count() > 22 {
                    title.push('…');
                }
                let text = RichText::new(format!("{} {title}", if on { "✔" } else { "+" }))
                    .size(11.5)
                    .color(if on { Color32::WHITE } else { Color32::from_rgb(180, 188, 205) });
                let chip = egui::Button::new(text)
                    .fill(if on { CHIP_ON } else { CHIP_OFF })
                    .stroke(Stroke::new(1.0_f32, if on { ACCENT } else { Color32::from_rgb(56, 64, 84) }))
                    .corner_radius(12.0);
                let response = ui.add(chip).on_hover_text(tab.path.display().to_string());
                if response.clicked() {
                    let mut chosen = selected.clone();
                    if on {
                        chosen.retain(|id| *id != tab.id);
                    } else {
                        chosen.push(tab.id);
                    }
                    self.chosen_sources = Some(chosen);
                }
            }
        });
        ui.add_space(4.0);

        // Zone de saisie en bas, messages au-dessus.
        let input_height = 92.0 + if self.draft_images.is_empty() { 0.0 } else { 62.0 };
        let messages_height = (ui.available_height() - input_height).max(80.0);
        let multi = selected.len() > 1;
        let mut quick: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("ai_messages")
            .max_height(messages_height)
            .min_scrolled_height(messages_height)
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if self.conversation.messages.is_empty() && self.running.is_none() {
                    quick = self.empty_state(ui, tabs, active, &selected);
                }
                let messages = self.conversation.messages.clone();
                for message in &messages {
                    ui.add_space(6.0);
                    if message.user {
                        self.user_bubble(ui, message);
                    } else if let Some((page, document)) = self.assistant_message(ui, message, message.sources.len() > 1) {
                        if let Some(source) = message.sources.get(document.saturating_sub(1)) {
                            actions.push(PanelAction::OpenCitation { path: source.path.clone(), page });
                        }
                    }
                }
                if let Some(running) = &self.running {
                    ui.add_space(6.0);
                    if running.text.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(14.0).color(ACCENT));
                            ui.label(RichText::new(&running.status).size(12.0).color(MUTED));
                        });
                    } else {
                        let _ = super::markdown::show(ui, &running.text, multi);
                        ui.add(egui::Spinner::new().size(12.0).color(ACCENT));
                    }
                    for note in &running.notes {
                        ui.label(RichText::new(format!("↪ {note}")).size(10.5).color(MUTED));
                    }
                }
                if let Some(error) = &self.error_banner {
                    ui.label(RichText::new(error).size(11.5).color(WARN));
                }
            });
        if let Some(prompt) = quick {
            self.draft = prompt;
            actions.push(PanelAction::Send);
        }
        self.input_area(ui, prefs, actions);
    }

    /// Conversation vide : présentation et boutons rapides.
    fn empty_state(&mut self, ui: &mut egui::Ui, tabs: &[TabInfo], active: Option<u64>, selected: &[u64]) -> Option<String> {
        let mut chosen = None;
        ui.add_space(14.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("✨").size(30.0).color(ACCENT));
            ui.label(RichText::new(tr("Que voulez-vous savoir ?", "What would you like to know?")).size(15.0).strong().color(Color32::WHITE));
            let hint = if selected.is_empty() {
                tr(
                    "Aucune source choisie : l'assistant répondra sans document.",
                    "No source chosen: the assistant will answer without a document.",
                )
            } else {
                tr(
                    "Les réponses citent les pages : cliquez sur « p. 12 » pour y aller.",
                    "Answers cite pages: click “p. 12” to go there.",
                )
            };
            ui.label(RichText::new(hint).size(11.5).color(MUTED));
        });
        if selected.is_empty() {
            return None;
        }
        ui.add_space(12.0);
        let page = active.and_then(|id| tabs.iter().find(|t| t.id == id)).map_or(0, |t| t.current_page);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            for (label, prompt) in quick_prompts(page) {
                let button = egui::Button::new(RichText::new(label).size(12.0).color(Color32::from_rgb(220, 226, 240)))
                    .fill(Color32::from_rgb(32, 38, 54))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(60, 72, 100)))
                    .corner_radius(10.0)
                    .min_size(Vec2::new(0.0, 28.0));
                if ui.add(button).clicked() {
                    chosen = Some(prompt);
                }
            }
        });
        ui.add_space(12.0);
        ui.label(
            RichText::new(tr(
                "Astuces : ✂ capture une zone de la page pour poser une question dessus. Clic droit sur un texte sélectionné pour l'expliquer ou le traduire. Ctrl+V colle une capture d'écran.",
                "Tips: ✂ captures a zone of the page to ask about it. Right-click selected text to explain or translate it. Ctrl+V pastes a screenshot.",
            ))
            .size(11.0)
            .color(MUTED),
        );
        chosen
    }

    fn thumbnails(&mut self, ui: &mut egui::Ui, images: &[ImageData], removable: bool) -> Option<usize> {
        let mut removed = None;
        ui.horizontal_wrapped(|ui| {
            for (i, image) in images.iter().enumerate() {
                let Some(texture) = self.texture(ui.ctx(), &image.png) else { continue };
                let size = texture.size_vec2();
                let scale = (56.0 / size.y).min(120.0 / size.x).min(1.0);
                let response = ui
                    .add(egui::Image::new(&texture).fit_to_exact_size(size * scale).corner_radius(6.0).sense(egui::Sense::click()))
                    .on_hover_ui(|ui| {
                        ui.label(&image.label);
                        let big = (360.0 / size.x).min(360.0 / size.y).min(1.0);
                        ui.add(egui::Image::new(&texture).fit_to_exact_size(size * big));
                    });
                if removable {
                    let corner = response.rect.right_top() + Vec2::new(-8.0, 8.0);
                    let close = Rect::from_center_size(corner, Vec2::splat(16.0));
                    let hit = ui.interact(close, ui.id().with(("remove_image", i)), egui::Sense::click());
                    ui.painter().circle_filled(corner, 8.0, Color32::from_black_alpha(200));
                    ui.painter().text(corner, egui::Align2::CENTER_CENTER, "✖", egui::FontId::proportional(10.0), Color32::WHITE);
                    if hit.clicked() {
                        removed = Some(i);
                    }
                }
            }
        });
        removed
    }

    fn user_bubble(&mut self, ui: &mut egui::Ui, message: &Message) {
        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
            let max = ui.available_width() * 0.86;
            egui::Frame::NONE
                .fill(USER_BUBBLE)
                .corner_radius(CornerRadius { nw: 12, ne: 12, sw: 12, se: 3 })
                .inner_margin(Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.set_max_width(max);
                    if !message.images.is_empty() {
                        self.thumbnails(ui, &message.images, false);
                    }
                    ui.add(egui::Label::new(RichText::new(&message.text).size(13.0).color(Color32::WHITE)).wrap());
                });
        });
    }

    /// Réponse de l'assistant ; renvoie la page et le numéro du document d'un renvoi cliqué.
    fn assistant_message(&mut self, ui: &mut egui::Ui, message: &Message, multi: bool) -> Option<(usize, usize)> {
        if message.error {
            egui::Frame::NONE
                .fill(Color32::from_rgb(62, 40, 30))
                .corner_radius(8.0)
                .inner_margin(Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("⚠ {}", message.text)).size(12.0).color(WARN));
                });
            return None;
        }
        let clicked = super::markdown::show(ui, &message.text, multi);
        for note in &message.notes {
            ui.label(RichText::new(format!("↪ {note}")).size(10.5).color(MUTED));
        }
        ui.horizontal(|ui| {
            if let Some(via) = &message.via {
                ui.label(RichText::new(via).size(10.5).color(Color32::from_rgb(110, 120, 140)))
                    .on_hover_text(tr("Fournisseur, modèle et temps avant le premier mot", "Provider, model and time to the first word"));
            }
            if ui
                .add(egui::Button::new(RichText::new(tr("📋 Copier", "📋 Copy")).size(10.5).color(MUTED)).frame(false))
                .on_hover_text(tr("Copier la réponse", "Copy the answer"))
                .clicked()
            {
                ui.ctx().copy_text(message.text.clone());
            }
        });
        clicked.map(|c| (c.page, c.document))
    }

    fn input_area(&mut self, ui: &mut egui::Ui, prefs: &AiPrefs, actions: &mut Vec<PanelAction>) {
        ui.add_space(4.0);
        if !self.draft_images.is_empty() {
            let images = self.draft_images.clone();
            if let Some(removed) = self.thumbnails(ui, &images, true) {
                self.draft_images.remove(removed);
            }
        }
        egui::Frame::NONE
            .fill(Color32::from_rgb(30, 35, 49))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(60, 72, 100)))
            .corner_radius(12.0)
            .inner_margin(Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let edit = egui::TextEdit::multiline(&mut self.draft)
                    .id(egui::Id::new("kite_ai_input"))
                    .hint_text(tr("Posez une question sur le document…", "Ask a question about the document…"))
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .frame(false)
                    .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter));
                let response = ui.add(edit);
                if self.focus_input {
                    response.request_focus();
                    self.focus_input = false;
                }
                let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let capture = round_icon_button(
                        ui,
                        "✂",
                        28.0,
                        tr("Capturer une zone de la page (Échap pour annuler)", "Capture a zone of the page (Esc to cancel)"),
                    );
                    if capture.clicked() {
                        self.capture_mode = !self.capture_mode;
                        if self.capture_mode {
                            actions.push(PanelAction::StartCapture);
                        }
                    }
                    if self.capture_mode {
                        ui.painter().circle_stroke(capture.rect.center(), 13.0, Stroke::new(1.5_f32, ACCENT));
                    }
                    if round_icon_button(ui, "📎", 28.0, tr("Joindre une image (PNG, JPEG)", "Attach an image (PNG, JPEG)")).clicked() {
                        if let Some(paths) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg"]).pick_files() {
                            self.attach_files(&paths);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.running.is_some() {
                            let stop = egui::Button::new(RichText::new(tr("■ Arrêter", "■ Stop")).size(12.0).color(Color32::WHITE))
                                .fill(Color32::from_rgb(120, 60, 60))
                                .corner_radius(14.0);
                            if ui.add(stop).clicked() {
                                self.stop(prefs);
                            }
                        } else {
                            let can_send = !self.draft.trim().is_empty() || !self.draft_images.is_empty();
                            let send = egui::Button::new(RichText::new("➡").size(15.0).color(Color32::WHITE))
                                .fill(if can_send { ACCENT } else { Color32::from_rgb(50, 58, 80) })
                                .corner_radius(14.0)
                                .min_size(Vec2::new(34.0, 28.0));
                            let send = ui.add_enabled(can_send, send).on_hover_text(tr("Envoyer (Entrée)", "Send (Enter)"));
                            if (send.clicked() || enter) && can_send {
                                actions.push(PanelAction::Send);
                            }
                        }
                    });
                });
            });
    }

    /// Conversation de démonstration pour les captures (QA) : question avec zone capturée et réponse.
    pub fn load_demo(&mut self, ctx: &egui::Context, zone_png: Option<Vec<u8>>, source: SourceRef) {
        let _ = ctx;
        let english = is_en();
        let mut images = Vec::new();
        if let Some(png) = zone_png {
            images.push(ImageData { png, label: tr("Zone de la page 1", "Zone of page 1").into(), zone_text: None });
        }
        self.conversation = Conversation::new();
        self.conversation.messages.push(Message {
            user: true,
            text: if english { "What does this passage mean? Explain it simply.".into() } else { "Que veut dire ce passage ? Explique-le simplement.".into() },
            images,
            notes: vec![],
            error: false,
            sources: vec![],
            via: None,
        });
        let answer = if english {
            "This passage explains **why silicon is studied for battery anodes** [p. 1].

- **Much higher capacity** than the graphite used today, so more energy in the same space [p. 1].
- **The catch**: silicon swells a lot while charging, which cracks the electrode over time [p. 2].
- **The fixes reviewed**: nanostructures, composites with carbon and new binders [p. 3].

In short: silicon promises longer-lasting batteries, if its swelling can be tamed."
        } else {
            "Ce passage explique **pourquoi le silicium intéresse pour les anodes de batteries** [p. 1].

- **Capacité bien plus grande** que le graphite utilisé aujourd'hui : plus d'énergie dans le même volume [p. 1].
- **Le problème** : le silicium gonfle beaucoup pendant la charge, ce qui fissure l'électrode avec le temps [p. 2].
- **Les solutions étudiées** : nanostructures, composites avec du carbone et nouveaux liants [p. 3].

En bref : le silicium promet des batteries plus endurantes, à condition de maîtriser son gonflement."
        };
        self.conversation.messages.push(Message {
            user: false,
            text: answer.into(),
            images: vec![],
            notes: vec![],
            error: false,
            sources: vec![source],
            via: None,
        });
        self.conversation.refresh_title();
        self.open = true;
        self.draft = if english { "Which solution works best?".into() } else { "Quelle solution marche le mieux ?".into() };
    }
}

/// Fournisseurs à essayer : celui choisi, puis (relais activé) ceux dont la clé est rangée, les plus
/// rapides d'abord. Le service personnalisé n'est jamais un relais (il peut être éteint).
pub fn routes(prefs: &AiPrefs) -> Vec<Route> {
    let route = |provider: AiProvider| Route {
        provider,
        model: match prefs.model(provider) {
            "" => super::default_model(provider).to_string(),
            chosen => chosen.to_string(),
        },
        base_url: super::base_url(provider, &prefs.custom_url),
        api_key: keystore::load(provider),
    };
    let mut routes = vec![route(prefs.provider)];
    if prefs.fallback {
        let mut others: Vec<AiProvider> = AiProvider::ALL
            .into_iter()
            .filter(|p| *p != prefs.provider && *p != AiProvider::Custom && keystore::load(*p).is_some())
            .collect();
        others.sort_by_key(|p| providers::info(*p).fallback_rank);
        routes.extend(others.into_iter().map(route));
    }
    routes
}

/// Liste des fournisseurs, regroupés (gratuits, payants, autre). Renvoie vrai si le choix a changé.
pub fn provider_picker(ui: &mut egui::Ui, provider: &mut AiProvider, saved_keys: &[AiProvider]) -> bool {
    let before = *provider;
    let groups: [(&str, Vec<AiProvider>); 3] = [
        (
            tr("Offre gratuite", "Free plan"),
            AiProvider::ALL.into_iter().filter(|p| providers::info(*p).free).collect(),
        ),
        (tr("Payants", "Paid"), vec![AiProvider::Anthropic, AiProvider::OpenAi]),
        (tr("Autre", "Other"), vec![AiProvider::Custom]),
    ];
    for (title, list) in groups {
        ui.label(RichText::new(title).size(10.5).color(MUTED));
        for p in list {
            let info = providers::info(p);
            ui.horizontal(|ui| {
                ui.radio_value(provider, p, providers::display_name(p));
                if p == AiProvider::Gemini {
                    ui.label(RichText::new(tr("recommandé", "recommended")).size(10.0).color(Color32::from_rgb(120, 210, 150)));
                }
                if !info.tested {
                    ui.label(RichText::new(tr("non testé", "not tested")).size(10.0).color(WARN))
                        .on_hover_text(tr(
                            "Pas encore essayé par l'équipe Kite PDF avec une vraie clé.",
                            "Not yet tried by the Kite PDF team with a real key.",
                        ));
                }
                if saved_keys.contains(&p) {
                    ui.label(RichText::new(tr("✔ clé", "✔ key")).size(10.0).color(Color32::from_rgb(120, 210, 150)));
                }
            });
            if *provider == p {
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.add(egui::Label::new(RichText::new(providers::pitch(p)).size(11.0).color(MUTED)).wrap());
                });
            }
        }
        ui.add_space(4.0);
    }
    *provider != before
}

/// Lien vers la page où créer une clé.
pub fn key_link(ui: &mut egui::Ui, provider: AiProvider) {
    let info = providers::info(provider);
    if info.key_url.is_empty() {
        ui.label(
            RichText::new(tr("Celle donnée par le service, s'il en demande une.", "The one given by the service, if it requires one."))
                .size(11.5)
                .color(MUTED),
        );
        return;
    }
    let text = if info.free {
        trf!("Obtenir une clé gratuite sur {} ↗", "Get a free key from {} ↗", info.name)
    } else {
        trf!("Obtenir une clé sur {} ↗", "Get a key from {} ↗", info.name)
    };
    if ui.link(RichText::new(text).size(11.5)).on_hover_text(info.key_url).clicked() {
        let _ = std::process::Command::new("explorer").arg(info.key_url).spawn();
    }
}

/// Fournisseur pas encore essayé : on le dit, et on invite à faire un retour ou une contribution.
pub fn untested_notice(ui: &mut egui::Ui, provider: AiProvider) {
    if providers::info(provider).tested {
        return;
    }
    egui::Frame::NONE
        .fill(Color32::from_rgb(54, 46, 28))
        .corner_radius(8.0)
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(tr(
                    "⚠ Ce fournisseur n'a pas encore été testé par l'équipe Kite PDF : nous n'avions pas de clé. Kite PDF est open source : si vous l'essayez, dites-nous si ça marche. Une contribution (pull request) pour améliorer l'assistant est la bienvenue !",
                    "⚠ This provider has not been tested by the Kite PDF team yet: we had no key. Kite PDF is open source: if you try it, tell us whether it works. A contribution (pull request) to improve the assistant is welcome!",
                ))
                .size(11.5)
                .color(Color32::from_rgb(240, 214, 160)),
            );
            if ui.link(RichText::new(tr("Donner un retour sur GitHub ↗", "Give feedback on GitHub ↗")).size(11.5)).clicked() {
                let _ = std::process::Command::new("explorer").arg(providers::FEEDBACK_URL).spawn();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_png_from_rgba_downscales_large_images() {
        let png = png_from_rgba(3000, 1000, vec![200; 3000 * 1000 * 4]).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2000, 666));
        assert!(png_from_rgba(10, 10, vec![0; 3]).is_none());
    }

    #[test]
    fn test_sources_follow_active_tab_until_chosen() {
        let tabs = vec![
            TabInfo { id: 1, title: "a".into(), path: "a.pdf".into(), current_page: 0 },
            TabInfo { id: 2, title: "b".into(), path: "b.pdf".into(), current_page: 3 },
        ];
        let mut panel = AiPanel { persistence: false, ..Default::default() };
        assert_eq!(panel.source_ids(&tabs, Some(2)), vec![2]);
        panel.chosen_sources = Some(vec![1, 2, 9]);
        assert_eq!(panel.source_ids(&tabs, Some(2)), vec![1, 2]);
    }

    #[test]
    fn test_selection_question_prefills_draft() {
        let mut panel = AiPanel { persistence: false, ..Default::default() };
        panel.ask_about_selection(SelectionAsk::Explain, "  La loi d'Ohm  ", 4);
        assert!(panel.draft.contains("La loi d'Ohm"));
        assert!(panel.draft.contains('5'));
        assert!(panel.send_requested && panel.open);
        let mut panel = AiPanel { persistence: false, ..Default::default() };
        panel.ask_about_selection(SelectionAsk::Question, "U = RI", 0);
        assert!(!panel.send_requested);
    }
}
