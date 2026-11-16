//! Fournisseurs d'IA proposés : adresse de l'API, modèle par défaut, page où créer une clé, et ce que
//! l'offre gratuite accepte.
//!
//! Tous, sauf Claude, parlent le format « chat completions » d'OpenAI. Les limites gratuites changent
//! souvent : `max_input_tokens` est volontairement prudent, et un document plus long est réduit aux
//! pages proches de celle affichée (l'utilisateur en est prévenu).

use crate::settings::AiProvider;

pub struct ProviderInfo {
    pub name: &'static str,
    /// Adresse de base de l'API (sans `/` final), vide pour le service personnalisé
    pub base_url: &'static str,
    /// Modèle utilisé si l'utilisateur n'en choisit pas (vide : choisi automatiquement ou à choisir)
    pub default_model: &'static str,
    /// Page où créer une clé
    pub key_url: &'static str,
    /// Offre gratuite (sans carte bancaire, avec des limites)
    pub free: bool,
    /// Essayé avec une vraie clé par les auteurs (Gemini, NVIDIA et OpenRouter le 16/11/2026).
    /// Les autres attendent les retours des utilisateurs.
    pub tested: bool,
    /// Le modèle par défaut lit les images (captures, pages scannées)
    pub vision: bool,
    /// Taille d'une question (documents compris) que l'offre accepte sans refus, en jetons
    pub max_input_tokens: usize,
    /// Une clé est obligatoire
    pub key_required: bool,
    /// Ordre d'essai quand le fournisseur choisi ne répond pas : les plus rapides d'abord
    pub fallback_rank: u8,
}

pub fn info(provider: AiProvider) -> ProviderInfo {
    match provider {
        AiProvider::Gemini => ProviderInfo {
            name: "Google Gemini",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
            default_model: "gemini-flash-lite-latest",
            key_url: "https://aistudio.google.com/apikey",
            free: true,
            tested: true,
            vision: true,
            max_input_tokens: 200_000,
            key_required: true,
            fallback_rank: 2,
        },
        AiProvider::Groq => ProviderInfo {
            name: "Groq",
            base_url: "https://api.groq.com/openai/v1",
            default_model: "openai/gpt-oss-120b",
            key_url: "https://console.groq.com/keys",
            free: true,
            tested: false,
            vision: false,
            // Offre gratuite : environ 8 000 jetons par minute, réponse comprise.
            max_input_tokens: 5_500,
            key_required: true,
            fallback_rank: 0,
        },
        AiProvider::Mistral => ProviderInfo {
            name: "Mistral",
            base_url: "https://api.mistral.ai/v1",
            default_model: "mistral-small-latest",
            key_url: "https://console.mistral.ai/api-keys",
            free: true,
            tested: false,
            vision: true,
            max_input_tokens: 100_000,
            key_required: true,
            fallback_rank: 4,
        },
        AiProvider::OpenRouter => ProviderInfo {
            name: "OpenRouter",
            base_url: "https://openrouter.ai/api/v1",
            // Vide : le modèle gratuit (« :free ») au plus grand contexte est choisi à chaque question.
            default_model: "",
            key_url: "https://openrouter.ai/settings/keys",
            free: true,
            tested: true,
            vision: false,
            max_input_tokens: 100_000,
            key_required: true,
            fallback_rank: 5,
        },
        AiProvider::Nvidia => ProviderInfo {
            name: "NVIDIA NIM",
            base_url: "https://integrate.api.nvidia.com/v1",
            default_model: "nvidia/nemotron-3.5-lightning-30b-a3b",
            key_url: "https://build.nvidia.com/settings/api-keys",
            free: true,
            tested: true,
            vision: false,
            max_input_tokens: 100_000,
            key_required: true,
            fallback_rank: 3,
        },
        AiProvider::Cerebras => ProviderInfo {
            name: "Cerebras",
            base_url: "https://api.cerebras.ai/v1",
            default_model: "gpt-oss-120b",
            key_url: "https://cloud.cerebras.ai/platform",
            free: true,
            tested: false,
            vision: false,
            max_input_tokens: 30_000,
            key_required: true,
            fallback_rank: 1,
        },
        AiProvider::Anthropic => ProviderInfo {
            name: "Claude (Anthropic)",
            base_url: "https://api.anthropic.com/v1",
            default_model: "claude-opus-5-5",
            key_url: "https://console.anthropic.com/settings/keys",
            free: false,
            tested: false,
            vision: true,
            max_input_tokens: 150_000,
            key_required: true,
            fallback_rank: 6,
        },
        AiProvider::OpenAi => ProviderInfo {
            name: "OpenAI",
            base_url: "https://api.openai.com/v1",
            default_model: "gpt-5",
            key_url: "https://platform.openai.com/api-keys",
            free: false,
            tested: false,
            vision: true,
            max_input_tokens: 150_000,
            key_required: true,
            fallback_rank: 7,
        },
        AiProvider::Custom => ProviderInfo {
            name: "",
            base_url: "",
            default_model: "",
            key_url: "",
            free: false,
            tested: false,
            vision: true,
            max_input_tokens: 100_000,
            key_required: false,
            fallback_rank: 8,
        },
    }
}

