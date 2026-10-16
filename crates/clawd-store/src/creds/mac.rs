//! macOS Keychain backend: generic password with `kSecAttrService = service` and
//! `kSecAttrAccount = username` through `security-framework` (the same mapping python-keyring's
//! macOS backend uses, so items saved by 0.1.4 are found).

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

/// `get_generic_password(service, account)` as text.
pub(super) fn get_password(service: &str, username: &str) -> Option<String> {
    let bytes = get_generic_password(service, username).ok()?;
    String::from_utf8(bytes).ok()
}

/// `set_generic_password(service, account, secret)`.
pub(super) fn set_password(service: &str, username: &str, secret: &str) -> bool {
    set_generic_password(service, username, secret.as_bytes()).is_ok()
}

/// `delete_generic_password(service, account)`.
pub(super) fn delete_password(service: &str, username: &str) -> bool {
    delete_generic_password(service, username).is_ok()
}
