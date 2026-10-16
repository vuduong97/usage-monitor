//! Usage from Anthropic: the read-only `GET /api/oauth/usage` first (costs no quota), the Haiku
//! rate-limit-header probe as fallback for accounts where it reports nothing usable
//! (Enterprise spend limits, API organisations). Pure free-ride: a 401/403 means the token's
//! owner (Claude Code) must renew it; we never refresh. Token goes only to `api.anthropic.com`
//! and never into a log line.
//!
//! The numbers must match Python exactly, rounding included (`int(round(x))` is banker's
//! rounding in Python; `.5` boundaries are in the golden fixtures).

use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use clawd_contracts::{Clock, SystemClock, TokenExpired, UsageNotApplicable, UsagePayload};
use serde_json::{json, Value};
use thiserror::Error;

use crate::payload::{add_chime_field, add_clock_fields};
use crate::util::{parse_iso_timestamp, py_float, round_i64, truthy};
use crate::UsageEnv;

/// `usage.API_URL`: the Haiku probe endpoint.
pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// `usage.USAGE_URL`: the read-only usage endpoint.
pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// `usage.API_HEADERS_TEMPLATE` (without `Authorization`).
const API_HEADERS: [(&str, &str); 4] = [
    ("anthropic-version", "2023-06-01"),
    ("anthropic-beta", "oauth-2025-04-20"),
    ("content-type", "application/json"),
    ("user-agent", "claude-code/2.1.5"),
];
/// `usage.API_BODY`: the one-token Haiku probe.
const API_BODY: &str = r#"{"model":"claude-haiku-4-5-20251001","max_tokens":1,"messages":[{"role":"user","content":"hi"}]}"#;
/// `usage.USAGE_HEADERS_TEMPLATE` (without `Authorization`); also sent through the proxy.
pub(crate) const USAGE_HEADERS: [(&str, &str); 3] = [
    ("anthropic-beta", "oauth-2025-04-20"),
    ("Content-Type", "application/json"),
    ("User-Agent", "claude-code/2.1.5"),
];
/// `httpx.AsyncClient(timeout=20.0)`.
const POLL_TIMEOUT: Duration = Duration::from_secs(20);

/// What the usage endpoint can fail with (`TokenExpired` / `UsageNotApplicable` in Python).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PollError {
    #[error(transparent)]
    TokenExpired(#[from] TokenExpired),
    #[error(transparent)]
    NotApplicable(#[from] UsageNotApplicable),
}

fn first_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// `usage.poll_api(token)`: the Haiku probe: payload from the `anthropic-ratelimit-unified-*`
/// response headers (`acct: "ent"` plus billing-period fields). `Ok(None)` on a network error or
/// non-auth HTTP error (try again next poll); 401/403 -> [`TokenExpired`].
pub async fn poll_api(env: &UsageEnv, token: &str) -> Result<Option<UsagePayload>, TokenExpired> {
    let mut request = env.client.post(&env.api_url).timeout(POLL_TIMEOUT);
    for (name, value) in API_HEADERS {
        request = request.header(name, value);
    }
    let resp = match request.bearer_auth(token).body(API_BODY).send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!("API call failed: {}", e.without_url());
            return Ok(None);
        }
    };
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        tracing::warn!("API HTTP {status} (token expired/invalid)");
        return Err(TokenExpired);
    }
    if status >= 400 {
        let text = resp.text().await.unwrap_or_default();
        tracing::warn!("API HTTP {status}: {}", first_chars(&text, 200));
        return Ok(None);
    }
    let headers = resp.headers().clone();
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let hdr = |name: &str, default: &str| get(name).unwrap_or(default).to_owned();

    let now = env.clock.now();
    let reset_minutes = |reset_ts: &str| -> i64 {
        let Ok(r) = reset_ts.trim().parse::<f64>() else {
            return 0;
        };
        let mins = (r - now) / 60.0;
        if mins > 0.0 {
            round_i64(mins).unwrap_or(0)
        } else {
            0
        }
    };
    let pct = |util: &str| -> i64 {
        util.trim()
            .parse::<f64>()
            .ok()
            .and_then(|f| round_i64(f * 100.0))
            .unwrap_or(0)
    };

    let mut payload = UsagePayload::new();
    // Pro/Max accounts expose 5h/7d windows; Enterprise/overage use a single spending-limit
    // model reported via overage-utilization.
    if get("anthropic-ratelimit-unified-5h-utilization").is_some_and(|v| !v.is_empty()) {
        payload.insert(
            "s".into(),
            json!(pct(&hdr("anthropic-ratelimit-unified-5h-utilization", "0"))),
        );
        payload.insert(
            "sr".into(),
            json!(reset_minutes(&hdr(
                "anthropic-ratelimit-unified-5h-reset",
                "0"
            ))),
        );
        payload.insert(
            "w".into(),
            json!(pct(&hdr("anthropic-ratelimit-unified-7d-utilization", "0"))),
        );
        payload.insert(
            "wr".into(),
            json!(reset_minutes(&hdr(
                "anthropic-ratelimit-unified-7d-reset",
                "0"
            ))),
        );
        payload.insert(
            "st".into(),
            json!(hdr("anthropic-ratelimit-unified-5h-status", "unknown")),
        );
        payload.insert("acct".into(), json!("pro"));
        payload.insert("ok".into(), json!(true));
    } else {
        let reset_ts = hdr("anthropic-ratelimit-unified-overage-reset", "0");
        payload.insert(
            "s".into(),
            json!(pct(&hdr(
                "anthropic-ratelimit-unified-overage-utilization",
                "0"
            ))),
        );
        payload.insert("sr".into(), json!(reset_minutes(&reset_ts)));
        payload.insert("w".into(), json!(0));
        payload.insert("wr".into(), json!(0));
        payload.insert(
            "st".into(),
            json!(hdr("anthropic-ratelimit-unified-status", "unknown")),
        );
        payload.insert("acct".into(), json!("ent"));
        for (key, value) in billing_period_info(now, &reset_ts) {
            payload.insert(key.into(), value);
        }
        payload.insert("ok".into(), json!(true));
    }
    add_chime_field(env, &mut payload);
    add_clock_fields(env, &mut payload);
    Ok(Some(payload))
}

