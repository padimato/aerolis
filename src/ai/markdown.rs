//! Affichage des réponses en Markdown simple : titres, listes, **gras**, *italique*, `code`, blocs de code,
//! et renvois de page (« [p. 12] », « [2, p. 5] ») affichés comme des pastilles cliquables.

use super::{parse_citation, Citation};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Sense, Stroke};

const TEXT: Color32 = Color32::from_rgb(222, 228, 240);
const STRONG: Color32 = Color32::WHITE;
const CODE_BG: Color32 = Color32::from_rgb(40, 46, 62);
const CITATION_BG: Color32 = Color32::from_rgb(38, 62, 112);
const CITATION_FG: Color32 = Color32::from_rgb(160, 196, 255);

#[derive(Debug, Clone, PartialEq)]
enum Block {
    Heading(u8, String),
    /// Profondeur, repère (« • » ou « 3. »), texte
    Item(usize, String, String),
    Paragraph(String),
    Code(String),
    Rule,
}

fn parse_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut code: Option<String> = None;
    let flush = |paragraph: &mut String, blocks: &mut Vec<Block>| {
        if !paragraph.trim().is_empty() {
            blocks.push(Block::Paragraph(paragraph.trim().to_string()));
        }
        paragraph.clear();
    };
    for raw in text.lines() {
        if raw.trim_start().starts_with("```") {
            match code.take() {
                Some(content) => blocks.push(Block::Code(content.trim_end().to_string())),
                None => {
                    flush(&mut paragraph, &mut blocks);
                    code = Some(String::new());
                }
            }
            continue;
        }
        if let Some(content) = code.as_mut() {
            content.push_str(raw);
            content.push('\n');
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut paragraph, &mut blocks);
            continue;
        }
        if line.starts_with('#') {
            let level = line.chars().take_while(|c| *c == '#').count();
            let rest = line[level..].trim();
            if level <= 6 && (line.len() == level || line.as_bytes()[level] == b' ') {
                flush(&mut paragraph, &mut blocks);
                blocks.push(Block::Heading(level as u8, rest.to_string()));
                continue;
            }
        }
        if line == "---" || line == "***" || line == "___" {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Rule);
            continue;
        }
        let bullet = ["- ", "* ", "+ ", "• "].iter().find_map(|m| line.strip_prefix(m));
        if let Some(rest) = bullet {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Item(indent / 2, "•".into(), rest.to_string()));
            continue;
        }
        let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && digits.len() <= 3 {
            let after = &line[digits.len()..];
            if let Some(rest) = after.strip_prefix(". ").or_else(|| after.strip_prefix(") ")) {
                flush(&mut paragraph, &mut blocks);
                blocks.push(Block::Item(indent / 2, format!("{digits}."), rest.to_string()));
                continue;
            }
        }
        // Ligne de tableau : gardée telle quelle, en police à chasse fixe.
        if line.starts_with('|') {
            flush(&mut paragraph, &mut blocks);
            if !line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                blocks.push(Block::Code(line.to_string()));
            }
            continue;
        }
        if !paragraph.is_empty() {
            paragraph.push('\n');
        }
        paragraph.push_str(line);
    }
    if let Some(content) = code {
        blocks.push(Block::Code(content.trim_end().to_string()));
    }
    flush(&mut paragraph, &mut blocks);
    blocks
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    code: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Span {
    Text(String, Style),
    Citation(Citation, String),
}

