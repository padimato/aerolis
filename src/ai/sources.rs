//! Texte des documents choisis comme sources, préparé pour l'IA.
//!
//! Le texte de chaque page est lu une fois par fichier (puis gardé en mémoire tant que le fichier ne
//! change pas). Une page sans texte prend le texte reconnu par l'OCR s'il y en a, sinon elle peut être
//! envoyée en image (pages scannées). Un document trop long est réduit aux pages proches de celle affichée,
//! et l'utilisateur en est prévenu.

use super::ContextBlock;
use crate::i18n::tr;
use crate::pdf_engine::PdfEngine;
use crate::trf;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Nombre maximal de pages scannées envoyées en image par question.
pub const MAX_PAGE_IMAGES: usize = 20;
/// Largeur des images de pages scannées, en pixels.
const PAGE_IMAGE_WIDTH: f32 = 1200.0;

/// Un document choisi comme source, décrit par l'interface.
#[derive(Debug, Clone)]
pub struct SourceSpec {
    pub title: String,
    pub path: PathBuf,
    /// Taille affichée de chaque page, en points (rotation comprise)
    pub page_sizes: Vec<(f32, f32)>,
    /// Texte reconnu par l'OCR, pour les pages qui n'ont pas de texte dans le fichier
    pub ocr_text: HashMap<usize, String>,
    /// Page affichée (à partir de 0) : au centre de l'extrait si le document est trop long
    pub focus_page: usize,
}

#[derive(Debug, Clone)]
enum PageContent {
    Text(String),
    /// Page sans texte
    Empty,
}

type CacheKey = (PathBuf, Option<SystemTime>, usize);
static TEXT_CACHE: Mutex<Option<HashMap<CacheKey, Arc<Vec<PageContent>>>>> = Mutex::new(None);

/// Fins de ligne normalisées, caractères de contrôle de PDFium retirés.
fn clean_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .map(|c| if c == '\r' { '\n' } else { c })
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

/// Texte de toutes les pages du fichier (lu une seule fois par version du fichier).
fn read_pages(spec: &SourceSpec) -> Result<Arc<Vec<PageContent>>, String> {
    let modified = std::fs::metadata(&spec.path).and_then(|m| m.modified()).ok();
    let key = (spec.path.clone(), modified, spec.page_sizes.len());
    if let Some(found) = TEXT_CACHE.lock().ok().and_then(|c| c.as_ref().and_then(|m| m.get(&key).cloned())) {
        return Ok(found);
    }
    let engine = PdfEngine::instance();
    let started = std::time::Instant::now();
    let doc = engine.open_lightweight(&spec.path).map_err(|e| e.to_string())?;
    let pages: Vec<PageContent> = (0..spec.page_sizes.len())
        .map(|page| {
            let text = clean_text(&doc.extract_page_plain_text(page));
            if text.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
                PageContent::Empty
            } else {
                PageContent::Text(text)
            }
        })
        .collect();
    {
        let _lock = PdfEngine::lock_guard();
        drop(doc);
    }
    if std::env::var_os("AEROLIS_TIMING").is_some() {
        eprintln!("[ai] texte de {} pages lu en {:.0} ms", pages.len(), started.elapsed().as_secs_f64() * 1000.0);
    }
    let pages = Arc::new(pages);
    if let Ok(mut cache) = TEXT_CACHE.lock() {
        let map = cache.get_or_insert_with(HashMap::new);
        // Peu de documents à la fois : on garde les plus récents.
        if map.len() > 12 {
            map.clear();
        }
        map.insert(key, pages.clone());
    }
    Ok(pages)
}

/// Image JPEG d'une page entière.
fn render_page_jpeg(spec: &SourceSpec, page: usize) -> Option<Vec<u8>> {
    let (w_pt, h_pt) = *spec.page_sizes.get(page)?;
    let width = PAGE_IMAGE_WIDTH;
    let height = (width * h_pt / w_pt.max(1.0)).min(2000.0);
    let engine = PdfEngine::instance();
    let doc = engine.open_lightweight(&spec.path).ok()?;
    let image = doc.render_page(page, width as i32, height as i32).ok();
    {
        let _lock = PdfEngine::lock_guard();
        drop(doc);
    }
    let image = image?;
    let rgb: Vec<u8> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80);
    encoder
        .encode(&rgb, image.size[0] as u32, image.size[1] as u32, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(out)
}