/// `usage._billing_period_info(now, reset_ts)`: fraction of the (assumed calendar-monthly)
/// billing period elapsed (`tp`, 0-100), its length in days (`pd`) and the reset date (`rd`).
/// Local time, like Python's `fromtimestamp`.
pub(crate) fn billing_period_info(now: f64, reset_ts: &str) -> Vec<(&'static str, Value)> {
    let fallback = || vec![("tp", json!(0)), ("pd", json!(30))];
    let Ok(period_end) = reset_ts.trim().parse::<f64>() else {
        return fallback();
    };
    // An absent header reads "0": 1970 minus a month would be 1969 (OSError on Windows).
    if !period_end.is_finite() || period_end <= 0.0 {
        return fallback();
    }
    let secs = period_end.floor();
    let nanos = ((period_end - secs) * 1e9) as u32;
    let Some(dt_end) = Local.timestamp_opt(secs as i64, nanos).single() else {
        return fallback();
    };
    let (prev_year, prev_month) = if dt_end.month() > 1 {
        (dt_end.year(), dt_end.month() - 1)
    } else {
        (dt_end.year() - 1, 12)
    };
    let prev_day = dt_end.day().min(days_in_month(prev_year, prev_month));
    let Some(start_naive) = NaiveDate::from_ymd_opt(prev_year, prev_month, prev_day)
        .and_then(|d| d.and_hms_nano_opt(dt_end.hour(), dt_end.minute(), dt_end.second(), nanos))
    else {
        return fallback();
    };
    let Some(dt_start) = Local
        .from_local_datetime(&start_naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&start_naive).latest())
    else {
        return fallback();
    };
    let period_start =
        dt_start.timestamp() as f64 + f64::from(dt_start.timestamp_subsec_nanos()) / 1e9;
    let period_len = period_end - period_start;
    if period_len <= 0.0 {
        return fallback();
    }
    let pct_val = (now - period_start) / period_len * 100.0;
    let total_days = round_i64(period_len / 86400.0).unwrap_or(30);
    let rd = format!("{} {}", dt_end.format("%b"), dt_end.day());
    vec![
        ("tp", json!(round_i64(pct_val).unwrap_or(0).clamp(0, 100))),
        ("pd", json!(total_days)),
        ("rd", json!(rd)),
    ]
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|first_next| first_next.pred_opt())
        .map_or(28, |last| last.day())
}

/// `usage._minutes_until(iso_ts, now)`: whole minutes until an ISO-8601 timestamp (0 if past or
/// invalid).
fn minutes_until(iso_ts: Option<&Value>, now: f64) -> i64 {
    let Some(Value::String(raw)) = iso_ts else {
        return 0;
    };
    let Some(ts) = parse_iso_timestamp(raw).filter(|_| !raw.is_empty()) else {
        return 0;
    };
    let mins = (ts - now) / 60.0;
    if mins > 0.0 {
        round_i64(mins).unwrap_or(0)
    } else {
        0
    }
}

/// `usage._window_pct(window)`: utilization clamped to 0-100 (0 when unreadable).
fn window_pct(window: Option<&Value>) -> i64 {
    let util = window
        .and_then(Value::as_object)
        .and_then(|w| w.get("utilization"));
    py_float(util)
        .and_then(round_i64)
        .map_or(0, |p| p.clamp(0, 100))
}

