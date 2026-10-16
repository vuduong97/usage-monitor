//! The renderer of a feed release on this computer: reuse the unpacked one when it still checks
//! out, else download, unpack and verify it once (`SettingsApi._use_feed_renderer`,
//! `settings_api.py:259-288`, plan `261014-renderer-via-feed`).
//!
//! Any failure leaves the page with what it had (a bundled renderer, or its canvas): never an
//! error, only a log line. A bundled renderer still wins when the bundle is the source: the
//! caller only asks for a feed renderer when the release shown comes from the feed.

use std::path::{Path, PathBuf};

use crate::feed::{Feed, Release};
use crate::preview::{load_unpacked, unpack_renderer, Renderer};

/// `SettingsApi._use_feed_renderer(feed, rel)` without the window state: the renderer of `rel`
/// for `platform` (`feed::renderer_platform()`), unpacked under `root` (`AppPaths::renderer_dir`).
/// `None` when the release has none for this computer, the download or the unpack failed, or the
/// result is built from another commit than the release. The caller then swaps its renderer and
/// drops its frame cache together.
pub async fn load_feed_renderer(
    feed: &Feed,
    rel: &Release,
    root: &Path,
    platform: &str,
) -> Option<Renderer> {
    let m = rel.manifest.as_ref()?;
    let f = m.renderer(platform)?;
    let commit = m
        .commit
        .strip_suffix("-dirty")
        .unwrap_or(&m.commit)
        .to_owned();
    let (root, sha) = (root.to_path_buf(), f.sha256.clone());
    if let Some(r) = load_blocking(&root, &sha, &commit).await {
        return Some(r);
    }
    let data = match feed.fetch_renderer(rel, platform).await {
        Ok(Some((_, data))) => data,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!("renderer from the feed: {e}");
            return None;
        }
    };
    let (root2, sha2) = (root.clone(), sha.clone());
    let unpacked: Result<PathBuf, String> = tokio::task::spawn_blocking(move || {
        unpack_renderer(&data, &root2, &sha2).map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    if let Err(e) = unpacked {
        tracing::warn!("renderer from the feed: {e}");
        return None;
    }
    let r = load_blocking(&root, &sha, &commit).await;
    match &r {
        None => tracing::warn!(
            "renderer from the feed (release {}) does not match it",
            m.seq
        ),
        Some(r) => tracing::info!(
            "preview: feed renderer {} (release {}, {} styles)",
            if r.commit.is_empty() { "?" } else { &r.commit },
            m.seq,
            r.styles.len()
        ),
    }
    r
}

async fn load_blocking(root: &Path, sha: &str, commit: &str) -> Option<Renderer> {
    let (root, sha, commit) = (root.to_path_buf(), sha.to_owned(), commit.to_owned());
    tokio::task::spawn_blocking(move || load_unpacked(&root, &sha, &commit))
        .await
        .ok()
        .flatten()
}
