//! Settings commands: talking to the clock (`settings_api.py`): login, save, find, WiFi scan,
//! reboot, factory reset.
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use std::sync::Arc;
use std::time::Duration;

use clawd_clock::{manual_clock, stored_clock};
use clawd_contracts::{ClockError, DEFAULT_CLOCK_USER};
use clawd_firmware::firmware;
use clawd_firmware::preview::CLASSIC;
use clawd_store::creds;
use serde_json::{json, Value};
use tokio::time::Instant;

use super::session::{arg, bad_value, diff, err, fail, merged, truthy, Bridge, TYPE_IP};
use super::CommandCtx;
use crate::state::ShellCmd;

/// `SettingsApi.login(password, user)`.
pub async fn login(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    login_on(&Bridge::of(ctx), &args).await
}

/// `login` on a bridge (the command without the Tauri context).
pub async fn login_on(b: &Bridge, args_in: &[Value]) -> Result<Value, String> {
    let args = args_in.to_vec();
    let password = arg(&args, 0).as_str().unwrap_or_default();
    let user = arg(&args, 1).as_str().unwrap_or(DEFAULT_CLOCK_USER);
    if password.is_empty() || user.is_empty() {
        return Ok(fail("bad", "Nhập mật khẩu in trên thẻ đi kèm đồng hồ."));
    }
    let (_, key) = b.address();
    let keyring = Arc::clone(&b.data().keyring);
    let api = b.api();
    api.set_auth(Some((user.to_owned(), password.to_owned())));
    if let Err(e) = api.config().await {
        api.set_auth(creds::get_with(&*keyring, &b.services.names, &key));
        return Ok(match e {
            ClockError::AuthNeeded => fail("auth", "Sai mật khẩu."),
            other => err(&other),
        });
    }
    let saved = creds::put_with(&*keyring, &b.services.names, &key, user, password);
    Ok(merged(b.state().await, json!({"saved": saved})))
}

/// `SettingsApi.save(edited, style)`: show `style` (`"classic"` or a pack id; `null`: leave it)
/// when it is not what the clock shows already, then POST the changed settings. The pack goes
/// first: it is kept on the clock's filesystem, so a reboot for a WiFi change keeps it.
pub async fn save(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    save_on(&Bridge::of(ctx), &args).await
}

/// `save` on a bridge (the command without the Tauri context).
pub async fn save_on(b: &Bridge, args_in: &[Value]) -> Result<Value, String> {
    let args = args_in.to_vec();
    if b.job_running() {
        return Ok(fail("busy", "Đang cập nhật firmware, chờ xong đã."));
    }
    let (changes, style) =
        match diff(&b, arg(&args, 0)).and_then(|c| Ok((c, b.style_arg(arg(&args, 1))?))) {
            Ok(ok) => ok,
            Err(e) => return Ok(bad_value(&e)),
        };
    let heap = b.data().heap.clone();
    let shown = firmware::clock_style(&heap);
    let style = style.filter(|s| {
        *s != if shown.is_empty() {
            CLASSIC
        } else {
            shown.as_str()
        }
    });
    // Back to the classic screen only deletes the clock's pack: it needs neither a bundle nor a
    // feed.
    if style.as_deref().is_some_and(|s| s != CLASSIC)
        && firmware::style_state(&heap, b.src().as_deref().map(|r| &r.bundle)) != "ready"
    {
        return Ok(fail(
            "bad",
            "Firmware trên đồng hồ chưa vẽ được kiểu này: cập nhật firmware trước.",
        ));
    }
    if let Some(s) = &style {
        if let Err(e) = b.apply_style(s).await {
            return Ok(e.into_fail());
        }
    }
    let reply = match b.api().save(&changes).await {
        Ok(r) => r,
        Err(e) => return Ok(err(&e)),
    };
    let reboot = reply.get("reboot").is_some_and(truthy);
    if !reboot {
        if let Ok(cfg) = b.api().config().await {
            b.data().cfg = cfg;
        }
    }
    if let Some(screen) = changes.pointer("/usage/screen") {
        b.mirror_screen(&json!({"usage": {"screen": screen}}));
    }
    let cfg = b.data().cfg.clone();
    Ok(json!({
        "ok": true, "sent": changes, "reboot": reboot, "config": cfg,
        "styleSent": style.is_some(), "fw": b.fw_info(),
    }))
}

/// `SettingsApi.set_clock(raw)`: use the clock at this address (typed by hand), or `""` for
/// "Tìm lại": the saved address again (the tray unpins a typed one so mDNS may replace it), or
/// where mDNS finds the clock meanwhile. Only that one address is ever asked: there is no
/// network scan.
pub async fn set_clock(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    set_clock_on(&Bridge::of(ctx), &args).await
}

