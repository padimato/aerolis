//! Clé API rangée dans le Gestionnaire d'identification de Windows (« Informations d'identification
//! Windows », cible `Aerolis/ai/<fournisseur>`). Windows la chiffre pour le compte de l'utilisateur :
//! elle n'est jamais écrite en clair sur le disque, ni dans les réglages.
//!
//! Pendant les captures automatisées (`AEROLIS_DATA_DIR` défini), les clés restent en mémoire.

use crate::settings::AiProvider;
use std::collections::HashMap;
use std::sync::Mutex;

/// Clés gardées en mémoire à la place du coffre (essais automatisés).
static MEMORY: Mutex<Option<HashMap<AiProvider, String>>> = Mutex::new(None);

/// Variable d'environnement : lire le vrai coffre même avec un dossier de réglages d'essai
/// (essais de l'assistant avec les clés que l'utilisateur a enregistrées lui-même).
pub const USE_VAULT_ENV: &str = "AEROLIS_USE_VAULT";

fn in_memory() -> bool {
    cfg!(test) || std::env::var_os(crate::settings::DATA_DIR_ENV).is_some() && std::env::var_os(USE_VAULT_ENV).is_none()
}

fn target(provider: AiProvider) -> String {
    format!("Aerolis/ai/{}", provider.key())
}

/// Cible sous l'ancien nom de l'application (« Kite PDF »).
fn legacy_target(provider: AiProvider) -> String {
    format!("KitePDF/ai/{}", provider.key())
}

pub fn load(provider: AiProvider) -> Option<String> {
    if in_memory() {
        return MEMORY.lock().ok()?.as_ref()?.get(&provider).cloned();
    }
    read_or_migrate(&target(provider), &legacy_target(provider))
}

/// Lit la clé ; une clé encore rangée sous l'ancien nom passe sous le nouveau, puis l'ancienne est effacée.
fn read_or_migrate(target: &str, legacy: &str) -> Option<String> {
    if let Some(key) = windows::read(target) {
        return Some(key);
    }
    let key = windows::read(legacy)?;
    if windows::write(target, &key).is_ok() {
        windows::delete(legacy);
    }
    Some(key)
}

pub fn save(provider: AiProvider, key: &str) -> Result<(), String> {
    let key = key.trim();
    if in_memory() {
        let mut memory = MEMORY.lock().map_err(|e| e.to_string())?;
        memory.get_or_insert_with(HashMap::new).insert(provider, key.to_string());
        return Ok(());
    }
    windows::write(&target(provider), key)
}

pub fn delete(provider: AiProvider) {
    if in_memory() {
        if let Ok(mut memory) = MEMORY.lock() {
            if let Some(map) = memory.as_mut() {
                map.remove(&provider);
            }
        }
        return;
    }
    windows::delete(&target(provider));
    windows::delete(&legacy_target(provider));
}

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct Credential {
        flags: u32,
        kind: u32,
        target_name: *mut u16,
        comment: *mut u16,
        last_written: FileTime,
        blob_size: u32,
        blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *mut c_void,
        target_alias: *mut u16,
        user_name: *mut u16,
    }

    const CRED_TYPE_GENERIC: u32 = 1;
    const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;

    #[link(name = "advapi32")]
    extern "system" {
        fn CredWriteW(credential: *const Credential, flags: u32) -> i32;
        fn CredReadW(target: *const u16, kind: u32, flags: u32, credential: *mut *mut Credential) -> i32;
        fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
        fn CredFree(buffer: *mut c_void);
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn read(target: &str) -> Option<String> {
        let name = wide(target);
        let mut credential: *mut Credential = std::ptr::null_mut();
        // SAFETY : `name` se termine par 0 ; le tampon renvoyé est libéré par `CredFree`.
        unsafe {
            if CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) == 0 || credential.is_null() {
                return None;
            }
            let c = &*credential;
            let bytes = std::slice::from_raw_parts(c.blob, c.blob_size as usize).to_vec();
            CredFree(credential as *mut c_void);
            String::from_utf8(bytes).ok().filter(|k| !k.is_empty())
        }
    }

    pub fn write(target: &str, key: &str) -> Result<(), String> {
        let mut name = wide(target);
        let mut user = wide("Aerolis");
        let mut blob = key.as_bytes().to_vec();
        let credential = Credential {
            flags: 0,
            kind: CRED_TYPE_GENERIC,
            target_name: name.as_mut_ptr(),
            comment: std::ptr::null_mut(),
            last_written: FileTime { low: 0, high: 0 },
            blob_size: blob.len() as u32,
            blob: blob.as_mut_ptr(),
            persist: CRED_PERSIST_LOCAL_MACHINE,
            attribute_count: 0,
            attributes: std::ptr::null_mut(),
            target_alias: std::ptr::null_mut(),
            user_name: user.as_mut_ptr(),
        };
        // SAFETY : les pointeurs restent valides pendant l'appel.
        let ok = unsafe { CredWriteW(&credential, 0) } != 0;
        // La copie de la clé en mémoire est effacée tout de suite.
        blob.iter_mut().for_each(|b| *b = 0);
        if ok {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().to_string())
        }
    }

    pub fn delete(target: &str) {
        let name = wide(target);
        // SAFETY : `name` se termine par 0.
        unsafe {
            CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0);
        }
    }
}

#[cfg(not(windows))]
mod windows {
    pub fn read(_target: &str) -> Option<String> {
        None
    }
    pub fn write(_target: &str, _key: &str) -> Result<(), String> {
        Err("unsupported".into())
    }
    pub fn delete(_target: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_store_roundtrip() {
        save(AiProvider::Custom, "  sk-test  ").unwrap();
        assert_eq!(load(AiProvider::Custom).as_deref(), Some("sk-test"));
        delete(AiProvider::Custom);
        assert_eq!(load(AiProvider::Custom), None);
    }

    /// Clé rangée sous l'ancien nom (« Kite PDF ») : reprise sous le nouveau (valeurs factices).
    #[test]
    #[cfg(windows)]
    fn test_key_saved_under_the_old_name_moves_to_the_new_one() {
        let (new, old) = (
            format!("Aerolis/test/migration-{}", std::process::id()),
            format!("KitePDF/test/migration-{}", std::process::id()),
        );
        windows::write(&old, "valeur-factice").unwrap();
        assert_eq!(read_or_migrate(&new, &old).as_deref(), Some("valeur-factice"));
        assert_eq!(windows::read(&new).as_deref(), Some("valeur-factice"));
        assert_eq!(windows::read(&old), None);
        // Ensuite, la nouvelle clé est lue directement.
        assert_eq!(read_or_migrate(&new, &old).as_deref(), Some("valeur-factice"));
        windows::delete(&new);
        assert_eq!(read_or_migrate(&new, &old), None);
    }

    /// Vrai coffre de Windows : écrit, relit et efface une cible de test.
    #[cfg(windows)]
    #[test]
    fn test_windows_vault_roundtrip() {
        let target = "Aerolis/test/keystore-roundtrip";
        windows::write(target, "sk-ant-test-123").unwrap();
        assert_eq!(windows::read(target).as_deref(), Some("sk-ant-test-123"));
        windows::delete(target);
        assert_eq!(windows::read(target), None);
    }
}
