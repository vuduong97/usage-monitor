use clawd_contracts::AppPaths;

use crate::config_file::read_config_value;
use crate::pytext::strip;

/// `usage.HTTP_PUSH_PATH`: appended to a bare host.
const HTTP_PUSH_PATH: &str = "/api/usage";

/// `usage.normalize_http_target(raw)`: an `http_targets` entry as a push URL, or `None` if
/// unusable. A bare host or IP (`192.168.0.125`, `clawd.local:80`) becomes
/// `http://<host>/api/usage`; a full `http(s)` URL keeps its own path.
///
/// Follows `urllib.parse.urlsplit`/`urlunsplit`; input Python's `urlsplit` rejects with a
/// `ValueError` (unbalanced `[` `]` in the host) is `None` here instead of an exception.
pub fn normalize_http_target(raw: &str) -> Option<String> {
    let raw = strip(raw);
    if raw.is_empty() {
        return None;
    }
    // urlsplit drops tabs and newlines anywhere in the URL
    let mut url: String = raw
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    if !url.contains("://") {
        url = format!("http://{url}");
    }
    let parts = split_url(&url)?;
    if !matches!(parts.scheme.as_str(), "http" | "https") || host_of(parts.netloc).is_empty() {
        return None;
    }
    let path = if matches!(parts.path, "" | "/") {
        HTTP_PUSH_PATH
    } else {
        parts.path
    };
    let query = if parts.query.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.query)
    };
    Some(format!(
        "{}://{}{}{}",
        parts.scheme, parts.netloc, path, query
    ))
}

struct UrlParts<'a> {
    scheme: String,
    netloc: &'a str,
    path: &'a str,
    query: &'a str,
}

/// `urllib.parse.urlsplit(url)` (the fragment is dropped). `None` where Python raises.
fn split_url(url: &str) -> Option<UrlParts<'_>> {
    let mut rest = url;
    let mut scheme = String::new();
    if let Some(i) = rest.find(':') {
        let name = &rest[..i];
        let mut chars = name.chars();
        let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
        if valid {
            scheme = name.to_ascii_lowercase();
            rest = &rest[i + 1..];
        }
    }
    let mut netloc = "";
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        netloc = &after[..end];
        rest = &after[end..];
        if netloc.contains('[') != netloc.contains(']') {
            return None;
        }
    }
    let rest = rest.split('#').next().unwrap_or("");
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    Some(UrlParts {
        scheme,
        netloc,
        path,
        query,
    })
}

/// `SplitResult.hostname`, lower-cased (empty when there is none).
fn host_of(netloc: &str) -> String {
    let hostinfo = netloc.rsplit('@').next().unwrap_or("");
    let host = match hostinfo.split_once('[') {
        Some((_, bracketed)) => bracketed.split(']').next().unwrap_or(""),
        None => hostinfo.split(':').next().unwrap_or(""),
    };
    host.to_lowercase()
}

/// `usage.read_http_targets()`: push URLs from the comma-separated `http_targets` option, in
/// order, deduplicated; empty = WiFi push by config off (the finder's mDNS pick is used).
pub fn read_http_targets(paths: &AppPaths) -> Vec<String> {
    let raw = read_config_value(paths, "http_targets").unwrap_or_default();
    let mut targets: Vec<String> = Vec::new();
    for entry in raw.split(',') {
        match normalize_http_target(entry) {
            Some(url) => {
                if !targets.contains(&url) {
                    targets.push(url);
                }
            }
            None if !strip(entry).is_empty() => {
                tracing::warn!("Ignoring invalid http_targets entry: {:?}", strip(entry));
            }
            None => {}
        }
    }
    targets
}
