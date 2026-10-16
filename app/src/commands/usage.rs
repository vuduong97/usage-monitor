//! Settings commands: usage source and preview (`settings_api.py`).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use std::sync::Arc;

use clawd_config::{read_config_dirs, read_onoff_setting, read_usage_source};
use clawd_firmware::firmware::Bundle;
use clawd_firmware::preview::{self, PakCache, CLASSIC, PAK_MAX};
use clawd_usage::codex;
use serde_json::{json, Map, Value};

use super::session::{arg, base64, err, fail, text_of, Bridge, FRAME_CACHE_SIZE};
use super::CommandCtx;
use crate::state::ShellCmd;

/// How long to wait before asking the clock for `/clawd.pak` again after a failure (s).
const PAK_RETRY_S: f64 = 30.0;

/// `SettingsApi.usage_preview(styles, motion)`: the clock's usage screen in each style
/// (`"classic"` or a pack id), with the usage the app last sent it.
///
/// `{"ok": true, "exact": true, "frames": {"classic": "data:image/..."}, "sentAt": <epoch>|null}`
/// when the clock's own renderer is built into the app. With `exact` false there are no frames
/// and the page draws its approximation from `payload`.
pub async fn usage_preview(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    usage_preview_on(&Bridge::of(ctx), &args).await
}

/// `usage_preview` on a bridge (the command without the Tauri context).
pub async fn usage_preview_on(b: &Bridge, args_in: &[Value]) -> Result<Value, String> {
    let args = args_in.to_vec();
    Ok(preview_of(&b.clone(), &args).await)
}

async fn preview_of(b: &Bridge, args: &[Value]) -> Value {
    // The page is untrusted input.
    let motion = arg(args, 1).as_bool().unwrap_or(true);
    let src = b.src();
    let mut known = vec![CLASSIC.to_owned()];
    known.extend(
        src.iter()
            .flat_map(|r| r.styles.iter().map(|p| p.id.clone())),
    );
    let asked: Option<Vec<String>> = arg(args, 0).as_array().and_then(|list| {
        let ids: Option<Vec<String>> = list.iter().map(|v| v.as_str().map(str::to_owned)).collect();
        ids.filter(|ids| !ids.is_empty() && ids.iter().all(|i| known.contains(i)))
    });
    let styles = asked.unwrap_or(known);

    let stored = clawd_store::paths::usage_payload(b.paths());
    let (payload, sent_at) = match stored {
        Some((p, at)) => (Some(p), Some(at)),
        None => (None, None),
    };
    let now = b.now();
    let live = preview::live_payload(payload.as_ref(), now);
    let mut out = json!({
        "ok": true, "exact": false, "payload": live, "sentAt": sent_at, "now": now,
    });

    // Once: the feed task may swap the renderer and its cache meanwhile.
    let (first_look, env, generation) = {
        let mut d = b.data();
        let first = !std::mem::replace(&mut d.renderer_loaded, true);
        (
            first,
            d.env.clone().unwrap_or_else(clawd_contracts::Env::system),
            d.renderer_generation,
        )
    };
    if first_look {
        // The bundled renderer belongs to the bundled firmware, even when the feed is newer
        // (`settings_app.py`: `firmware_commit=bundle.commit`). Loading it reads files, so it
        // runs outside the session lock.
        let commit = renderer_commit(b.bundle());
        let loaded =
            tokio::task::spawn_blocking(move || preview::load_renderer(&env, None, &commit))
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("[settings] loading the preview renderer failed: {e}");
                    None
                });
        let mut d = b.data();
        if d.renderer_generation == generation && d.renderer.is_none() {
            d.renderer = loaded.map(Arc::new);
        }
    }
    let (renderer, generation) = {
        let d = b.data();
        (d.renderer.clone(), d.renderer_generation)
    };
    let Some(renderer) = renderer else {
        return out;
    };
    let kind = if motion { "clip" } else { "still" };
    let mut frames = Map::new();
    for style in dedup(&styles) {
        if style != CLASSIC && !renderer.styles.contains_key(&style) {
            continue;
        }
        let key = preview::frame_key(&style, live.as_ref(), (now / 60.0) as u64, kind);
        let cached = {
            let mut d = b.data();
            if d.renderer_generation != generation {
                return out;
            }
            d.frames
                .get_or_insert_with(|| preview::FrameCache::new(FRAME_CACHE_SIZE))
                .get(&key)
        };
        let url = match cached {
            Some(url) => url,
            None => {
                let drawn = if motion {
                    renderer
                        .clip(&style, live.as_ref())
                        .await
                        .map(|f| preview::apng_data_url(&f))
                } else {
                    match renderer.draw(&style, live.as_ref()).await {
                        Ok(rgb) => preview::bmp_data_url(&rgb),
                        Err(e) => Err(e),
                    }
                };
                match drawn {
                    Ok(url) => {
                        let mut d = b.data();
                        if d.renderer_generation == generation {
                            d.frames
                                .get_or_insert_with(|| preview::FrameCache::new(FRAME_CACHE_SIZE))
                                .put(&key, url.clone());
                        }
                        url
                    }
                    Err(e) => {
                        tracing::warn!("[settings] preview renderer failed: {e}");
                        return out; // the page falls back to its canvas
                    }
                }
            }
        };
        frames.insert(style, Value::String(url));
    }
    out["exact"] = json!(true);
    out["frames"] = Value::Object(frames);
    out
}

