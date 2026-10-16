use serde::{Deserialize, Serialize};

/// `appupdate.AppRelease`: a published GitHub release newer than the running app, with this
/// platform's installer and its checksum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRelease {
    /// `"1.2.3"`.
    pub version: String,
    /// `"v1.2.3"`.
    pub tag: String,
    /// Release notes, capped at 2000 chars.
    pub notes: String,
    /// The release's web page.
    pub page: String,
    /// The asset for this platform (`Clawd-X.Y.Z-windows-setup.exe`, `...-mac-arm64.zip`).
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Expected SHA-256 from `SHA256SUMS` (verified by minisign first), 64 lowercase hex.
    pub sha256: String,
}