/// `usage.usage_from_oauth_usage(body, now)`: map an `/api/oauth/usage` body to the device
/// payload (`s sr w wr st acct ok`). `now` = epoch seconds, `None` = the real time.
/// [`UsageNotApplicable`] when the body carries no 5h window.
pub fn usage_from_oauth_usage(
    body: &Value,
    now: Option<f64>,
) -> Result<UsagePayload, UsageNotApplicable> {
    let Some(body) = body.as_object() else {
        return Err(UsageNotApplicable("not a JSON object".into()));
    };
    let five = match body.get("five_hour") {
        Some(Value::Object(five)) if five.get("utilization").is_some_and(|u| !u.is_null()) => five,
        _ => return Err(UsageNotApplicable("no five_hour window".into())),
    };
    let week = body.get("seven_day").filter(|w| w.is_object());
    let now = now.unwrap_or_else(|| SystemClock.now());
    let locked = |w: Option<&Value>| w.and_then(|w| w.get("locked_reason")).is_some_and(truthy);
    let mut status = "allowed";
    if five.get("locked_reason").is_some_and(truthy) || locked(week) {
        status = "rejected";
    } else if let Some(Value::Array(limits)) = body.get("limits") {
        for lim in limits {
            let kind = lim.get("kind").and_then(Value::as_str);
            let severity_not_normal = match lim.get("severity") {
                None | Some(Value::Null) => false,
                Some(Value::String(s)) => s != "normal",
                Some(_) => true,
            };
            if lim.is_object() && matches!(kind, Some("session" | "weekly")) && severity_not_normal
            {
                status = "allowed_warning";
                break;
            }
        }
    }
    let mut payload = UsagePayload::new();
    payload.insert("s".into(), json!(window_pct(body.get("five_hour"))));
    payload.insert(
        "sr".into(),
        json!(minutes_until(five.get("resets_at"), now)),
    );
    payload.insert("w".into(), json!(window_pct(week)));
    payload.insert(
        "wr".into(),
        json!(minutes_until(week.and_then(|w| w.get("resets_at")), now)),
    );
    payload.insert("st".into(), json!(status));
    payload.insert("acct".into(), json!("pro"));
    payload.insert("ok".into(), json!(true));
    Ok(payload)
}

/// `usage._usage_result(status, text)`: shared handling of a usage-endpoint answer, direct or
/// via the proxy. 401/403 -> [`TokenExpired`]; 404 or a "not applicable" 400 ->
/// [`UsageNotApplicable`]; other errors -> `Ok(None)` (try again next poll); 200 -> payload.
pub(crate) fn usage_result(
    env: &UsageEnv,
    status: i64,
    text: &str,
) -> Result<Option<UsagePayload>, PollError> {
    if status == 401 || status == 403 {
        return Err(TokenExpired.into());
    }
    if status == 404 || (status == 400 && text.to_lowercase().contains("not applicable")) {
        return Err(UsageNotApplicable(format!("HTTP {status}")).into());
    }
    if status >= 400 {
        tracing::warn!("Usage endpoint HTTP {status}: {}", first_chars(text, 200));
        return Ok(None);
    }
    let Ok(body) = serde_json::from_str::<Value>(text) else {
        tracing::warn!("Usage endpoint returned non-JSON");
        return Ok(None);
    };
    let mut payload = usage_from_oauth_usage(&body, Some(env.clock.now()))?;
    add_chime_field(env, &mut payload);
    add_clock_fields(env, &mut payload);
    Ok(Some(payload))
}

/// `usage.poll_usage_endpoint(token)`: usage for a Claude Code token via the read-only
/// endpoint, with `c`/`t`/`tf` fields added per the config. `Ok(None)` for a transient failure;
/// 401/403 -> [`TokenExpired`]; 404 or a "not applicable" 400 -> [`UsageNotApplicable`].
pub async fn poll_usage_endpoint(
    env: &UsageEnv,
    token: &str,
) -> Result<Option<UsagePayload>, PollError> {
    let mut request = env.client.get(&env.usage_url).timeout(POLL_TIMEOUT);
    for (name, value) in USAGE_HEADERS {
        request = request.header(name, value);
    }
    let resp = match request.bearer_auth(token).send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!("Usage endpoint call failed: {}", e.without_url());
            return Ok(None);
        }
    };
    let status = i64::from(resp.status().as_u16());
    let text = resp.text().await.unwrap_or_default();
    usage_result(env, status, &text)
}

/// `usage.poll_token(token)`: the endpoint first, the probe ([`poll_api`]) only when the
/// endpoint says [`UsageNotApplicable`].
pub async fn poll_token(env: &UsageEnv, token: &str) -> Result<Option<UsagePayload>, TokenExpired> {
    match poll_usage_endpoint(env, token).await {
        Ok(payload) => Ok(payload),
        Err(PollError::TokenExpired(e)) => Err(e),
        Err(PollError::NotApplicable(e)) => {
            tracing::info!("Usage endpoint not applicable ({e}); using the rate-limit headers");
            poll_api(env, token).await
        }
    }
}
