//! `feed.decrypt` and the per-file check (`Feed._open`).
//!
//! Every file is `nonce(12) | ciphertext | tag(16)` with AAD `clawd-fw/<seq>/<name>`
//! (`feed.py:170-178`).

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use clawd_contracts::{FeedError, FeedFile};

use crate::crc32::crc32_hex;
use crate::firmware::sha256_hex;

/// `feed.decrypt(key, seq, name, blob)`: AES-256-GCM; `blob` is `nonce(12) | ciphertext |
/// tag(16)`, AAD `clawd-fw/<seq>/<name>`. `FeedError` when too short, the key is not 32 bytes, or
/// it does not decrypt.
pub fn decrypt(key: &[u8], seq: u64, name: &str, blob: &[u8]) -> Result<Vec<u8>, FeedError> {
    if blob.len() < 28 {
        return Err(FeedError(format!("{name}: too short")));
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| FeedError(format!("{name}: the release key is not 32 bytes")))?;
    let (nonce, rest) = blob.split_at(12);
    let nonce = Nonce::try_from(nonce).map_err(|_| FeedError(format!("{name}: too short")))?;
    let aad = format!("clawd-fw/{seq}/{name}");
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: rest,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| FeedError(format!("{name}: does not decrypt (wrong key or damaged)")))
}

/// The publisher's side (`clawd-home feed/release.mjs`): `nonce | ciphertext | tag` for `data`.
/// The app never encrypts; the testkit's fake feed and the golden tests use it. `nonce` must be
/// unique per message under one key.
pub fn encrypt(
    key: &[u8],
    seq: u64,
    name: &str,
    nonce: [u8; 12],
    data: &[u8],
) -> Result<Vec<u8>, FeedError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| FeedError(format!("{name}: the release key is not 32 bytes")))?;
    let n = Nonce::try_from(&nonce[..]).map_err(|_| FeedError(format!("{name}: bad nonce")))?;
    let aad = format!("clawd-fw/{seq}/{name}");
    let sealed = cipher
        .encrypt(
            &n,
            Payload {
                msg: data,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| FeedError(format!("{name}: cannot encrypt")))?;
    let mut out = nonce.to_vec();
    out.extend(sealed);
    Ok(out)
}

/// `Feed._open(key, seq, f, blob)`: the plaintext of one file, checked against its manifest row:
/// encrypted size, then size, SHA-256 and CRC of the plaintext.
pub fn open(key: &[u8], seq: u64, f: &FeedFile, blob: &[u8]) -> Result<Vec<u8>, FeedError> {
    if blob.len() as u64 != f.enc {
        return Err(FeedError(format!(
            "{}: {} B, the manifest says {}",
            f.name,
            blob.len(),
            f.enc
        )));
    }
    let data = decrypt(key, seq, &f.name, blob)?;
    if data.len() as u64 != f.size || sha256_hex(&data) != f.sha256 || crc32_hex(&data) != f.crc {
        return Err(FeedError(format!(
            "{}: does not match the manifest",
            f.name
        )));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_failures() {
        let key = [3u8; 32];
        let blob = encrypt(&key, 6, "clawd.pak", [7; 12], b"hello").unwrap();
        assert_eq!(blob.len(), 5 + 28);
        assert_eq!(decrypt(&key, 6, "clawd.pak", &blob).unwrap(), b"hello");
        // wrong seq, name, key: all change the AAD / key
        assert!(decrypt(&key, 7, "clawd.pak", &blob).is_err());
        assert!(decrypt(&key, 6, "desk.pak", &blob).is_err());
        assert!(decrypt(&[4u8; 32], 6, "clawd.pak", &blob).is_err());
        assert_eq!(
            decrypt(&key, 6, "x", &blob[..27]).unwrap_err().0,
            "x: too short"
        );
        let mut bad = blob.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(
            decrypt(&key, 6, "clawd.pak", &bad).unwrap_err().0,
            "clawd.pak: does not decrypt (wrong key or damaged)"
        );
    }
}
