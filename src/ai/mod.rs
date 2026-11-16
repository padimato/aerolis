//! Assistant IA : questions sur les PDF ouverts, avec la clé API de l'utilisateur.
//!
//! - [`keystore`] : clé API dans le Gestionnaire d'identification de Windows (chiffrée pour le compte).
//! - [`sources`] : texte des documents choisis comme sources (et images des pages scannées).
//! - [`net`] : requêtes en continu (streaming) vers Claude, OpenAI ou un service compatible.
//! - [`history`] : conversations enregistrées dans `%APPDATA%\KitePDF\conversations`.
//! - [`markdown`] : affichage des réponses (titres, listes, gras, renvois « p. 12 » cliquables).
//! - [`panel`] : bouton rond flottant et fenêtre de discussion.

pub mod history;
pub mod keystore;
pub mod markdown;
pub mod net;
pub mod panel;
pub mod privacy;
pub mod providers;
pub mod sources;

use crate::settings::AiProvider;
use serde_json::{json, Value};

/// Modèle utilisé quand l'utilisateur n'en a pas choisi (vide : choisi automatiquement ou à choisir).
pub fn default_model(provider: AiProvider) -> &'static str {
    providers::info(provider).default_model
}

/// Adresse de base de l'API du fournisseur (sans `/` final).
pub fn base_url(provider: AiProvider, custom_url: &str) -> String {
    match provider {
        AiProvider::Custom => {
            let url = custom_url.trim().trim_end_matches('/');
            if url.is_empty() {
                "http://localhost:11434/v1".into()
            } else {
                url.into()
            }
        }
        other => providers::info(other).base_url.into(),
    }
}

/// Une clé API est indispensable (un service local n'en demande pas).
pub fn key_required(provider: AiProvider) -> bool {
    providers::info(provider).key_required
}

/// Version de l'API Anthropic envoyée dans l'en-tête `anthropic-version`.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";

/// Bloc de contexte placé au début de la conversation : texte des documents, images des pages scannées.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextBlock {
    Text(String),
    Image { media_type: &'static str, data: std::sync::Arc<Vec<u8>> },
}

/// Un message de la conversation tel qu'il est envoyé.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub user: bool,
    pub text: String,
    /// Images PNG jointes par l'utilisateur
    pub images: Vec<std::sync::Arc<Vec<u8>>>,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub provider: AiProvider,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub system: String,
    pub context: Vec<ContextBlock>,
    pub turns: Vec<Turn>,
    /// Modèles de secours essayés par le fournisseur lui-même si `model` est saturé (OpenRouter)
    pub fallback_models: Vec<String>,
    /// Envoi par le proxy de confidentialité (format OpenAI, `base_url` et `api_key` sont ceux du proxy)
    pub via_proxy: bool,
}

/// Consignes données au modèle. `english` : langue de l'interface.
pub fn system_prompt(english: bool, document_count: usize) -> String {
    let citation = if document_count > 1 {
        "Several documents are provided, numbered 1, 2, 3... Cite pages as [2, p. 12] (document number, then page). \
         For a range, write [2, p. 12-14]."
    } else {
        "Cite pages as [p. 12]. For a range, write [p. 12-14]."
    };
    let language = if english {
        "Always reply in the language of the user's latest message, whatever the language of the documents. If that is unclear, reply in English."
    } else {
        "Always reply in the language of the user's latest message, whatever the language of the documents. If that is unclear, reply in French."
    };
    format!(
        "You are the assistant built into Kite PDF, a PDF reader for Windows. The user asks questions about the \
documents they have open; their text is given between <document> tags, page by page (<page number=\"N\">). \
Pages with no extractable text may be given as images instead.\n\n\
Base your answers on these documents. When a sentence uses information from a document, put the page reference \
right after it. {citation} Only use page numbers that appear in the page tags; never invent one. If the documents \
do not contain the answer, say so plainly, then answer from general knowledge if that helps and say that you did.\n\n\
The user may attach screenshots: often a zone of a page captured in the reader, with the text found in that zone. \
Focus your answer on what they point at.\n\n\
Format answers in Markdown: short paragraphs, bullet lists, **bold** for key terms, headings only for long answers. \
No tables wider than four columns. Never use LaTeX or $...$: write formulas, units and symbols in plain text (for example 4200 mAh/g, Li+, 10^-5 S/cm, x^2, <=). Be clear and direct, like a good tutor.\n\n{language}"
    )
}

