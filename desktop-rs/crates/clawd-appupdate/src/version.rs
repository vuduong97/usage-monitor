//! Pure helpers (`appupdate.py:82-130`), all tested by golden fixtures.

use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

/// `appupdate.parse_version(s)`: `v1.2.3` / `1.2.3` -> `(1, 2, 3)` (each part 1-4 digits).
/// Anything else (`1.2`, `1.2.3-rc1`, ...) is not a release.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut parts = s.split('.');
    let (a, b, c) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let number = |part: &str| -> Option<u32> {
        let ok = (1..=4).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit());
        if ok {
            part.parse().ok()
        } else {
            None
        }
    };
    Some((number(a)?, number(b)?, number(c)?))
}

/// `appupdate.is_newer(candidate, current)`: both parse and `candidate > current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// `appupdate.this_platform()`: `("mac" | "windows" | "linux", "arm64" | "x86_64" | ...)`.
pub fn this_platform() -> (String, String) {
    let system = if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    };
    (system.to_owned(), arch.to_owned())
}

/// `appupdate.asset_name(version, system, arch)`: the release file for this platform
/// (`Clawd-X.Y.Z-mac-<arch>.zip`, `Clawd-X.Y.Z-windows-setup.exe`); `None` where there is no
/// self-update.
pub fn asset_name(version: &str, system: &str, arch: &str) -> Option<String> {
    if system == "mac" && (arch == "arm64" || arch == "x86_64") {
        return Some(format!("Clawd-{version}-mac-{arch}.zip"));
    }
    if system == "windows" {
        return Some(format!("Clawd-{version}-windows-setup.exe"));
    }
    None
}

/// `str.splitlines()`: Python also breaks on `\r`, `\x0b`, `\x0c`, `\x1c-\x1e`, `\x85`,
/// `\u2028` and `\u2029`, not only on `\n`.
fn py_splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\x0b'
                | '\x0c'
                | '\x1c'
                | '\x1d'
                | '\x1e'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if !is_break {
            continue;
        }
        lines.push(&text[start..i]);
        let mut end = i + c.len_utf8();
        if c == '\r' && matches!(chars.peek(), Some((_, '\n'))) {
            chars.next();
            end += 1;
        }
        start = end;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// `appupdate.parse_sums(text)`: `sha256sum` output (`<64 hex>  <name>`, `*` before the name
/// marks binary mode) -> `{name: lowercase hex}`.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in py_splitlines(text) {
        let line = line.trim();
        let Some((digest, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let digest = digest.to_lowercase();
        let is_hex64 =
            digest.len() == 64 && digest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if is_hex64 {
            let name = rest.trim_start().trim_start_matches('*').trim();
            out.insert(name.to_owned(), digest);
        }
    }
    out
}

/// `appupdate.team_id(codesign_output)`: the Team ID in `codesign -dv --verbose=2` output;
/// `None` for an ad-hoc signature.
pub fn team_id(codesign_output: &str) -> Option<String> {
    // `^TeamIdentifier=(\S+)$` with re.M: lines end at `\n` only, the value has no whitespace.
    for line in codesign_output.split('\n') {
        let Some(value) = line.strip_prefix("TeamIdentifier=") else {
            continue;
        };
        if !value.is_empty() && !value.chars().any(char::is_whitespace) {
            return (value != "not").then(|| value.to_owned());
        }
    }
    None
}

/// `appupdate.sha256_file(path)`: streaming SHA-256, lowercase hex.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10.0"), Some((0, 10, 0)));
        for bad in ["1.2", "v1.2.3-rc1", "1.2.3.4", "latest", "", "12345.1.1", "V1.2.3"] {
            assert_eq!(parse_version(bad), None, "{bad:?}");
        }
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("v1.0.0-rc1", "0.1.0"));
    }

    #[test]
    fn test_asset_names_sums_and_team_id() {
        assert_eq!(
            asset_name("1.2.3", "mac", "arm64").as_deref(),
            Some("Clawd-1.2.3-mac-arm64.zip")
        );
        assert_eq!(
            asset_name("1.2.3", "mac", "x86_64").as_deref(),
            Some("Clawd-1.2.3-mac-x86_64.zip")
        );
        assert_eq!(
            asset_name("1.2.3", "windows", "x86_64").as_deref(),
            Some("Clawd-1.2.3-windows-setup.exe")
        );
        assert_eq!(asset_name("1.2.3", "linux", "x86_64"), None);
        assert_eq!(asset_name("1.2.3", "mac", "ppc"), None);

        let h = "a".repeat(64);
        let sums = parse_sums(&format!(
            "{h}  one.zip\n{} *two.exe\nbad  three\n",
            h.to_uppercase()
        ));
        assert_eq!(sums.len(), 2);
        assert_eq!(sums["one.zip"], h);
        assert_eq!(sums["two.exe"], h);

        assert_eq!(
            team_id("Identifier=x\nTeamIdentifier=ABCDE12345\n").as_deref(),
            Some("ABCDE12345")
        );
        assert_eq!(team_id("TeamIdentifier=not set\n"), None);
        assert_eq!(team_id(""), None);
        // a `\r` is part of `\S+`'s neighbour, not a line end: no match, like the regex
        assert_eq!(team_id("TeamIdentifier=ABC\r\n"), None);
    }

    #[test]
    fn splitlines_breaks_like_python() {
        assert_eq!(py_splitlines("a\r\nb\rc\n"), vec!["a", "b", "c"]);
        assert_eq!(py_splitlines("a\u{2028}b"), vec!["a", "b"]);
        assert_eq!(py_splitlines(""), Vec::<&str>::new());
        assert_eq!(py_splitlines("\n\nx"), vec!["", "", "x"]);
    }

    #[test]
    fn sha256_file_streams() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(
            sha256_file(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
