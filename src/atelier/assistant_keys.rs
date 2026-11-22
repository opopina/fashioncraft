//! The Assistant's API keys: in the macOS keychain or the Windows credential manager, and
//! elsewhere in a file only this user can read, in the session folder. Never in the
//! preferences. A provider's environment variable (ANTHROPIC_API_KEY…) works too.
use fashioncraft::assistant::Provider;

#[cfg(any(target_os = "macos", windows))]
const SERVICE: &str = "FashionCraft";

fn account(provider: Provider) -> String {
    format!("assistant-{}", provider.id())
}

/// Where a provider's key comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Source {
    /// Saved from the Assistant's connection settings.
    Saved,
    /// From this environment variable.
    Environment(&'static str),
    /// Not needed (a local model) or not there.
    Missing,
}

/// The key for `provider`: the saved one, else its environment variable's.
pub(super) fn find(provider: Provider) -> (String, Source) {
    if !provider.needs_key() {
        return (String::new(), Source::Missing);
    }
    if let Ok(Some(key)) = store::get(&account(provider)) {
        return (key, Source::Saved);
    }
    if let Some(variable) = provider.key_variable()
        && let Ok(key) = std::env::var(variable)
        && !key.trim().is_empty()
    {
        return (key, Source::Environment(variable));
    }
    (String::new(), Source::Missing)
}

pub(super) fn save(provider: Provider, key: &str) -> Result<(), String> {
    store::set(&account(provider), key.trim())
}

pub(super) fn forget(provider: Provider) -> Result<(), String> {
    store::delete(&account(provider))
}

/// Where saved keys go, for the person.
pub(super) const SAVED_NOTE: &str = store::SAVED_NOTE;

#[cfg(any(target_os = "macos", windows))]
mod store {
    pub(super) const SAVED_NOTE: &str = if cfg!(windows) {
        "A key is saved in the Windows credential manager."
    } else {
        "A key is saved in the macOS keychain."
    };

    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(super::SERVICE, account).map_err(|e| e.to_string())
    }
    pub(super) fn get(account: &str) -> Result<Option<String>, String> {
        match entry(account)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    pub(super) fn set(account: &str, key: &str) -> Result<(), String> {
        entry(account)?.set_password(key).map_err(|e| e.to_string())
    }
    pub(super) fn delete(account: &str) -> Result<(), String> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod store {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    pub(super) const SAVED_NOTE: &str =
        "A key is saved in a private file in FashionCraft's session folder.";

    fn path() -> PathBuf {
        fashioncraft::agent::session_dir().join("assistant-keys.json")
    }
    fn all() -> BTreeMap<String, String> {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
    fn write(keys: &BTreeMap<String, String>) -> Result<(), String> {
        use std::io::Write;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let path = path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(keys).map_err(|e| e.to_string())?;
        let partial = path.with_extension("json.partial");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&partial).map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&partial, &path).map_err(|e| e.to_string())
    }
    pub(super) fn get(account: &str) -> Result<Option<String>, String> {
        Ok(all().remove(account))
    }
    pub(super) fn set(account: &str, key: &str) -> Result<(), String> {
        let mut keys = all();
        keys.insert(account.to_owned(), key.to_owned());
        write(&keys)
    }
    pub(super) fn delete(account: &str) -> Result<(), String> {
        let mut keys = all();
        if keys.remove(account).is_some() {
            write(&keys)?;
        }
        Ok(())
    }
}