fn anthropic_content(text: &str, images: &[std::sync::Arc<Vec<u8>>]) -> Vec<Value> {
    let mut content: Vec<Value> = images
        .iter()
        .map(|png| {
            json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64(png)}})
        })
        .collect();
    content.push(json!({"type": "text", "text": text}));
    content
}

/// Modèles où Anthropic peut relancer la demande sur un autre modèle si ses filtres de sécurité la refusent.
fn supports_server_fallback(model: &str) -> bool {
    model.starts_with("claude-opus-5-5") || model.starts_with("claude-sonnet-5-5") || model.starts_with("claude-fable-5")
}

/// En-têtes supplémentaires pour Anthropic (fonction bêta de repli).
pub fn anthropic_beta_header(model: &str) -> Option<&'static str> {
    supports_server_fallback(model).then_some("server-side-fallback-2026-07-01")
}

/// Corps d'une requête `POST /v1/messages` en continu.
pub fn anthropic_body(req: &ChatRequest) -> Value {
    let mut messages: Vec<Value> = Vec::new();
    for (i, turn) in req.turns.iter().enumerate() {
        let mut content = Vec::new();
        if i == 0 && !req.context.is_empty() {
            for block in &req.context {
                content.push(match block {
                    ContextBlock::Text(text) => json!({"type": "text", "text": text}),
                    ContextBlock::Image { media_type, data } => json!({
                        "type": "image",
                        "source": {"type": "base64", "media_type": media_type, "data": b64(data)}
                    }),
                });
            }
            // Point de cache après les documents : les questions suivantes les relisent à prix réduit.
            if let Some(last) = content.last_mut() {
                last["cache_control"] = json!({"type": "ephemeral"});
            }
        }
        content.extend(anthropic_content(&turn.text, &turn.images));
        messages.push(json!({"role": if turn.user { "user" } else { "assistant" }, "content": content}));
    }
    let max_tokens = if req.model.contains("claude-3") { 4096 } else { 32000 };
    let mut body = json!({
        "model": req.model,
        "max_tokens": max_tokens,
        "stream": true,
        "system": req.system,
        // Cache automatique de toute la conversation (en plus du point placé après les documents).
        "cache_control": {"type": "ephemeral"},
        "messages": messages,
    });
    if supports_server_fallback(&req.model) {
        body["fallbacks"] = json!("default");
    }
    body
}

/// Corps d'une requête `POST /chat/completions` en continu (OpenAI et services compatibles).
pub fn openai_body(req: &ChatRequest) -> Value {
    let mut messages = vec![json!({"role": "system", "content": req.system})];
    for (i, turn) in req.turns.iter().enumerate() {
        if !turn.user {
            messages.push(json!({"role": "assistant", "content": turn.text}));
            continue;
        }
        let mut content: Vec<Value> = Vec::new();
        if i == 0 {
            for block in &req.context {
                content.push(match block {
                    ContextBlock::Text(text) => json!({"type": "text", "text": text}),
                    ContextBlock::Image { media_type, data } => json!({
                        "type": "image_url",
                        "image_url": {"url": format!("data:{media_type};base64,{}", b64(data))}
                    }),
                });
            }
        }
        for png in &turn.images {
            content.push(json!({
                "type": "image_url",
                "image_url": {"url": format!("data:image/png;base64,{}", b64(png))}
            }));
        }
        content.push(json!({"type": "text", "text": turn.text}));
        messages.push(json!({"role": "user", "content": content}));
    }
    let mut body = json!({"model": req.model, "stream": true, "messages": messages});
    // Réponse rapide : raisonnement court quand le fournisseur permet de le régler.
    match req.provider {
        AiProvider::OpenRouter => body["reasoning"] = json!({"effort": "low"}),
        AiProvider::Nvidia => body["chat_template_kwargs"] = json!({"enable_thinking": false}),
        _ => {}
    }
    if req.provider == AiProvider::OpenRouter && !req.fallback_models.is_empty() {
        // OpenRouter enchaîne lui-même ces modèles si le premier est saturé.
        let mut models = vec![req.model.clone()];
        models.extend(req.fallback_models.iter().cloned());
        body["models"] = json!(models);
    }
    body
}

