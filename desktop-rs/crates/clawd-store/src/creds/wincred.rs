//! Windows Credential Manager backend: `CredReadW` / `CredWriteW` / `CredDeleteW` through the
//! `windows` crate (features `Win32_Security_Credentials`), mirroring python-keyring's
//! `WinVaultKeyring` exactly.
#![allow(unused_variables, dead_code)] // milestone stub: removed when implemented

/// `WinVaultKeyring.get_password(service, username)`: read target `service` and accept it when
/// its `UserName` matches, else target `username@service`; the blob is decoded as UTF-16LE,
/// falling back to UTF-8.
pub(super) fn get_password(service: &str, username: &str) -> Option<String> {
    todo!("phase 03")
}

/// `WinVaultKeyring.set_password`: when `service` already holds a credential of another user it
/// is moved to `old_user@service`; the new one is written at `service` as a UTF-16LE blob with
/// `CRED_PERSIST_ENTERPRISE`, `UserName = username` and the python-keyring comment.
pub(super) fn set_password(service: &str, username: &str, secret: &str) -> bool {
    todo!("phase 03")
}

/// `WinVaultKeyring.delete_password`: delete the credential for this user (either target).
pub(super) fn delete_password(service: &str, username: &str) -> bool {
    todo!("phase 03")
}
