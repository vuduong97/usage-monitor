//! Windows Credential Manager backend: `CredReadW` / `CredWriteW` / `CredDeleteW` through the
//! `windows` crate (features `Win32_Security_Credentials`), mirroring python-keyring's
//! `WinVaultKeyring` exactly.

use clawd_contracts::WINVAULT_COMMENT;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_ENTERPRISE,
    CRED_TYPE_GENERIC,
};

/// One generic credential as `win32cred.CredRead` returns it (the parts the keyring uses).
struct Stored {
    user: String,
    blob: Vec<u8>,
}

/// Why a Credential Manager call failed (anything except "not found", which is `Ok(None)`).
#[derive(Debug)]
struct CredError;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Overwrite a secret buffer so it does not linger in freed memory.
fn wipe(buf: &mut [u8]) {
    for byte in buf.iter_mut() {
        // SAFETY: `byte` is a valid, exclusive reference; a volatile write keeps the optimizer
        // from dropping the store.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

fn pwstr_to_string(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: the API returns NUL-terminated wide strings that stay valid until `CredFree`.
    unsafe { p.to_string().unwrap_or_default() }
}

/// `_read_credential(target)`: `Ok(None)` when it does not exist (winerror 1168).
fn read_credential(target: &str) -> Result<Option<Stored>, CredError> {
    let name = wide(target);
    let mut raw: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated and outlives the call; `raw` receives an API-owned buffer.
    let res = unsafe { CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut raw) };
    match res {
        Ok(()) => {}
        Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => return Ok(None),
        Err(_) => return Err(CredError),
    }
    if raw.is_null() {
        return Err(CredError);
    }
    // SAFETY: `raw` points to a CREDENTIALW allocated by CredReadW, valid until CredFree.
    let stored = unsafe {
        let cred = &*raw;
        let blob = if cred.CredentialBlob.is_null() || cred.CredentialBlobSize == 0 {
            Vec::new()
        } else {
            let secret = std::slice::from_raw_parts_mut(
                cred.CredentialBlob,
                cred.CredentialBlobSize as usize,
            );
            let copy = secret.to_vec();
            wipe(secret);
            copy
        };
        Stored {
            user: pwstr_to_string(cred.UserName),
            blob,
        }
    };
    // SAFETY: `raw` came from CredReadW and is freed exactly once.
    unsafe { CredFree(raw as *const _) };
    Ok(Some(stored))
}

/// `DecodingCredential.value`: the blob as UTF-16 (BOM honoured, little-endian otherwise), else
/// UTF-8 (a credential another tool wrote). `None` when neither decodes.
fn decode_blob(blob: &[u8]) -> Option<String> {
    let utf16 = blob.len().is_multiple_of(2).then(|| {
        let (big_endian, body) = match blob {
            [0xFF, 0xFE, rest @ ..] => (false, rest),
            [0xFE, 0xFF, rest @ ..] => (true, rest),
            _ => (false, blob),
        };
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|pair| {
                if big_endian {
                    u16::from_be_bytes([pair[0], pair[1]])
                } else {
                    u16::from_le_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        String::from_utf16(&units).ok()
    });
    utf16
        .flatten()
        .or_else(|| String::from_utf8(blob.to_vec()).ok())
}

/// `_set_password(target, username, password)`: a UTF-16LE blob without terminator,
/// `CRED_PERSIST_ENTERPRISE`, the python-keyring comment.
fn write_credential(target: &str, username: &str, blob: &mut [u8]) -> Result<(), CredError> {
    let mut target_w = wide(target);
    let mut user_w = wide(username);
    let mut comment_w = wide(WINVAULT_COMMENT);
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target_w.as_mut_ptr()),
        Comment: PWSTR(comment_w.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_ENTERPRISE,
        UserName: PWSTR(user_w.as_mut_ptr()),
        ..Default::default()
    };
    // SAFETY: every pointer in `credential` references a live buffer owned by this frame.
    let res = unsafe { CredWriteW(&credential, 0) };
    res.map_err(|_| CredError)
}

fn utf16le_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn write_text(target: &str, username: &str, text: &str) -> Result<(), CredError> {
    let mut blob = utf16le_bytes(text);
    let result = write_credential(target, username, &mut blob);
    wipe(&mut blob);
    result
}

fn compound_name(username: &str, service: &str) -> String {
    format!("{username}@{service}")
}

/// `WinVaultKeyring._resolve_credential`: the credential under `service` when it is this user's,
/// else the one under `username@service`.
fn resolve_credential(service: &str, username: &str) -> Result<Option<Stored>, CredError> {
    let first = read_credential(service)?;
    match first {
        Some(found) if username.is_empty() || found.user == username => Ok(Some(found)),
        _ => read_credential(&compound_name(username, service)),
    }
}

/// `WinVaultKeyring.get_password(service, username)`: read target `service` and accept it when
/// its `UserName` matches, else target `username@service`; the blob is decoded as UTF-16LE,
/// falling back to UTF-8.
pub(super) fn get_password(service: &str, username: &str) -> Option<String> {
    let mut found = resolve_credential(service, username).ok()??;
    let text = decode_blob(&found.blob);
    wipe(&mut found.blob);
    text
}

/// `WinVaultKeyring.set_password`: when `service` already holds a credential of another user it
/// is moved to `old_user@service`; the new one is written at `service` as a UTF-16LE blob with
/// `CRED_PERSIST_ENTERPRISE`, `UserName = username` and the python-keyring comment.
pub(super) fn set_password(service: &str, username: &str, secret: &str) -> bool {
    let existing = match read_credential(service) {
        Ok(existing) => existing,
        Err(CredError) => return false,
    };
    if let Some(mut old) = existing {
        // python-keyring re-saves it under the compound target (decoded, then written as UTF-16)
        let Some(mut text) = decode_blob(&old.blob) else {
            wipe(&mut old.blob);
            return false;
        };
        let saved = write_text(&compound_name(&old.user, service), &old.user, &text);
        wipe(&mut old.blob);
        // SAFETY: zeroing the bytes of a String keeps it valid UTF-8 (all NUL).
        wipe(unsafe { text.as_bytes_mut() });
        if saved.is_err() {
            return false;
        }
    }
    write_text(service, username, secret).is_ok()
}

/// `WinVaultKeyring.delete_password`: delete the credential for this user (either target).
pub(super) fn delete_password(service: &str, username: &str) -> bool {
    let mut deleted = false;
    for target in [service.to_owned(), compound_name(username, service)] {
        let Ok(Some(mut existing)) = read_credential(&target) else {
            continue;
        };
        wipe(&mut existing.blob);
        if existing.user == username {
            deleted = true;
            let name = wide(&target);
            // SAFETY: `name` is NUL-terminated and outlives the call.
            let res = unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) };
            if let Err(e) = res {
                if e.code() != ERROR_NOT_FOUND.to_hresult() {
                    return false;
                }
            }
        }
    }
    deleted
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawd_contracts::OsStoreNames;

    /// A throwaway service name (never the production one) whose credentials are deleted on
    /// drop, so a failing test leaves nothing in the Credential Manager.
    struct Sandbox {
        service: String,
    }

    impl Sandbox {
        fn new(tag: &str) -> Self {
            let names = OsStoreNames::for_test(&format!(
                "wincred-{tag}-{}-{}",
                std::process::id(),
                crate::prefs::thread_tag()
            ));
            assert!(!names.is_production(), "tests never touch production names");
            Self {
                service: names.keyring_service,
            }
        }

        fn cleanup_targets(&self) -> Vec<String> {
            ["a", "b", "admin"]
                .iter()
                .map(|u| compound_name(u, &self.service))
                .chain(std::iter::once(self.service.clone()))
                .collect()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            for target in self.cleanup_targets() {
                let name = wide(&target);
                // SAFETY: `name` is NUL-terminated and outlives the call.
                let _ = unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) };
            }
        }
    }

    #[test]
    fn blob_is_utf16le_with_python_keyring_attributes() {
        let sb = Sandbox::new("blob");
        assert!(set_password(&sb.service, "a", "pä\u{1F600}"));
        let stored = read_credential(&sb.service).unwrap().unwrap();
        assert_eq!(stored.user, "a");
        assert_eq!(stored.blob, utf16le_bytes("pä\u{1F600}"));
        assert_eq!(
            get_password(&sb.service, "a").as_deref(),
            Some("pä\u{1F600}")
        );
        // comment and persistence, read back raw
        let name = wide(&sb.service);
        let mut raw: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: as in `read_credential`.
        unsafe {
            CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut raw).unwrap();
            assert_eq!((*raw).Persist, CRED_PERSIST_ENTERPRISE);
            assert_eq!(pwstr_to_string((*raw).Comment), WINVAULT_COMMENT);
            CredFree(raw as *const _);
        }
    }

    #[test]
    fn another_user_moves_the_first_to_a_compound_target() {
        let sb = Sandbox::new("collision");
        assert!(set_password(&sb.service, "a", "pw-a"));
        assert!(set_password(&sb.service, "b", "pw-b"));
        assert_eq!(read_credential(&sb.service).unwrap().unwrap().user, "b");
        let moved = read_credential(&compound_name("a", &sb.service))
            .unwrap()
            .unwrap();
        assert_eq!(moved.user, "a");
        assert_eq!(get_password(&sb.service, "a").as_deref(), Some("pw-a"));
        assert_eq!(get_password(&sb.service, "b").as_deref(), Some("pw-b"));
        assert_eq!(get_password(&sb.service, "zz"), None);
        assert!(delete_password(&sb.service, "a"));
        assert_eq!(get_password(&sb.service, "a"), None);
        assert_eq!(get_password(&sb.service, "b").as_deref(), Some("pw-b"));
        assert!(delete_password(&sb.service, "b"));
        assert!(!delete_password(&sb.service, "b"));
    }

    #[test]
    fn utf8_blob_written_by_another_tool_is_read() {
        let sb = Sandbox::new("utf8");
        let mut blob = b"abc".to_vec(); // odd length: not UTF-16
        write_credential(&sb.service, "a", &mut blob).unwrap();
        assert_eq!(get_password(&sb.service, "a").as_deref(), Some("abc"));
    }

    #[test]
    fn blob_decoding_follows_python() {
        assert_eq!(
            decode_blob(&utf16le_bytes("héllo")).as_deref(),
            Some("héllo")
        );
        assert_eq!(decode_blob(b"\xff\xfeh\0i\0").as_deref(), Some("hi")); // LE BOM
        assert_eq!(decode_blob(b"\xfe\xff\0h\0i").as_deref(), Some("hi")); // BE BOM
        assert_eq!(decode_blob(b"\0\xd8").as_deref(), None); // lone surrogate, not UTF-8 either
        assert_eq!(decode_blob("héllos".as_bytes()).as_deref(), Some("héllos")); // odd length: UTF-8
        assert_eq!(decode_blob(b"").as_deref(), Some(""));
    }
}
