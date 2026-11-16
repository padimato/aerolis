//! Langue de l'interface : français ou anglais.
//!
//! Chaque texte visible est écrit dans les deux langues à l'endroit où il est affiché :
//! `tr("Ouvrir", "Open")`, ou `trf!("{n} pages", "{n} pages")` pour un texte formaté.
//! La langue choisie (Réglages → Général) est gardée dans une variable globale, lue à chaque image.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Fr
    }
}

pub fn is_en() -> bool {
    current() == Lang::En
}

/// Texte dans la langue de l'interface.
pub fn tr(fr: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        fr
    }
}

/// Texte formaté dans la langue de l'interface : `trf!("{n} pages lues", "{n} pages read")`.
#[macro_export]
macro_rules! trf {
    ($fr:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() { format!($en $(, $arg)*) } else { format!($fr $(, $arg)*) }
    };
}

/// Langue de Windows (langue d'affichage de l'utilisateur) : français si c'est du français, anglais sinon.
pub fn system_lang() -> Lang {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        // Identifiant de langue principal : les 10 bits de poids faible. 0x0C = français.
        let primary = unsafe { GetUserDefaultUILanguage() } & 0x3ff;
        if primary == 0x0c {
            Lang::Fr
        } else {
            Lang::En
        }
    }
    #[cfg(not(windows))]
    {
        let lang = std::env::var("LANG").unwrap_or_default();
        if lang.starts_with("fr") {
            Lang::Fr
        } else {
            Lang::En
        }
    }
}

/// Libellé d'une table constante (couleurs, états de révision), écrit en français dans le code.
pub fn label(fr: &'static str) -> &'static str {
    if !is_en() {
        return fr;
    }
    match fr {
        "Aucun" => "None",
        "Accepté" => "Accepted",
        "Annulé" => "Cancelled",
        "Terminé" => "Completed",
        "Refusé" => "Rejected",
        "Blanc" => "White",
        "Crème" => "Cream",
        "Gris clair" => "Light grey",
        "Jaune pâle" => "Pale yellow",
        "Bleu pâle" => "Pale blue",
        "Vert pâle" => "Pale green",
        "Rouge pâle" => "Pale red",
        "Violet pâle" => "Pale purple",
        "Noir" => "Black",
        "Gris" => "Grey",
        "Rouge" => "Red",
        "Bleu" => "Blue",
        "Vert" => "Green",
        "Orange" => "Orange",
        "Jaune Or" => "Gold yellow",
        "Bleu Azur" => "Azure blue",
        "Vert Émeraude" => "Emerald green",
        "Rouge Corail" => "Coral red",
        "Violet Améthyste" => "Amethyst purple",
        other => other,
    }
}

/// Nombre suivi d'un nom, au singulier ou au pluriel (« 1 page », « 3 pages »).
pub fn plural(n: usize, one: &'static str, many: &'static str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plural_and_format() {
        // La langue globale n'est pas modifiée ici : les autres tests tournent en parallèle.
        assert_eq!(plural(1, "page", "pages"), "1 page");
        assert_eq!(plural(4, "page", "pages"), "4 pages");
        let n = 3;
        let text = trf!("{n} pages lues", "{n} pages read");
        assert!(text == "3 pages lues" || text == "3 pages read");
        assert!(matches!(system_lang(), Lang::Fr | Lang::En));
    }
}
