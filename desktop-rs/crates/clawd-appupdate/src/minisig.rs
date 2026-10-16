//! minisign verification of `SHA256SUMS` (new in the port, Q2): without it nothing proves the
//! release came from us (Authenticode is on the backlog).

use clawd_contracts::UpdateError;
use minisign_verify::{PublicKey, Signature};

/// The public key in the app is not a minisign key (the phase-02 placeholder, a typo in a fork
/// build): nothing can be verified, so nothing is installed.
pub const BAD_KEY: &str =
    "Khóa công khai minisign trong app không hợp lệ nên không xác minh được bản cập nhật";
/// `SHA256SUMS.minisig` is not a minisign signature file.
pub const BAD_SIGNATURE_FILE: &str = "SHA256SUMS.minisig không đọc được";
/// The signature does not match `SHA256SUMS` under the app's key.
pub const SIGNATURE_MISMATCH: &str =
    "Chữ ký của SHA256SUMS không hợp lệ: bản phát hành không đến từ nguồn đáng tin";
/// The release carries `SHA256SUMS` but no `SHA256SUMS.minisig`.
pub const SIGNATURE_MISSING: &str =
    "Bản phát hành thiếu SHA256SUMS.minisig nên không xác minh được nguồn gốc";

/// Verify that `sig` (the text of `SHA256SUMS.minisig`) is a valid minisign signature of `sums`
/// (the exact bytes of `SHA256SUMS`) under `public_key` (the base64 line of the `.pub` file;
/// the app passes [`clawd_contracts::MINISIGN_PUBKEY`]). A missing, malformed or wrong
/// signature, or an unparsable key (the phase-02 placeholder), is an [`UpdateError`] with a
/// Vietnamese message (fixtures record it): the update is refused, never "verified anyway".
///
/// Both the plain (`Ed`) and the pre-hashed (`ED`) minisign signature forms are accepted:
/// `SHA256SUMS` is a few hundred bytes, so the legacy form is just as strong.
pub fn verify_sums(sums: &[u8], sig: &str, public_key: &str) -> Result<(), UpdateError> {
    let key =
        PublicKey::from_base64(public_key.trim()).map_err(|_| UpdateError(BAD_KEY.to_owned()))?;
    let signature =
        Signature::decode(sig).map_err(|_| UpdateError(BAD_SIGNATURE_FILE.to_owned()))?;
    key.verify(sums, &signature, true)
        .map_err(|_| UpdateError(SIGNATURE_MISMATCH.to_owned()))
}
