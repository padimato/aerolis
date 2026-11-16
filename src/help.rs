//! Aide : accueil du premier lancement et aide-mémoire des raccourcis (`F1`).

use egui::{Color32, CornerRadius, Margin, RichText, Stroke, Vec2};

const CARD_FILL: Color32 = Color32::from_rgb(24, 28, 39);
const CARD_STROKE: Color32 = Color32::from_rgb(70, 100, 170);
const MUTED: Color32 = Color32::from_rgb(150, 160, 182);
const KEY_FILL: Color32 = Color32::from_rgb(44, 50, 66);

/// Raccourcis par thème : (touches, action).
pub fn shortcuts() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
    (
        crate::i18n::tr("Fichiers", "Files"),
        vec![
            ("Ctrl+O", crate::i18n::tr("Ouvrir un PDF (ou le glisser ici)", "Open a PDF (or drag it here)")),
            ("Ctrl+S", crate::i18n::tr("Enregistrer", "Save")),
            (crate::i18n::tr("Ctrl+Maj+S", "Ctrl+Shift+S"), crate::i18n::tr("Enregistrer une copie", "Save a copy")),
            ("Ctrl+P", crate::i18n::tr("Imprimer", "Print")),
        ],
    ),
    (
        crate::i18n::tr("Fenêtre et onglets", "Window and tabs"),
        vec![
            ("Ctrl+T", crate::i18n::tr("PDF récents, page, zoom ou outil", "Recent PDFs, page, zoom or tool")),
            (crate::i18n::tr("Ctrl+Maj+T", "Ctrl+Shift+T"), crate::i18n::tr("Rouvrir l'onglet fermé", "Reopen the closed tab")),
            ("Ctrl+W", crate::i18n::tr("Fermer l'onglet", "Close the tab")),
            ("Ctrl+Tab", crate::i18n::tr("Onglet suivant (Maj : précédent)", "Next tab (Shift: previous)")),
            ("Ctrl+1 … 8", crate::i18n::tr("Aller à l'onglet n°", "Go to tab no.")),
            ("Ctrl+B", crate::i18n::tr("Garder la barre latérale ouverte", "Keep the sidebar open")),
            ("Ctrl+J", crate::i18n::tr("Assistant IA", "AI assistant")),
            ("Ctrl+,", crate::i18n::tr("Réglages", "Settings")),
            ("F1", crate::i18n::tr("Cette aide", "This help")),
        ],
    ),
    (
        crate::i18n::tr("Lecture", "Reading"),
        vec![
            ("Ctrl+F", crate::i18n::tr("Chercher un mot", "Find a word")),
            (crate::i18n::tr("Flèches", "Arrows"), crate::i18n::tr("Faire défiler", "Scroll")),
            (crate::i18n::tr("Pg préc. / Pg suiv.", "Pg Up / Pg Down"), crate::i18n::tr("Écran précédent / suivant", "Previous / next screen")),
            (crate::i18n::tr("Début / Fin", "Home / End"), crate::i18n::tr("Première / dernière page", "First / last page")),
            (crate::i18n::tr("Ctrl+molette", "Ctrl+wheel"), crate::i18n::tr("Zoomer (ou pincer)", "Zoom (or pinch)")),
            ("Ctrl+9 / Ctrl+0", crate::i18n::tr("Ajuster à la largeur / à la page", "Fit width / page")),
            (crate::i18n::tr("Espace + glisser", "Space + drag"), crate::i18n::tr("Déplacer la page", "Move the page")),
        ],
    ),
    (
        crate::i18n::tr("Annotations", "Annotations"),
        vec![
            ("V", crate::i18n::tr("Sélectionner, copier du texte", "Select, copy text")),
            ("H", crate::i18n::tr("Surligner", "Highlight")),
            ("T", crate::i18n::tr("Écrire du texte", "Write text")),
            ("D", crate::i18n::tr("Dessiner", "Draw")),
            ("R", crate::i18n::tr("Flèche ou rectangle", "Arrow or rectangle")),
            ("N", crate::i18n::tr("Commentaire", "Comment")),
            ("M", crate::i18n::tr("Mesurer", "Measure")),
            ("Ctrl+Z / Ctrl+Y", crate::i18n::tr("Annuler / rétablir", "Undo / redo")),
            (crate::i18n::tr("Suppr", "Del"), crate::i18n::tr("Supprimer l'annotation choisie", "Delete the chosen annotation")),
        ],
    ),
]
}

/// Choix fait sur l'écran d'accueil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WelcomeAction {
    OpenFile,
    Start,
    ShowShortcuts,
}

fn key_badge(ui: &mut egui::Ui, keys: &str) {
    egui::Frame::NONE
        .fill(KEY_FILL)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(keys).monospace().size(11.5).color(Color32::WHITE));
        });
}

fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(CARD_FILL)
        .stroke(Stroke::new(1.0_f32, CARD_STROKE))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(22))
}