/// `set_clock` on a bridge (the command without the Tauri context).
pub async fn set_clock_on(b: &Bridge, args: &[Value]) -> Result<Value, String> {
    let Some(raw) = arg(args, 0).as_str() else {
        return Ok(fail("bad", "Địa chỉ không hợp lệ."));
    };
    let raw = raw.trim();
    let typed = if raw.is_empty() {
        None
    } else {
        manual_clock(raw)
    };
    if !raw.is_empty() && typed.is_none() {
        return Ok(fail(
            "bad",
            &format!("\u{201c}{raw}\u{201d} không phải địa chỉ hợp lệ."),
        ));
    }
    b.notify(ShellCmd::SetClock {
        host: raw.to_owned(),
    });
    if let Some(c) = typed {
        let (base, key) = (c.base(), raw.to_owned());
        b.blocking(move |b| b.connect(&base, &key)).await;
        return Ok(b.state().await);
    }
    let deadline = Instant::now() + b.data().search_wait;
    let last = loop {
        // Re-read: the tray may move it (mDNS).
        let stored = stored_clock(&b.store().load());
        if let Some(c) = &stored {
            if Bridge::answers(&c.base()).await {
                let name = if c.name.is_empty() { &c.host } else { &c.name };
                let (base, key) = (c.base(), name.clone());
                b.blocking(move |b| b.connect(&base, &key)).await;
                return Ok(b.state().await);
            }
        }
        if Instant::now() >= deadline {
            break stored;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    let whereabouts = match last {
        Some(c) => format!("Đồng hồ ở {} không trả lời.", c.host),
        None => "Chưa có địa chỉ đồng hồ.".to_owned(),
    };
    Ok(fail("offline", &format!("{whereabouts} {TYPE_IP}")))
}

/// `SettingsApi.scan`: the clock's WiFi scan, one row per network, strongest first.
pub async fn scan(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    scan_on(&Bridge::of(ctx), &args).await
}

/// `scan` on a bridge (the command without the Tauri context).
pub async fn scan_on(b: &Bridge, _args: &[Value]) -> Result<Value, String> {
    let nets = match b.clone().api().scan().await {
        Ok(n) => n,
        Err(e) => return Ok(err(&e)),
    };
    Ok(json!({"ok": true, "networks": strongest_per_ssid(&nets)}))
}

/// The rows of a scan: the strongest signal per SSID, sorted by signal (stable).
pub fn strongest_per_ssid(nets: &[Value]) -> Vec<Value> {
    let rssi = |n: &Value, default: f64| n.get("rssi").and_then(Value::as_f64).unwrap_or(default);
    let mut best: Vec<(String, Value)> = Vec::new();
    for n in nets {
        let Some(ssid) = n
            .get("ssid")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let row = json!({
            "ssid": ssid, "rssi": n.get("rssi").cloned().unwrap_or(json!(-100)),
            "enc": n.get("enc").is_some_and(truthy),
        });
        match best.iter_mut().find(|(s, _)| s == ssid) {
            Some((_, kept)) => {
                if rssi(n, -999.0) > rssi(kept, -999.0) {
                    *kept = row;
                }
            }
            None => best.push((ssid.to_owned(), row)),
        }
    }
    let mut rows: Vec<Value> = best.into_iter().map(|(_, row)| row).collect();
    rows.sort_by(|a, c| rssi(c, -100.0).total_cmp(&rssi(a, -100.0)));
    rows
}

/// `SettingsApi.reboot`.
pub async fn reboot(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    reboot_on(&Bridge::of(ctx), &args).await
}

/// `reboot` on a bridge (the command without the Tauri context).
pub async fn reboot_on(b: &Bridge, _args: &[Value]) -> Result<Value, String> {
    Ok(match b.clone().api().reboot().await {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(&e),
    })
}

/// `SettingsApi.factory_reset(confirm)`: the clock's factory reset, after typing `XOA`.
pub async fn factory_reset(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    factory_reset_on(&Bridge::of(ctx), &args).await
}

/// `factory_reset` on a bridge (the command without the Tauri context).
pub async fn factory_reset_on(b: &Bridge, args_in: &[Value]) -> Result<Value, String> {
    let args = args_in.to_vec();
    if arg(&args, 0).as_str() != Some("XOA") {
        return Ok(fail("bad", "Gõ XOA để xác nhận."));
    }
    Ok(match b.clone().api().factory_reset().await {
        Ok(()) => json!({"ok": true}),
        Err(e) => err(&e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scan_keeps_the_strongest_row_per_ssid_sorted_by_signal() {
        let nets = vec![
            json!({"ssid": "Home", "rssi": -70, "enc": 3}),
            json!({"ssid": "Cafe", "rssi": -50}),
            json!({"ssid": "Home", "rssi": -60, "enc": 3}),
            json!({"ssid": "", "rssi": -10}),
            json!({"rssi": -10}),
            json!({"ssid": "Far"}),
        ];
        assert_eq!(
            strongest_per_ssid(&nets),
            vec![
                json!({"ssid": "Cafe", "rssi": -50, "enc": false}),
                json!({"ssid": "Home", "rssi": -60, "enc": true}),
                json!({"ssid": "Far", "rssi": -100, "enc": false}),
            ]
        );
    }
}