/// The commit the bundled renderer must match: the bundled firmware's, `""` without one.
fn renderer_commit(bundle: Option<&Bundle>) -> String {
    bundle.map(|x| x.commit.clone()).unwrap_or_default()
}

fn dedup(styles: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in styles {
        if !out.contains(s) {
            out.push(s.clone());
        }
    }
    out
}

/// What the clock's usage screen can show, and the current choice (`usage_sources`).
fn sources_of(b: &Bridge) -> Value {
    let paths = b.paths();
    let env = b.env();
    let dirs: Vec<_> = read_config_dirs(paths)
        .into_iter()
        .filter(|d| d.is_dir())
        .collect();
    let cliproxy = read_onoff_setting(paths, "cliproxy", "off") == "on";
    let dir_name = |d: &std::path::Path| {
        d.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut opts: Vec<Value> = Vec::new();
    if dirs.len() > 1 || cliproxy {
        opts.push(json!({
            "id": "claude", "label": "Claude: mọi tài khoản",
            "detail": "Tự hiện tài khoản đang dùng nhiều nhất.", "ok": true,
        }));
        for d in &dirs {
            opts.push(json!({
                "id": format!("claude:{}", d.display()), "label": format!("Claude: {}", dir_name(d)),
                "detail": d.display().to_string(), "ok": true,
            }));
        }
    } else {
        opts.push(json!({
            "id": "claude", "label": "Claude Code", "ok": !dirs.is_empty(),
            "detail": dirs.first().map_or("Chưa thấy Claude Code trên máy này.".to_owned(), |d| d.display().to_string()),
        }));
    }
    let codex_ok = codex::available(paths, &env);
    let codex_now = if codex_ok {
        codex::read(paths, &env, None)
    } else {
        None
    };
    let detail = if codex_now.is_some() {
        "Theo lượt Codex gần nhất trên máy này."
    } else if codex_ok {
        "Gói Codex này chưa có giới hạn 5 giờ / tuần để hiện."
    } else {
        "Chưa thấy Codex CLI trên máy này."
    };
    opts.push(json!({"id": "codex", "label": "Codex CLI", "ok": codex_ok, "detail": detail}));
    opts.push(json!({
        "id": "off", "label": "Không hiện usage", "ok": true,
        "detail": "Đồng hồ chỉ hiện giờ và Clawd.",
    }));
    json!({"ok": true, "options": opts, "current": read_usage_source(paths)})
}

/// `SettingsApi.usage_sources`.
pub async fn usage_sources(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    usage_sources_on(&Bridge::of(ctx), &args).await
}

/// `usage_sources` on a bridge (the command without the Tauri context).
pub async fn usage_sources_on(b: &Bridge, _args: &[Value]) -> Result<Value, String> {
    b.clone().blocking(|b| Ok(sources_of(&b))).await
}

/// `SettingsApi.set_usage_source(value)`.
pub async fn set_usage_source(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    set_usage_source_on(&Bridge::of(ctx), &args).await
}

/// `set_usage_source` on a bridge (the command without the Tauri context).
pub async fn set_usage_source_on(b: &Bridge, args_in: &[Value]) -> Result<Value, String> {
    let args = args_in.to_vec();
    let value = arg(&args, 0).as_str().unwrap_or_default().to_owned();
    b.clone()
        .blocking(move |b| set_usage_source_of(&b, value))
        .await
}

fn set_usage_source_of(b: &Bridge, value: String) -> Result<Value, String> {
    let listed = sources_of(b)["options"].as_array().is_some_and(|opts| {
        opts.iter()
            .any(|o| o["id"].as_str() == Some(value.as_str()))
    });
    if !listed {
        return Ok(fail("bad", "Nguồn usage không hợp lệ."));
    }
    let saved = clawd_store::prefs::update_prefs(b.paths(), |prefs| {
        if value == "claude" {
            prefs.0.remove("usage_source"); // the default: also what older versions did
        } else {
            prefs
                .0
                .insert("usage_source".into(), Value::String(value.clone()));
        }
    });
    if let Err(e) = saved {
        return Ok(fail("io", &format!("Không lưu được lựa chọn: {e}")));
    }
    b.notify(ShellCmd::UsageSource);
    Ok(json!({"ok": true, "current": read_usage_source(b.paths())}))
}

/// `SettingsApi.clawd_pak`: the clock's Clawd sprites (`/clawd.pak`) for the page's approximate
/// preview, which has no renderer: `{"ok": true, "pak": "<base64>"}`. Kept in the config folder
/// and read again only when `/api/heap` reports a different pack; never part of the app itself.
/// `{"ok": false}` when there is none to be had (the page then draws the app icon).
pub async fn clawd_pak(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    clawd_pak_on(&Bridge::of(ctx), &args).await
}

/// `clawd_pak` on a bridge (the command without the Tauri context).
pub async fn clawd_pak_on(b: &Bridge, _args: &[Value]) -> Result<Value, String> {
    Ok(pak_of(&b.clone()).await)
}

async fn pak_of(b: &Bridge) -> Value {
    let cache = PakCache::new(b.paths().pak_file());
    let have = cache.load();
    let have_json = |bytes: &[u8]| json!({"ok": true, "pak": base64(bytes)});
    let want = b
        .data()
        .heap
        .get("pak")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .unwrap_or(0);
    if let Some((bytes, count)) = &have {
        if want == 0 || u64::from(*count) == want {
            return have_json(bytes);
        }
    }
    let (base, _) = b.address();
    let now = b.now();
    let recently_failed = b.data().pak_failed.is_some_and(|at| now - at < PAK_RETRY_S);
    if want == 0 || base.is_empty() || recently_failed {
        return match &have {
            Some((bytes, _)) => have_json(bytes),
            None => fail("nopak", "Đồng hồ chưa có hình Clawd."),
        };
    }
    let fetched = match b.api().get_file("/clawd.pak", PAK_MAX).await {
        Ok(data) => preview::pak_count(&data)
            .map(|_| data)
            .map_err(|e| fail("error", &format!("Lỗi: {e}"))),
        Err(e) => Err(err(&e)),
    };
    match fetched {
        Ok(data) => {
            let _ = cache.save(&data, u32::try_from(want).unwrap_or(u32::MAX));
            have_json(&data)
        }
        Err(failure) => {
            b.data().pak_failed = Some(now);
            tracing::warn!(
                "[settings] could not read clawd.pak from the clock: {}",
                text_of(&failure["error"])
            );
            match &have {
                Some((bytes, _)) => have_json(bytes),
                None => failure,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(commit: &str) -> Bundle {
        Bundle {
            path: "fw.bin".into(),
            size: 1,
            sha256: String::new(),
            variant: String::new(),
            seq: 1,
            layouts: 1,
            commit: commit.into(),
            style_api: 0,
            styles: Vec::new(),
        }
    }

    #[test]
    fn the_renderer_commit_is_the_bundled_firmwares() {
        assert_eq!(renderer_commit(Some(&bundle("abc123"))), "abc123");
        assert_eq!(renderer_commit(None), "");
    }
}
