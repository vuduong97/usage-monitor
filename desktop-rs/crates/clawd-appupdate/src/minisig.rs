//! minisign verification of `SHA256SUMS` (new in the port, Q2): without it nothing proves the
//! release came from us (Authenticode is on the backlog).

use clawd_contracts::UpdateError;

/// Verify that `sig` (the text of `SHA256SUMS.minisig`) is a valid minisign signature of `sums`
/// (the exact bytes of `SHA256SUMS`) under `public_key` (the base64 line of the `.pub` file;
/// the app passes [`clawd_contracts::MINISIGN_PUBKEY`]). A missing, malformed or wrong
/// signature, or an unparsable key (the phase-02 placeholder), is an [`UpdateError`] with a
/// Vietnamese message (fixtures record it): the update is refused, never "verified anyway".
pub fn verify_sums(sums: &[u8], sig: &str, public_key: &str) -> Result<(), UpdateError> {
    todo!("phase 10")
}