pub fn b64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Ce que contient une ligne `data:` d'un flux SSE.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Text(String),
    /// Le modèle raisonne avant de répondre (texte du raisonnement non affiché)
    Thinking,
    /// Fin de la réponse ; raison d'arrêt éventuelle (`max_tokens`, `refusal`...)
    Stop(Option<String>),
    Error(String),
    /// Rien d'utile (début de message, ping...)
    Nothing,
}

/// Lit une ligne d'un flux SSE. Seules les lignes `data:` comptent ; `[DONE]` termine le flux d'OpenAI.
pub fn parse_sse_line(provider: AiProvider, line: &str) -> StreamEvent {
    let Some(data) = line.strip_prefix("data:") else {
        return StreamEvent::Nothing;
    };
    let data = data.trim();
    if data == "[DONE]" {
        return StreamEvent::Stop(None);
    }
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return StreamEvent::Nothing;
    };
    if let Some(error) = value.get("error") {
        let message = error.get("message").and_then(Value::as_str).unwrap_or("error");
        return StreamEvent::Error(message.to_string());
    }
    match provider {
        AiProvider::Anthropic => match value["type"].as_str() {
            Some("content_block_delta") if value["delta"]["type"] == "text_delta" => {
                StreamEvent::Text(value["delta"]["text"].as_str().unwrap_or_default().to_string())
            }
            Some("content_block_delta") if value["delta"]["type"] == "thinking_delta" => StreamEvent::Thinking,
            Some("message_delta") => match value["delta"]["stop_reason"].as_str() {
                Some(reason) if reason != "end_turn" => StreamEvent::Stop(Some(reason.to_string())),
                _ => StreamEvent::Nothing,
            },
            Some("message_stop") => StreamEvent::Stop(None),
            _ => StreamEvent::Nothing,
        },
        _ => {
            let choice = &value["choices"][0];
            if let Some(text) = choice["delta"]["content"].as_str() {
                if !text.is_empty() {
                    return StreamEvent::Text(text.to_string());
                }
            }
            // DeepSeek, Qwen, gpt-oss... : raisonnement dans un champ à part, avant la réponse.
            let delta = &choice["delta"];
            if ["reasoning_content", "reasoning"].iter().any(|k| delta[k].as_str().is_some_and(|t| !t.is_empty())) {
                return StreamEvent::Thinking;
            }
            match choice["finish_reason"].as_str() {
                Some("length") => StreamEvent::Stop(Some("max_tokens".into())),
                Some("content_filter") => StreamEvent::Stop(Some("refusal".into())),
                _ => StreamEvent::Nothing,
            }
        }
    }
}

/// Modèles de discussion dans la réponse de `GET /models` (les modèles d'images, de voix, d'embeddings...
/// sont écartés). OpenRouter : seulement les modèles gratuits, du plus grand contexte au plus petit.
pub fn parse_model_list(provider: AiProvider, body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let items = value["data"].as_array().or(value["models"].as_array()).cloned().unwrap_or_default();
    const NOT_CHAT: [&str; 16] = [
        "embed", "tts", "whisper", "dall-e", "audio", "realtime", "transcribe", "moderation", "image", "search",
        "rerank", "guard", "ocr", "veo", "imagen", "safeguard",
    ];
    let mut models: Vec<(String, u64)> = items
        .iter()
        .filter_map(|m| {
            let id = m["id"].as_str().or(m["name"].as_str())?;
            // Gemini : « models/gemini-... » dans la liste, « gemini-... » dans les requêtes.
            let id = id.strip_prefix("models/").unwrap_or(id).to_string();
            Some((id, m["context_length"].as_u64().unwrap_or(0)))
        })
        .filter(|(id, _)| !NOT_CHAT.iter().any(|bad| id.to_ascii_lowercase().contains(bad)))
        .collect();
    match provider {
        AiProvider::OpenAi => {
            models.retain(|(m, _)| {
                m.starts_with("gpt") || m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("chatgpt")
            });
            models.sort();
            models.reverse();
        }
        AiProvider::Gemini => {
            models.retain(|(m, _)| m.starts_with("gemini") || m.starts_with("gemma"));
        }
        AiProvider::OpenRouter => {
            // Mesuré le 16/11/2026 : ceux-ci répondent quand les autres gratuits sont saturés (429).
            const PREFERRED: [&str; 3] = ["nemotron-3.5-lightning", "gemma-4-31b", "nemotron-3.5-super"];
            let rank = |id: &str| PREFERRED.iter().position(|p| id.contains(p)).unwrap_or(PREFERRED.len());
            models.retain(|(m, _)| m.ends_with(":free"));
            models.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));
        }
        _ => {}
    }
    models.into_iter().map(|(id, _)| id).collect()
}