/// Pages à garder quand le document dépasse `budget` caractères : d'abord la page affichée, puis
/// alternativement celles d'après et d'avant. Renvoie les pages gardées, dans l'ordre.
fn pages_within_budget(lengths: &[usize], focus: usize, budget: usize) -> Vec<usize> {
    let count = lengths.len();
    if count == 0 {
        return Vec::new();
    }
    let focus = focus.min(count - 1);
    let mut kept = vec![focus];
    let mut used = lengths[focus];
    let (mut before, mut after) = (focus as isize - 1, focus + 1);
    loop {
        let mut progressed = false;
        for take_after in [true, false] {
            let candidate = if take_after {
                (after < count).then_some(after)
            } else {
                (before >= 0).then_some(before as usize)
            };
            let Some(page) = candidate else { continue };
            if used + lengths[page] > budget {
                return {
                    kept.sort_unstable();
                    kept
                };
            }
            used += lengths[page];
            kept.push(page);
            if take_after {
                after += 1;
            } else {
                before -= 1;
            }
            progressed = true;
        }
        if !progressed {
            break;
        }
    }
    kept.sort_unstable();
    kept
}

/// Pages envoyées et remarques à montrer à l'utilisateur.
pub struct PreparedContext {
    pub blocks: Vec<ContextBlock>,
    pub notes: Vec<String>,
}

