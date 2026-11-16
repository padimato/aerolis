//! Conversations avec l'assistant, une par fichier JSON dans `%APPDATA%\KitePDF\conversations`.
//! Les images jointes y sont gardées (PNG en base64). La clé API n'y figure jamais.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Document utilisé comme source d'une réponse (pour ouvrir les renvois « p. 12 »).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    pub title: String,
    pub path: PathBuf,
}

/// Image jointe à une question : capture d'une zone, image collée ou glissée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageData {
    #[serde(with = "base64_bytes")]
    pub png: Vec<u8>,
    /// « Zone de la page 3 », « Capture collée »...
    pub label: String,
    /// Texte trouvé dans la zone capturée (envoyé avec la question)
    #[serde(default)]
    pub zone_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub user: bool,
    pub text: String,
    #[serde(default)]
    pub images: Vec<ImageData>,
    /// Remarques sous la réponse (document tronqué...)
    #[serde(default)]
    pub notes: Vec<String>,
    /// Message d'erreur à la place d'une réponse
    #[serde(default)]
    pub error: bool,
    /// Documents sources de cette réponse, dans l'ordre de leur numéro
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    /// Fournisseur et modèle qui ont répondu, temps avant le premier mot
    #[serde(default)]
    pub via: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    /// Dernière modification, en secondes depuis 1970
    pub updated: i64,
    pub messages: Vec<Message>,
}

impl Conversation {
    pub fn new() -> Self {
        let now = chrono::Local::now();
        Self {
            id: now.format("%Y%m%d-%H%M%S-%3f").to_string(),
            title: String::new(),
            updated: now.timestamp(),
            messages: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Titre tiré de la première question.
    pub fn refresh_title(&mut self) {
        if !self.title.is_empty() {
            return;
        }
        if let Some(first) = self.messages.iter().find(|m| m.user && !m.text.trim().is_empty()) {
            let line = first.text.lines().next().unwrap_or_default().trim();
            let mut title: String = line.chars().take(60).collect();
            if line.chars().count() > 60 {
                title.push('…');
            }
            self.title = title;
        }
    }
}

/// Résumé d'une conversation enregistrée, pour la liste de l'historique.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub updated: i64,
    pub message_count: usize,
}

pub fn dir() -> PathBuf {
    crate::settings::app_data_dir().join("conversations")
}

fn file(dir: &Path, id: &str) -> PathBuf {
    // L'identifiant vient de `Conversation::new` ; on refuse tout séparateur par prudence.
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    dir.join(format!("{safe}.json"))
}

pub fn save_in(dir: &Path, conversation: &Conversation) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_string(conversation).map_err(std::io::Error::other)?;
    let path = file(dir, &conversation.id);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, path)
}

pub fn load_in(dir: &Path, id: &str) -> Option<Conversation> {
    let text = std::fs::read_to_string(file(dir, id)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn delete_in(dir: &Path, id: &str) {
    let _ = std::fs::remove_file(file(dir, id));
}

/// Conversations enregistrées, la plus récente d'abord.
pub fn list_in(dir: &Path) -> Vec<Summary> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut list: Vec<Summary> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let conversation: Conversation = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            Some(Summary {
                id: conversation.id,
                title: conversation.title,
                updated: conversation.updated,
                message_count: conversation.messages.len(),
            })
        })
        .collect();
    list.sort_by(|a, b| b.updated.cmp(&a.updated));
    list
}

pub fn count_in(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "json")).count())
        .unwrap_or(0)
}

pub fn clear_in(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|x| x == "json") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_roundtrip_and_list() {
        let dir = std::env::temp_dir().join(format!("kite_ai_history_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut conversation = Conversation::new();
        conversation.messages.push(Message {
            user: true,
            text: "Que dit le chapitre 2 sur les transistors MOSFET et leur polarisation en régime saturé ?".into(),
            images: vec![ImageData { png: vec![137, 80, 78, 71], label: "Zone p. 3".into(), zone_text: Some("Vgs".into()) }],
            notes: vec![],
            error: false,
            sources: vec![],
            via: None,
        });
        conversation.messages.push(Message {
            user: false,
            text: "Il explique [p. 12]".into(),
            images: vec![],
            notes: vec!["note".into()],
            error: false,
            sources: vec![SourceRef { title: "cours.pdf".into(), path: "C:/cours.pdf".into() }],
            via: None,
        });
        conversation.refresh_title();
        assert!(conversation.title.ends_with('…'));
        assert!(conversation.title.chars().count() <= 61);
        save_in(&dir, &conversation).unwrap();
        assert_eq!(load_in(&dir, &conversation.id), Some(conversation.clone()));
        let list = list_in(&dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message_count, 2);
        assert_eq!(count_in(&dir), 1);
        // Un identifiant forgé ne sort pas du dossier.
        assert_eq!(file(&dir, "../../x"), dir.join("x.json"));
        delete_in(&dir, &conversation.id);
        assert_eq!(count_in(&dir), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