/// Renvoi vers une page dans une réponse : « [p. 12] », « [2, p. 12-14] ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Citation {
    /// Numéro du document à partir de 1 (1 s'il n'est pas précisé)
    pub document: usize,
    /// Première page citée, à partir de 1
    pub page: usize,
}

/// Lit le contenu d'un renvoi entre crochets (« p. 12 », « 2, p. 12-14 », « pp. 3–4 »).
pub fn parse_citation(inner: &str) -> Option<Citation> {
    let inner = inner.trim();
    let (document, rest) = match inner.split_once(',') {
        // « p. 4, p. 13 » n'est pas « document 4, page 13 » : la liste est lue par l'appelant.
        Some((doc, _)) if doc.trim_start().to_ascii_lowercase().starts_with('p') => return None,
        Some((doc, rest)) => {
            let doc = doc.trim().trim_start_matches(|c: char| c.is_alphabetic() || c == '.' || c == ' ');
            (doc.trim().parse::<usize>().ok()?, rest.trim())
        }
        None => (1, inner),
    };
    let rest = rest
        .strip_prefix("pp.")
        .or_else(|| rest.strip_prefix("p."))
        .or_else(|| rest.strip_prefix("page"))
        .or_else(|| rest.strip_prefix("Page"))?
        .trim();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let after = &rest[digits.len()..];
    // Seuls un intervalle de pages ou rien ne peuvent suivre le numéro.
    if !after.trim().is_empty() && !after.trim_start().starts_with(['-', '–', '—']) {
        return None;
    }
    let page = digits.parse::<usize>().ok().filter(|p| *p > 0)?;
    (document > 0).then_some(Citation { document, page })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn request(provider: AiProvider, model: &str) -> ChatRequest {
        ChatRequest {
            provider,
            model: model.into(),
            base_url: base_url(provider, ""),
            api_key: Some("test".into()),
            system: system_prompt(false, 1),
            context: vec![
                ContextBlock::Text("<document index=\"1\">…</document>".into()),
                ContextBlock::Image { media_type: "image/jpeg", data: Arc::new(vec![1, 2, 3]) },
            ],
            turns: vec![
                Turn { user: true, text: "Résume".into(), images: vec![Arc::new(vec![9])] },
                Turn { user: false, text: "Voici [p. 2]".into(), images: vec![] },
                Turn { user: true, text: "Et la page 3 ?".into(), images: vec![] },
            ],
            fallback_models: vec![],
            via_proxy: false,
        }
    }

    #[test]
    fn test_anthropic_body_puts_documents_first_and_caches_them() {
        let body = anthropic_body(&request(AiProvider::Anthropic, "claude-opus-5-5"));
        let first = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(first[0]["type"], "text");
        assert_eq!(first[1]["type"], "image");
        assert_eq!(first[1]["cache_control"]["type"], "ephemeral");
        assert_eq!(first[2]["type"], "image");
        assert_eq!(first[3]["text"], "Résume");
        assert_eq!(body["messages"][1]["role"], "assistant");
        assert_eq!(body["messages"][2]["content"].as_array().unwrap().len(), 1);
        assert_eq!(body["stream"], true);
        assert_eq!(body["fallbacks"], "default");
        assert!(body.get("temperature").is_none());
        let older = anthropic_body(&request(AiProvider::Anthropic, "claude-haiku-4-5"));
        assert!(older.get("fallbacks").is_none());
        assert_eq!(anthropic_beta_header("claude-haiku-4-5"), None);
    }

    #[test]
    fn test_openai_body_uses_data_urls() {
        let body = openai_body(&request(AiProvider::OpenAi, "gpt-5"));
        assert_eq!(body["messages"][0]["role"], "system");
        let first = body["messages"][1]["content"].as_array().unwrap();
        assert_eq!(first[1]["type"], "image_url");
        assert!(first[1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
        assert_eq!(first.last().unwrap()["text"], "Résume");
        assert_eq!(body["messages"][2]["content"], "Voici [p. 2]");
    }

    #[test]
    fn test_sse_lines() {
        let a = AiProvider::Anthropic;
        assert_eq!(
            parse_sse_line(a, r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Bon"}}"#),
            StreamEvent::Text("Bon".into())
        );
        assert_eq!(parse_sse_line(a, "event: ping"), StreamEvent::Nothing);
        assert_eq!(
            parse_sse_line(a, r#"data: {"type":"message_delta","delta":{"stop_reason":"max_tokens"}}"#),
            StreamEvent::Stop(Some("max_tokens".into()))
        );
        assert_eq!(parse_sse_line(a, r#"data: {"type":"message_stop"}"#), StreamEvent::Stop(None));
        assert_eq!(
            parse_sse_line(a, r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#),
            StreamEvent::Error("Overloaded".into())
        );
        let o = AiProvider::OpenAi;
        assert_eq!(
            parse_sse_line(o, r#"data: {"choices":[{"delta":{"content":"jour"},"finish_reason":null}]}"#),
            StreamEvent::Text("jour".into())
        );
        assert_eq!(parse_sse_line(o, "data: [DONE]"), StreamEvent::Stop(None));
        assert_eq!(
            parse_sse_line(o, r#"data: {"choices":[{"delta":{"content":null,"reasoning_content":"Hmm"}}]}"#),
            StreamEvent::Thinking
        );
    }

    #[test]
    fn test_model_list_filters_openai() {
        let body = r#"{"data":[{"id":"gpt-5"},{"id":"text-embedding-3-small"},{"id":"whisper-1"},{"id":"gpt-5-mini"}]}"#;
        assert_eq!(parse_model_list(AiProvider::OpenAi, body), vec!["gpt-5-mini", "gpt-5"]);
        let body = r#"{"data":[{"id":"models/gemini-flash-latest"},{"id":"models/text-embedding-004"}]}"#;
        assert_eq!(parse_model_list(AiProvider::Gemini, body), vec!["gemini-flash-latest"]);
        let body = r#"{"data":[{"id":"a/small:free","context_length":32000},{"id":"b/paid","context_length":900000},
            {"id":"c/big:free","context_length":131072}]}"#;
        assert_eq!(parse_model_list(AiProvider::OpenRouter, body), vec!["c/big:free", "a/small:free"]);
        let body = r#"{"data":[{"id":"x/huge:free","context_length":1000000},{"id":"nvidia/nemotron-3.5-lightning:free","context_length":1000}]}"#;
        assert_eq!(parse_model_list(AiProvider::OpenRouter, body)[0], "nvidia/nemotron-3.5-lightning:free");
        let body = r#"{"data":[{"id":"claude-opus-5-5"},{"id":"claude-haiku-5-5"}]}"#;
        assert_eq!(parse_model_list(AiProvider::Anthropic, body).len(), 2);
    }

    #[test]
    fn test_citations() {
        assert_eq!(parse_citation("p. 12"), Some(Citation { document: 1, page: 12 }));
        assert_eq!(parse_citation("2, p. 12-14"), Some(Citation { document: 2, page: 12 }));
        assert_eq!(parse_citation("Doc 3, pp. 4–5"), Some(Citation { document: 3, page: 4 }));
        assert_eq!(parse_citation("page 7"), Some(Citation { document: 1, page: 7 }));
        assert_eq!(parse_citation("1"), None);
        assert_eq!(parse_citation("p. 0"), None);
        assert_eq!(parse_citation("p. 3 et suivantes"), None);
        assert_eq!(parse_citation("voir note"), None);
    }
}
