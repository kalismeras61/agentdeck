//! API keys for custom model profiles, kept in the OS keychain (macOS Keychain, Windows
//! Credential Manager, the Linux kernel keyring). Never written to SQLite, logs or events.

const SERVICE: &str = "dev.agentdeck.app";

#[cfg(not(test))]
fn entry(profile_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("profile:{profile_id}")).map_err(|e| e.to_string())
}

#[cfg(not(test))]
pub fn set(profile_id: &str, key: &str) -> Result<(), String> {
    entry(profile_id)?
        .set_password(key)
        .map_err(|e| format!("Couldn't save the key in the keychain: {e}"))
}

#[cfg(not(test))]
pub fn get(profile_id: &str) -> Option<String> {
    entry(profile_id).ok()?.get_password().ok()
}

#[cfg(not(test))]
pub fn remove(profile_id: &str) {
    if let Ok(e) = entry(profile_id) {
        let _ = e.delete_credential();
    }
}

// Tests use an in-memory store so they never touch the real keychain.
#[cfg(test)]
fn store() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> =
        std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

#[cfg(test)]
pub fn set(profile_id: &str, key: &str) -> Result<(), String> {
    store()
        .lock()
        .unwrap()
        .insert(profile_id.into(), key.into());
    Ok(())
}

#[cfg(test)]
pub fn get(profile_id: &str) -> Option<String> {
    store().lock().unwrap().get(profile_id).cloned()
}

#[cfg(test)]
pub fn remove(profile_id: &str) {
    store().lock().unwrap().remove(profile_id);
}

#[allow(dead_code)]
pub const fn service() -> &'static str {
    SERVICE
}