/// Découpe une ligne en morceaux de texte mis en forme et en renvois de page.
fn parse_inline(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut style = Style::default();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let push = |current: &mut String, spans: &mut Vec<Span>, style: Style| {
        if !current.is_empty() {
            spans.push(Span::Text(std::mem::take(current), style));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            push(&mut current, &mut spans, style);
            style.code = !style.code;
            i += 1;
            continue;
        }
        if style.code {
            current.push(c);
            i += 1;
            continue;
        }
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            push(&mut current, &mut spans, style);
            style.bold = !style.bold;
            i += 2;
            continue;
        }
        if c == '*' || (c == '_' && (i == 0 || !chars[i - 1].is_alphanumeric())) || (c == '_' && style.italic) {
            // Un astérisque isolé entouré d'espaces reste un astérisque.
            let next = chars.get(i + 1).copied().unwrap_or(' ');
            if style.italic || !next.is_whitespace() {
                push(&mut current, &mut spans, style);
                style.italic = !style.italic;
                i += 1;
                continue;
            }
        }
        if c == '[' {
            if let Some(len) = chars[i + 1..].iter().position(|&ch| ch == ']') {
                let inner: String = chars[i + 1..i + 1 + len].iter().collect();
                // Plusieurs renvois dans les mêmes crochets : « [p. 3; p. 7] », « [p. 3, 5, 13] ».
                if let Some(citations) = parse_citation_list(&inner) {
                    push(&mut current, &mut spans, style);
                    for (citation, part) in citations {
                        spans.push(Span::Citation(citation, part));
                    }
                    i += len + 2;
                    continue;
                }
            }
        }
        current.push(c);
        i += 1;
    }
    push(&mut current, &mut spans, style);
    spans
}