/// Prépare le contexte de tous les documents sources (à appeler hors du fil de l'interface).
/// `max_chars` : taille maximale du texte envoyé, tous documents confondus (selon l'offre du fournisseur).
pub fn prepare(specs: &[SourceSpec], send_page_images: bool, max_chars: usize) -> Result<PreparedContext, String> {
    let mut texts: Vec<Vec<PageContent>> = Vec::new();
    for spec in specs {
        let mut pages = (*read_pages(spec)?).clone();
        for (page, text) in &spec.ocr_text {
            if let Some(slot @ PageContent::Empty) = pages.get_mut(*page) {
                if !text.trim().is_empty() {
                    *slot = PageContent::Text(text.trim().to_string());
                }
            }
        }
        texts.push(pages);
    }
    let total: usize = texts
        .iter()
        .flatten()
        .map(|p| match p {
            PageContent::Text(t) => t.len(),
            PageContent::Empty => 0,
        })
        .sum();
    let mut notes = Vec::new();
    let mut blocks = Vec::new();
    let mut images_left = if send_page_images { MAX_PAGE_IMAGES } else { 0 };
    let mut skipped_images = 0usize;
    let mut sent_images = 0usize;
    for (index, (spec, pages)) in specs.iter().zip(&texts).enumerate() {
        let lengths: Vec<usize> = pages
            .iter()
            .map(|p| match p {
                PageContent::Text(t) => t.len() + 40,
                PageContent::Empty => 40,
            })
            .collect();
        // Part du budget proportionnelle à la taille du document.
        let own: usize = lengths.iter().sum();
        let budget = if total > max_chars {
            (max_chars as f64 * own as f64 / total.max(1) as f64) as usize
        } else {
            usize::MAX
        };
        let kept = pages_within_budget(&lengths, spec.focus_page, budget);
        if kept.len() < pages.len() {
            let (first, last) = (kept.first().map_or(0, |p| p + 1), kept.last().map_or(0, |p| p + 1));
            notes.push(trf!(
                "« {} » est trop long : seules les pages {} à {} (autour de la page affichée) ont été envoyées.",
                "“{}” is too long: only pages {} to {} (around the page shown) were sent.",
                spec.title,
                first,
                last
            ));
        }
        let mut text = format!(
            "<document index=\"{}\" title=\"{}\" pages=\"{}\">\n",
            index + 1,
            spec.title.replace('"', "'"),
            pages.len()
        );
        let mut page_images = Vec::new();
        for &page in &kept {
            match &pages[page] {
                PageContent::Text(t) => {
                    text.push_str(&format!("<page number=\"{}\">\n{}\n</page>\n", page + 1, t));
                }
                PageContent::Empty => {
                    if images_left > 0 {
                        if let Some(jpeg) = render_page_jpeg(spec, page) {
                            images_left -= 1;
                            sent_images += 1;
                            text.push_str(&format!(
                                "<page number=\"{}\">(scanned page: see the image labelled document {} page {})</page>\n",
                                page + 1,
                                index + 1,
                                page + 1
                            ));
                            page_images.push((page, jpeg));
                            continue;
                        }
                    } else if send_page_images {
                        skipped_images += 1;
                    }
                    text.push_str(&format!("<page number=\"{}\">(no text on this page)</page>\n", page + 1));
                }
            }
        }
        text.push_str("</document>");
        blocks.push(ContextBlock::Text(text));
        for (page, jpeg) in page_images {
            blocks.push(ContextBlock::Text(format!("Image: document {} page {}", index + 1, page + 1)));
            blocks.push(ContextBlock::Image { media_type: "image/jpeg", data: Arc::new(jpeg) });
        }
    }
    if sent_images > 0 {
        notes.push(if sent_images == 1 {
            tr("1 page scannée envoyée en image.", "1 scanned page sent as an image.").to_string()
        } else {
            trf!("{} pages scannées envoyées en image.", "{} scanned pages sent as images.", sent_images)
        });
    }
    if skipped_images > 0 {
        notes.push(trf!(
            "{} autres pages scannées n'ont pas été envoyées (limite de {} images).",
            "{} more scanned pages were not sent (limit of {} images).",
            skipped_images,
            MAX_PAGE_IMAGES
        ));
    }
    Ok(PreparedContext { blocks, notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_keeps_pages_around_focus() {
        let lengths = vec![10; 10];
        assert_eq!(pages_within_budget(&lengths, 5, 1000), (0..10).collect::<Vec<_>>());
        assert_eq!(pages_within_budget(&lengths, 5, 30), vec![4, 5, 6]);
        assert_eq!(pages_within_budget(&lengths, 0, 30), vec![0, 1, 2]);
        assert_eq!(pages_within_budget(&lengths, 9, 25), vec![8, 9]);
        // La page affichée est toujours envoyée, même seule au-delà du budget.
        assert_eq!(pages_within_budget(&[500, 10], 0, 30), vec![0]);
    }

    #[test]
    fn test_prepare_reads_sample_pdf() {
        let dir = std::env::temp_dir().join("aerolis_ai_sources_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sample.pdf");
        crate::pdf_engine::ensure_sample_pdf(&path).unwrap();
        let doc = PdfEngine::instance().load_document(&path).unwrap();
        let sizes: Vec<(f32, f32)> = doc.page_dimensions.iter().map(|d| (d.width_pt, d.height_pt)).collect();
        {
            let _lock = PdfEngine::lock_guard();
            drop(doc);
        }
        let spec = SourceSpec {
            title: "sample.pdf".into(),
            path,
            page_sizes: sizes.clone(),
            ocr_text: HashMap::new(),
            focus_page: 0,
        };
        let prepared = prepare(&[spec.clone()], false, 1_000_000).unwrap();
        let ContextBlock::Text(text) = &prepared.blocks[0] else { panic!("texte attendu") };
        assert!(text.starts_with("<document index=\"1\" title=\"sample.pdf\""));
        assert!(text.contains("<page number=\"1\">"));
        assert!(text.len() > 200, "le texte de l'exemple doit être lu");
        // Petit budget (offre gratuite de Groq) : seules les pages autour de la page affichée partent.
        let small = prepare(&[spec], false, 2_000).unwrap();
        let ContextBlock::Text(short) = &small.blocks[0] else { panic!("texte attendu") };
        assert!(short.len() < text.len());
    }
}
