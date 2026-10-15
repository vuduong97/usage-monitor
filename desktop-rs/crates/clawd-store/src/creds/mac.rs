//! macOS Keychain backend: generic password with `kSecAttrService = service` and
//! `kSecAttrAccount = username` through `security-framework` (the same mapping python-keyring's
//! macOS backend uses, so items saved by 0.1.4 are found).

/// `get_generic_password(service, account)` as text.
pub(super) fn get_password(service: &str, username: &str) -> Option<String> {
    todo!("phase 03")
}

/// `set_generic_password(service, account, secret)`.
pub(super) fn set_password(service: &str, username: &str, secret: &str) -> bool {
    todo!("phase 03")
}

/// `delete_generic_password(service, account)`.
pub(super) fn delete_password(service: &str, username: &str) -> bool {
    todo!("phase 03")
}