/// Exposants et indices Unicode (absents de la police) écrits « 10^-5 », « Li^+ », « H_2O ».
/// ¹ ² ³, présents dans la police, restent tels quels.
fn plain_exponents(text: &str) -> String {
    const SUP: [(char, char); 13] = [
        ('⁰', '0'), ('⁴', '4'), ('⁵', '5'), ('⁶', '6'), ('⁷', '7'), ('⁸', '8'), ('⁹', '9'),
        ('⁺', '+'), ('⁻', '-'), ('⁼', '='), ('⁽', '('), ('⁾', ')'), ('ⁿ', 'n'),
    ];
    let sup = |c: char| -> Option<char> {
        SUP.iter().find(|(s, _)| *s == c).map(|(_, a)| *a).or(match c {
            '¹' => Some('1'),
            '²' => Some('2'),
            '³' => Some('3'),
            _ => None,
        })
    };
    let sub = |c: char| -> Option<char> {
        ('₀'..='₉').contains(&c).then(|| char::from(b'0' + (c as u32 - '₀' as u32) as u8)).or(match c {
            '₊' => Some('+'),
            '₋' => Some('-'),
            _ => None,
        })
    };
    let chars: Vec<char> = text.chars().collect();
    if !chars.iter().any(|c| SUP.iter().any(|(s, _)| s == c) || sub(*c).is_some()) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        // Suite d'exposants contenant au moins un caractère absent de la police.
        let run: Vec<char> = chars[i..].iter().take_while(|c| sup(**c).is_some()).copied().collect();
        if !run.is_empty() && run.iter().any(|c| !matches!(c, '¹' | '²' | '³')) {
            out.push('^');
            out.extend(run.iter().filter_map(|c| sup(*c)));
            i += run.len();
            continue;
        }
        let run: Vec<char> = chars[i..].iter().take_while(|c| sub(**c).is_some()).copied().collect();
        if !run.is_empty() {
            out.push('_');
            out.extend(run.iter().filter_map(|c| sub(*c)));
            i += run.len();
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Liste de renvois entre crochets : « p. 3; 2, p. 7 », « p. 3, 5, 13 », « p. 4, p. 13 ».
/// Tous les morceaux doivent être des renvois, sinon ce n'en est pas une liste (« [voir note] »).
fn parse_citation_list(inner: &str) -> Option<Vec<(Citation, String)>> {
    let mut out = Vec::new();
    for segment in inner.split(';') {
        if let Some(citation) = parse_citation(segment) {
            out.push((citation, segment.trim().to_string()));
            continue;
        }
        // « p. 3, 5, 13 » : pages du même document.
        let mut document = None;
        for piece in segment.split(',') {
            let piece = piece.trim();
            if let Some(citation) = parse_citation(piece) {
                document = Some(citation.document);
                out.push((citation, piece.to_string()));
            } else {
                let digits: String = piece.chars().take_while(|c| c.is_ascii_digit()).collect();
                let rest = piece[digits.len()..].trim();
                let page = digits.parse::<usize>().ok().filter(|p| *p > 0)?;
                if !rest.is_empty() && !rest.starts_with(['-', '–', '—']) {
                    return None;
                }
                out.push((Citation { document: document?, page }, piece.to_string()));
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Texte d'une pastille de renvoi.
fn citation_label(citation: Citation, multi: bool) -> String {
    if multi {
        format!("{} · p. {}", citation.document, citation.page)
    } else {
        format!("p. {}", citation.page)
    }
}

/// Paragraphe mis en forme ; renvoie le renvoi cliqué.
fn paragraph(ui: &mut egui::Ui, text: &str, size: f32, color: Color32, width: f32, multi: bool) -> Option<Citation> {
    let mut job = LayoutJob { wrap: egui::text::TextWrapping::wrap_at_width(width), ..Default::default() };
    let mut ranges: Vec<(std::ops::Range<usize>, Citation)> = Vec::new();
    let mut count = 0usize;
    for span in parse_inline(text) {
        match span {
            Span::Text(text, style) => {
                let font = if style.code { FontId::monospace(size - 1.0) } else { FontId::proportional(size) };
                let format = TextFormat {
                    font_id: font,
                    color: if style.bold { STRONG } else { color },
                    italics: style.italic,
                    background: if style.code { CODE_BG } else { Color32::TRANSPARENT },
                    ..Default::default()
                };
                count += text.chars().count();
                job.append(&text, 0.0, format);
            }
            Span::Citation(citation, _) => {
                let label = format!("\u{2009}{}\u{2009}", citation_label(citation, multi));
                let len = label.chars().count();
                let format = TextFormat {
                    font_id: FontId::proportional(size - 1.5),
                    color: CITATION_FG,
                    background: CITATION_BG,
                    valign: egui::Align::Center,
                    ..Default::default()
                };
                job.append(" ", 0.0, TextFormat { font_id: FontId::proportional(size), ..Default::default() });
                count += 1;
                ranges.push((count..count + len, citation));
                count += len;
                job.append(&label, 0.0, format);
            }
        }
    }
    let galley = ui.fonts(|f| f.layout_job(job));
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    let mut clicked = None;
    if let Some(pos) = response.hover_pos() {
        let index = galley.cursor_from_pos(pos - rect.min).ccursor.index;
        if let Some((range, citation)) = ranges.iter().find(|(r, _)| r.contains(&index)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            // Soulignement de la pastille survolée.
            let start = galley.pos_from_ccursor(egui::text::CCursor::new(range.start));
            let end = galley.pos_from_ccursor(egui::text::CCursor::new(range.end));
            if (start.min.y - end.min.y).abs() < 1.0 {
                let y = rect.min.y + start.max.y;
                ui.painter().line_segment(
                    [egui::pos2(rect.min.x + start.min.x, y), egui::pos2(rect.min.x + end.min.x, y)],
                    Stroke::new(1.0_f32, CITATION_FG),
                );
            }
            if response.clicked() {
                clicked = Some(*citation);
            }
        }
    }
    ui.painter().galley(rect.min, galley, color);
    clicked
}

/// Affiche `text` sur la largeur disponible. `multi` : plusieurs documents sources (le numéro du document
/// est montré dans les renvois). Renvoie le renvoi de page cliqué.
pub fn show(ui: &mut egui::Ui, text: &str, multi: bool) -> Option<Citation> {
    let width = ui.available_width();
    let mut clicked = None;
    let text = plain_exponents(text);
    let blocks = parse_blocks(&text);
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            ui.add_space(5.0);
        }
        let result = match block {
            Block::Heading(level, text) => {
                let size = match level {
                    1 => 16.5,
                    2 => 15.0,
                    _ => 14.0,
                };
                ui.add_space(3.0);
                paragraph(ui, &format!("**{text}**"), size, STRONG, width, multi)
            }
            Block::Paragraph(text) => paragraph(ui, text, 13.0, TEXT, width, multi),
            Block::Item(depth, marker, text) => {
                let indent = 6.0 + 16.0 * *depth as f32;
                let marker_width = if marker == "•" { 12.0 } else { 20.0 };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(indent);
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(marker_width, 17.0), Sense::hover());
                    ui.painter().text(
                        rect.left_top() + egui::vec2(0.0, 1.0),
                        egui::Align2::LEFT_TOP,
                        marker,
                        FontId::proportional(13.0),
                        CITATION_FG,
                    );
                    paragraph(ui, text, 13.0, TEXT, (width - indent - marker_width).max(60.0), multi)
                })
                .inner
            }
            Block::Code(code) => {
                egui::Frame::NONE
                    .fill(CODE_BG)
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        ui.label(egui::RichText::new(code).monospace().size(12.0).color(TEXT));
                    });
                None
            }
            Block::Rule => {
                ui.separator();
                None
            }
        };
        clicked = clicked.or(result);
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocks() {
        let text = "# Titre\nUn paragraphe\nsur deux lignes.\n\n- point **un**\n  - sous-point\n2. second\n```\ncode\n```\n---";
        let blocks = parse_blocks(text);
        assert_eq!(blocks[0], Block::Heading(1, "Titre".into()));
        assert_eq!(blocks[1], Block::Paragraph("Un paragraphe\nsur deux lignes.".into()));
        assert_eq!(blocks[2], Block::Item(0, "•".into(), "point **un**".into()));
        assert_eq!(blocks[3], Block::Item(1, "•".into(), "sous-point".into()));
        assert_eq!(blocks[4], Block::Item(0, "2.".into(), "second".into()));
        assert_eq!(blocks[5], Block::Code("code".into()));
        assert_eq!(blocks[6], Block::Rule);
    }

    #[test]
    fn test_inline_styles_and_citations() {
        let spans = parse_inline("Le **gain** vaut `Av` [p. 4] et [2, p. 7; 2, p. 9], voir [note].");
        assert_eq!(spans[0], Span::Text("Le ".into(), Style::default()));
        assert_eq!(spans[1], Span::Text("gain".into(), Style { bold: true, ..Default::default() }));
        assert_eq!(spans[3], Span::Text("Av".into(), Style { code: true, ..Default::default() }));
        assert_eq!(spans[5], Span::Citation(Citation { document: 1, page: 4 }, "p. 4".into()));
        assert_eq!(spans[7], Span::Citation(Citation { document: 2, page: 7 }, "2, p. 7".into()));
        assert_eq!(spans[8], Span::Citation(Citation { document: 2, page: 9 }, "2, p. 9".into()));
        assert_eq!(spans[9], Span::Text(", voir [note].".into(), Style::default()));
        let list = parse_inline("[p. 3, 5, 13] et [p. 4, p. 13] mais pas [1, 2]");
        let pages: Vec<usize> = list.iter().filter_map(|s| match s { Span::Citation(c, _) => Some(c.page), _ => None }).collect();
        assert_eq!(pages, vec![3, 5, 13, 4, 13]);
        assert_eq!(plain_exponents("10⁻⁵ S/cm, Li⁺, cm²/s, H₂O, 10¹⁴"), "10^-5 S/cm, Li^+, cm²/s, H_2O, 10^14");
        // Un astérisque isolé n'ouvre pas d'italique.
        assert_eq!(parse_inline("2 * 3"), vec![Span::Text("2 * 3".into(), Style::default())]);
        assert_eq!(parse_inline("snake_case_name"), vec![Span::Text("snake_case_name".into(), Style::default())]);
    }
}