/// Nom affiché (le service personnalisé dépend de la langue).
pub fn display_name(provider: AiProvider) -> &'static str {
    match provider {
        AiProvider::Custom => crate::i18n::tr("Autre service compatible OpenAI", "Other OpenAI-compatible service"),
        other => info(other).name,
    }
}

/// Une phrase pour aider à choisir.
pub fn pitch(provider: AiProvider) -> &'static str {
    use crate::i18n::tr;
    match provider {
        AiProvider::Gemini => tr(
            "Gratuit, rapide, lit les longs PDF en entier. Le plus polyvalent pour commencer.",
            "Free, fast, reads long PDFs in full. The most versatile to start with.",
        ),
        AiProvider::Groq => tr(
            "Gratuit et le plus rapide, mais l'offre gratuite n'accepte que de petits textes : seules les pages proches de celle affichée sont envoyées.",
            "Free and the fastest, but the free plan only takes short texts: only the pages near the one shown are sent.",
        ),
        AiProvider::Mistral => tr(
            "Gratuit, hébergé en Europe, lit les longs PDF.",
            "Free, hosted in Europe, reads long PDFs.",
        ),
        AiProvider::OpenRouter => tr(
            "Modèles gratuits « :free » (environ 50 questions par jour), souvent saturés : à garder en relais.",
            "Free “:free” models (about 50 questions a day), often busy: best kept as a fallback.",
        ),
        AiProvider::Nvidia => tr(
            "Crédits gratuits ; le modèle par défaut répond en 1 à 2 s. Certains autres modèles attendent longtemps leur tour.",
            "Free credits; the default model answers in 1 to 2 s. Some other models wait a long time for their turn.",
        ),
        AiProvider::Cerebras => tr(
            "Très rapide ; l'offre gratuite est devenue un essai limité dans le temps.",
            "Very fast; the free plan has become a time-limited trial.",
        ),
        AiProvider::Anthropic => tr("Payant, à l'usage.", "Paid, pay as you go."),
        AiProvider::OpenAi => tr("Payant, à l'usage.", "Paid, pay as you go."),
        AiProvider::Custom => tr(
            "Ollama ou LM Studio sur cet ordinateur, ou tout service compatible OpenAI.",
            "Ollama or LM Studio on this computer, or any OpenAI-compatible service.",
        ),
    }
}

/// Page des retours sur GitHub (fournisseurs non testés).
pub const FEEDBACK_URL: &str = "https://github.com/padimato/kite-pdf/issues";

/// Estimation du nombre de jetons d'un texte (environ 3,5 caractères par jeton en français et en anglais).
pub fn estimate_tokens(chars: usize) -> usize {
    chars * 2 / 7 + 1
}

/// Caractères de document qu'un fournisseur accepte (place gardée pour la question et l'historique).
pub fn document_budget_chars(provider: AiProvider, conversation_chars: usize) -> usize {
    let tokens = info(provider).max_input_tokens.saturating_sub(estimate_tokens(conversation_chars) + 1_500);
    (tokens * 7 / 2).max(4_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_is_consistent() {
        for provider in AiProvider::ALL {
            let i = info(provider);
            assert!(i.base_url.is_empty() || i.base_url.starts_with("https://"));
            assert!(!i.base_url.ends_with('/'));
            assert!(!display_name(provider).is_empty());
            assert!(!pitch(provider).is_empty());
            assert_eq!(i.key_required, provider != AiProvider::Custom);
        }
        // Groq gratuit : un long document est réduit, Gemini le prend en entier.
        assert!(document_budget_chars(AiProvider::Groq, 200) < 20_000);
        assert!(document_budget_chars(AiProvider::Gemini, 200) > 500_000);
        // Ordre d'essai sans doublon.
        let mut ranks: Vec<u8> = AiProvider::ALL.iter().map(|p| info(*p).fallback_rank).collect();
        ranks.sort_unstable();
        ranks.dedup();
        assert_eq!(ranks.len(), AiProvider::ALL.len());
    }
}