/// Écran d'accueil du premier lancement. Renvoie le choix fait, s'il y en a un.
pub fn show_welcome(ctx: &egui::Context, logo: Option<&egui::TextureHandle>) -> Option<WelcomeAction> {
    let mut action = None;
    let width = 520.0_f32.min(ctx.screen_rect().width() - 32.0);
    let modal = egui::Modal::new(egui::Id::new("aerolis_welcome")).frame(card_frame()).show(ctx, |ui| {
        ui.set_width(width - 44.0);
        ui.horizontal(|ui| {
            if let Some(logo) = logo {
                ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(40.0)));
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(crate::i18n::tr("Bienvenue dans Aerolis", "Welcome to Aerolis")).size(20.0).strong().color(Color32::WHITE));
                ui.label(RichText::new(crate::i18n::tr("Quelques gestes pour bien démarrer", "A few gestures to get started")).size(12.5).color(MUTED));
            });
        });
        ui.add_space(14.0);
        let tips: [(&str, &str, &str); 5] = [
            (crate::i18n::tr("Ouvrir un PDF", "Open a PDF"), "Ctrl+O", crate::i18n::tr("ou glissez le fichier dans la fenêtre.", "or drag the file into the window.")),
            (crate::i18n::tr("Tout retrouver", "Find everything"), "Ctrl+T", crate::i18n::tr("vos PDF récents, une page (« 12 »), un zoom (« 150 % ») ou un outil (« surligner »).", "your recent PDFs, a page (“12”), a zoom (“150 %”) or a tool (“highlight”).")),
            (crate::i18n::tr("Chercher un mot", "Find a word"), "Ctrl+F", crate::i18n::tr("y compris dans le texte des images.", "including in the text of images.")),
            (crate::i18n::tr("Annoter", "Annotate"), "H  T  D  N", crate::i18n::tr("surligner, écrire, dessiner, commenter. Ctrl+S enregistre dans le PDF.", "highlight, write, draw, comment. Ctrl+S saves into the PDF.")),
            (crate::i18n::tr("Barre latérale", "Sidebar"), "Ctrl+B", crate::i18n::tr("onglets, outils et sommaire, au bord gauche de la fenêtre.", "tabs, tools and contents, at the left edge of the window.")),
        ];
        for (title, keys, text) in tips {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(title).size(13.0).strong().color(Color32::WHITE));
                key_badge(ui, keys);
                ui.label(RichText::new(text).size(12.5).color(MUTED));
            });
            ui.add_space(6.0);
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.link(RichText::new(crate::i18n::tr("Tous les raccourcis (F1)", "All shortcuts (F1)")).size(12.0)).clicked() {
                action = Some(WelcomeAction::ShowShortcuts);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let start = egui::Button::new(RichText::new(crate::i18n::tr("Commencer", "Get started")).size(13.5).color(Color32::WHITE))
                    .fill(Color32::from_rgb(52, 92, 190))
                    .min_size(Vec2::new(110.0, 32.0));
                if ui.add(start).clicked() {
                    action = Some(WelcomeAction::Start);
                }
                let open = egui::Button::new(RichText::new(crate::i18n::tr("Ouvrir un PDF…", "Open a PDF…")).size(13.5)).min_size(Vec2::new(0.0, 32.0));
                if ui.add(open).clicked() {
                    action = Some(WelcomeAction::OpenFile);
                }
            });
        });
    });
    if action.is_none() && modal.should_close() {
        action = Some(WelcomeAction::Start);
    }
    action
}

/// Aide-mémoire des raccourcis. Renvoie `false` quand il faut le fermer.
pub fn show_shortcuts(ctx: &egui::Context) -> bool {
    let mut keep_open = true;
    let screen = ctx.screen_rect();
    // Trois colonnes sur un grand écran, deux sur un écran moyen, une sur une fenêtre étroite.
    let columns = if screen.width() >= 1480.0 { 3 } else if screen.width() >= 960.0 { 2 } else { 1 };
    let width = match columns {
        3 => 1400.0,
        2 => 920.0,
        _ => (screen.width() - 32.0).min(480.0),
    };
    let modal = egui::Modal::new(egui::Id::new("aerolis_shortcuts")).frame(card_frame()).show(ctx, |ui| {
        ui.set_width(width - 44.0);
        ui.set_max_width(width - 44.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::tr("Raccourcis clavier", "Keyboard shortcuts")).size(18.0).strong().color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(crate::i18n::tr("Fermer", "Close")).clicked() {
                    keep_open = false;
                }
            });
        });
        ui.add_space(10.0);
        let max_height = (screen.height() - 160.0).max(200.0);
        // Sans hauteur minimale, la zone se réduirait à la place laissée au premier affichage.
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .min_scrolled_height(max_height.min(600.0))
            .show(ui, |ui| {
            let shortcuts = shortcuts();
            let group = |ui: &mut egui::Ui, (title, entries): &(&str, Vec<(&str, &str)>)| {
                ui.label(RichText::new(*title).size(13.5).strong().color(Color32::from_rgb(130, 170, 255)));
                ui.add_space(2.0);
                egui::Grid::new(("aerolis_shortcuts_grid", *title)).num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                    for (keys, action) in entries.iter() {
                        key_badge(ui, keys);
                        ui.label(RichText::new(*action).size(12.5).color(MUTED));
                        ui.end_row();
                    }
                });
                ui.add_space(12.0);
            };
            // Groupes répartis dans l'ordre : les deux premiers (courts) partagent la première colonne.
            let layout: &[&[usize]] = match columns {
                3 => &[&[0, 1], &[2], &[3]],
                2 => &[&[0, 1], &[2, 3]],
                _ => &[&[0, 1, 2, 3]],
            };
            ui.columns(layout.len(), |cols| {
                for (col, groups) in cols.iter_mut().zip(layout) {
                    for &g in *groups {
                        group(col, &shortcuts[g]);
                    }
                }
            });
        });
        ui.label(RichText::new(crate::i18n::tr("F1 ouvre et ferme cette aide · Échap la ferme", "F1 opens and closes this help · Esc closes it")).size(11.0).color(MUTED));
    });
    if modal.should_close() {
        keep_open = false;
    }
    keep_open
}
