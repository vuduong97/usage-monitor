use serde::{Deserialize, Serialize};

/// `feed.FeedFile`: one row of a feed manifest's `files` or `renderers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedFile {
    /// `[a-z0-9][a-z0-9._-]{0,47}` with an optional one-level `/dir`.
    pub name: String,
    /// `fw | clawdpak | deskpak | styles | style` (renderers: `renderer`-like kinds as the feed sends).
    pub kind: String,
    pub size: u64,
    /// Encrypted size, always `size + 28` (nonce 12 + tag 16).
    pub enc: u64,
    /// 64 lowercase hex.
    pub sha256: String,
    /// 8 lowercase hex CRC-32.
    pub crc: String,
    /// A renderer's platform: `win-x64 | mac | linux-x64`; empty for `files` rows.
    pub platform: String,
}

/// `feed.Manifest`: a feed release (manifest v1 + `renderers[]`, plan.md frozen contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedManifest {
    /// Manifest version; always 1 (`parse_manifest` rejects anything else).
    #[serde(default = "manifest_v1")]
    pub v: u32,
    pub seq: u64,
    pub variant: String,
    pub layouts: u32,
    #[serde(rename = "styleApi")]
    pub style_api: u32,
    pub files: Vec<FeedFile>,
    pub date: String,
    pub notes: String,
    pub commit: String,
    /// One zip per platform, kept apart from `files` (older apps reject unknown kinds there).
    pub renderers: Vec<FeedFile>,
}

fn manifest_v1() -> u32 {
    1
}

impl FeedManifest {
    /// `Manifest.file(kind)`: the first `files` row of that kind.
    pub fn file(&self, kind: &str) -> Option<&FeedFile> {
        self.files.iter().find(|f| f.kind == kind)
    }

    /// `Manifest.renderer(platform)`.
    pub fn renderer(&self, platform: &str) -> Option<&FeedFile> {
        self.renderers.iter().find(|f| f.platform == platform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, kind: &str, platform: &str) -> FeedFile {
        FeedFile {
            name: name.into(),
            kind: kind.into(),
            size: 1,
            enc: 29,
            sha256: "0".repeat(64),
            crc: "00000000".into(),
            platform: platform.into(),
        }
    }

    #[test]
    fn lookup_by_kind_and_platform() {
        let m = FeedManifest {
            v: 1,
            seq: 7,
            variant: "esp8266-clawd".into(),
            layouts: 1,
            style_api: 0,
            files: vec![row("fw.bin", "fw", "")],
            date: String::new(),
            notes: String::new(),
            commit: String::new(),
            renderers: vec![row("r-mac.zip", "renderer", "mac")],
        };
        assert_eq!(m.file("fw").map(|f| f.name.as_str()), Some("fw.bin"));
        assert!(m.file("deskpak").is_none());
        assert_eq!(
            m.renderer("mac").map(|f| f.name.as_str()),
            Some("r-mac.zip")
        );
        assert!(m.renderer("win-x64").is_none());
    }
}
