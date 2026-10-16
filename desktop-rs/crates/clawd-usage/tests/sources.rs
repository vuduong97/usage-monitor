//! Ported from `tests/test_usage_sources.py`, `tests/test_macos_multidir.py` (selector,
//! `poll_active_payload`) and the source-choice half of `tests/test_codex_usage.py`: the
//! read-only usage endpoint, the Haiku fallback, CLIProxyAPI, plan selection and backoff.
//!
//! Token/config readers come from `clawd-config` (phase 06): until it is merged these tests
//! only pass with the local scratch implementation of those readers.

mod common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clawd_testkit::usage::{oauth_usage_max, FakeCliProxy};
use clawd_testkit::FakeResponse;
use clawd_usage::cliproxy::CLIPROXY_ACCOUNTS_TTL;
use clawd_usage::publisher::POLL_INTERVAL;
use clawd_usage::{
    poll_active, poll_active_payload, poll_claude, poll_token, CliProxySource, PlanKey,
    PlanSelector, PollState,
};
use common::{assert_same_json, dirs_config, key_absent, key_wrong, Fx, IDX, KEY};
use serde_json::{json, Value};

const TOKEN: &str = "sk-ant-test-token";

fn body_with_utilization(util: f64) -> Value {
    let mut body = oauth_usage_max();
    body["five_hour"]["utilization"] = json!(util);
    body
}

/// The usage endpoint answers by bearer token: `{token: five-hour utilization}`.
fn usage_by_token(fx: &Fx, by_token: &[(&str, f64)]) {
    let table: HashMap<String, f64> = by_token
        .iter()
        .map(|(t, u)| ((*t).to_owned(), *u))
        .collect();
    fx.anthropic.server.route_fn(
        "GET",
        "/api/oauth/usage",
        Arc::new(move |req| {
            let token = req
                .header("authorization")
                .and_then(|h| h.strip_prefix("Bearer "))
                .unwrap_or("");
            match table.get(token) {
                Some(util) => FakeResponse::json(200, &body_with_utilization(*util)),
                None => FakeResponse::text(401, "{}"),
            }
        }),
    );
}

fn usage_requests(fx: &Fx) -> usize {
    fx.anthropic.server.requests_to("/api/oauth/usage").len()
}

fn probe_requests(fx: &Fx) -> usize {
    fx.anthropic.server.requests_to("/v1/messages").len()
}

// ---- poll_token: the endpoint first, the Haiku probe only as the fallback ---------------------

#[tokio::test]
async fn poll_token_uses_endpoint_without_haiku() {
    let fx = Fx::new();
    fx.config("clock = off\n");
    fx.anthropic.usage_ok(&oauth_usage_max());
    let payload = poll_token(&fx.env, TOKEN).await.unwrap().unwrap();
    assert_eq!(payload.s(), Some(7));
    assert_eq!(probe_requests(&fx), 0);
    let req = &fx.anthropic.server.requests_to("/api/oauth/usage")[0];
    assert_eq!(
        req.header("authorization"),
        Some("Bearer sk-ant-test-token")
    );
    assert_eq!(req.header("anthropic-beta"), Some("oauth-2025-04-20"));
    assert_eq!(req.header("user-agent"), Some("claude-code/2.1.5"));
}

#[tokio::test]
async fn poll_token_falls_back_when_not_applicable() {
    let fx = Fx::new();
    fx.config("clock = off\n");
    fx.anthropic.usage_status(404, "not found").probe_headers(&[
        ("anthropic-ratelimit-unified-overage-utilization", "0.03"),
        ("anthropic-ratelimit-unified-status", "allowed"),
    ]);
    let payload = poll_token(&fx.env, TOKEN).await.unwrap().unwrap();
    assert_eq!((payload.s(), payload.acct()), (Some(3), Some("ent")));
    assert_eq!(probe_requests(&fx), 1);
}

#[tokio::test]
async fn poll_token_does_not_fall_back_on_expired_token() {
    let fx = Fx::new();
    fx.anthropic.usage_status(401, "{}");
    assert!(poll_token(&fx.env, TOKEN).await.is_err());
    assert_eq!(probe_requests(&fx), 0);
}

#[tokio::test]
async fn probe_401_is_token_expired_and_500_is_transient() {
    let fx = Fx::new();
    fx.config("clock = off\n");
    fx.anthropic.usage_status(404, "gone").probe_status(401);
    assert!(poll_token(&fx.env, TOKEN).await.is_err());
    // the first matching route wins: a fresh server for the transient case
    let fx = Fx::new();
    fx.config("clock = off\n");
    fx.anthropic.usage_status(404, "gone").probe_status(500);
    assert_eq!(poll_token(&fx.env, TOKEN).await.unwrap(), None);
}

#[tokio::test]
async fn unreachable_endpoint_is_transient() {
    let mut fx = Fx::new();
    fx.env.usage_url = "http://127.0.0.1:1/api/oauth/usage".to_owned();
    assert_eq!(poll_token(&fx.env, TOKEN).await.unwrap(), None);
}

// ---- CliProxySource ---------------------------------------------------------------------------

fn auth_files() -> Value {
    json!({"files": [
        {"provider": "claude", "type": "claude", "auth_index": IDX, "status": "active",
         "disabled": false, "unavailable": false},
        {"provider": "claude", "auth_index": "dddddddd00000000", "disabled": true},
        {"provider": "claude", "auth_index": "uuuuuuuu00000000", "unavailable": true},
        {"provider": "gemini", "auth_index": "gggggggg00000000"},
    ]})
}

fn proxy_ok() -> FakeCliProxy {
    FakeCliProxy::start(KEY, auth_files(), |_| (200, oauth_usage_max()))
}

fn proxy_fx(proxy: &FakeCliProxy) -> Fx {
    let fx = Fx::new();
    fx.config(&format!("clock = off\ncliproxy_url = {}\n", proxy.url()));
    fx
}

fn plan() -> PlanKey {
    PlanKey::Proxy(format!("cliproxy:{}", &IDX[..8]))
}

#[tokio::test]
async fn discovers_active_claude_accounts_only() {
    let proxy = proxy_ok();
    let fx = proxy_fx(&proxy);
    let (payloads, live) = CliProxySource::new().poll(&fx.env).await;
    assert!(live);
    assert_eq!(payloads.keys().collect::<Vec<_>>(), vec![&plan()]);
    assert_eq!(payloads[&plan()].s(), Some(7));
    let calls = proxy.api_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["authIndex"], IDX);
}

#[tokio::test]
async fn api_call_lets_the_proxy_substitute_the_token() {
    let proxy = proxy_ok();
    let fx = proxy_fx(&proxy);
    CliProxySource::new().poll(&fx.env).await;
    let call = &proxy.api_calls()[0];
    assert_eq!(call["method"], "GET");
    assert_eq!(call["url"], "https://api.anthropic.com/api/oauth/usage");
    assert_eq!(call["header"]["Authorization"], "Bearer $TOKEN$");
    assert_eq!(call["header"]["anthropic-beta"], "oauth-2025-04-20");
    assert!(
        !call.to_string().contains(KEY),
        "the management key never goes upstream"
    );
}

#[tokio::test]
async fn account_list_is_cached() {
    let proxy = proxy_ok();
    let fx = proxy_fx(&proxy);
    let mut source = CliProxySource::new();
    source.poll(&fx.env).await;
    source.poll(&fx.env).await;
    assert_eq!((proxy.list_calls(), proxy.api_calls().len()), (1, 2));
}

#[tokio::test]
async fn account_list_refreshes_after_ttl() {
    let proxy = proxy_ok();
    let fx = proxy_fx(&proxy);
    let mut source = CliProxySource::new();
    source.poll(&fx.env).await;
    fx.clock.advance(CLIPROXY_ACCOUNTS_TTL + 1.0);
    source.poll(&fx.env).await;
    assert_eq!(proxy.list_calls(), 2);
}

#[tokio::test]
async fn wrong_key_yields_nothing_and_is_dead() {
    let proxy = proxy_ok();
    let mut fx = proxy_fx(&proxy);
    fx.env.cliproxy_key_reader = key_wrong;
    let (payloads, live) = CliProxySource::new().poll(&fx.env).await;
    assert!(payloads.is_empty() && !live);
    assert!(proxy.api_calls().is_empty());
}

#[tokio::test]
async fn no_key_in_keychain_skips_the_proxy() {
    let proxy = proxy_ok();
    let mut fx = proxy_fx(&proxy);
    fx.env.cliproxy_key_reader = key_absent;
    let (payloads, live) = CliProxySource::new().poll(&fx.env).await;
    assert!(payloads.is_empty() && !live);
    assert_eq!(proxy.list_calls(), 0);
}

#[tokio::test]
async fn proxy_not_running_is_dead() {
    let fx = Fx::new();
    fx.config("cliproxy_url = http://127.0.0.1:1\n");
    let (payloads, live) = CliProxySource::new().poll(&fx.env).await;
    assert!(payloads.is_empty() && !live);
}

#[tokio::test]
async fn expired_account_is_dead_but_others_live() {
    let other = "bbbbbbbb11111111";
    let files = json!({"files": [{"provider": "claude", "auth_index": IDX},
                                 {"provider": "claude", "auth_index": other}]});
    let proxy = FakeCliProxy::start(KEY, files, |idx| {
        if idx == IDX {
            (401, json!({"error": "expired"}))
        } else {
            (200, oauth_usage_max())
        }
    });
    let fx = proxy_fx(&proxy);
    let (payloads, live) = CliProxySource::new().poll(&fx.env).await;
    assert!(live);
    assert_eq!(
        payloads.keys().collect::<Vec<_>>(),
        vec![&PlanKey::Proxy("cliproxy:bbbbbbbb".into())]
    );
}

#[tokio::test]
async fn all_accounts_expired_is_dead() {
    let proxy = FakeCliProxy::start(KEY, auth_files(), |_| (401, json!({"error": "expired"})));
    let fx = proxy_fx(&proxy);
    let (payloads, live) = CliProxySource::new().poll(&fx.env).await;
    assert!(payloads.is_empty() && !live);
}

#[tokio::test]
async fn transient_upstream_error_stays_live() {
    let proxy = FakeCliProxy::start(KEY, auth_files(), |_| (529, json!({"error": "overloaded"})));
    let fx = proxy_fx(&proxy);
    let (payloads, live) = CliProxySource::new().poll(&fx.env).await;
    assert!(payloads.is_empty() && live);
}

#[tokio::test]
async fn transient_account_error_backs_off() {
    let proxy = FakeCliProxy::start(KEY, auth_files(), |_| (529, json!({"error": "overloaded"})));
    let fx = proxy_fx(&proxy);
    let mut source = CliProxySource::new();
    source.poll(&fx.env).await;
    let (payloads, live) = source.poll(&fx.env).await; // next 5 s tick: not asked again
    assert!(payloads.is_empty() && live);
    assert_eq!(proxy.api_calls().len(), 1);
    fx.clock.advance(POLL_INTERVAL + 1.0); // interval elapsed
    source.poll(&fx.env).await;
    assert_eq!(proxy.api_calls().len(), 2);
}

#[tokio::test]
async fn not_applicable_account_backs_off_for_the_accounts_ttl() {
    let proxy = FakeCliProxy::start(KEY, auth_files(), |_| (404, json!({"error": "gone"})));
    let fx = proxy_fx(&proxy);
    let mut source = CliProxySource::new();
    let (payloads, live) = source.poll(&fx.env).await;
    assert!(payloads.is_empty() && live);
    fx.clock.advance(POLL_INTERVAL + 1.0);
    source.poll(&fx.env).await;
    assert_eq!(
        proxy.api_calls().len(),
        1,
        "still inside CLIPROXY_ACCOUNTS_TTL"
    );
}

#[tokio::test]
async fn proxy_account_order_is_the_listing_order() {
    let files = json!({"files": [{"provider": "claude", "auth_index": "zzzzzzzz00000000"},
                                 {"provider": "claude", "auth_index": "aaaaaaaa00000000"}]});
    let proxy = FakeCliProxy::start(KEY, files, |_| (200, oauth_usage_max()));
    let fx = proxy_fx(&proxy);
    let (ordered, _) = CliProxySource::new().poll_ordered(&fx.env).await;
    let plans: Vec<_> = ordered.iter().map(|(k, _)| k.to_string()).collect();
    assert_eq!(plans, ["cliproxy:zzzzzzzz", "cliproxy:aaaaaaaa"]);
}

// ---- PlanSelector -------------------------------------------------------------------------------

fn a() -> PlanKey {
    PlanKey::Dir(PathBuf::from("/a"))
}

fn b() -> PlanKey {
    PlanKey::Dir(PathBuf::from("/b"))
}

fn sessions(pairs: &[(&PlanKey, i64)]) -> HashMap<PlanKey, i64> {
    pairs.iter().map(|(k, v)| ((*k).clone(), *v)).collect()
}

#[test]
fn selector_startup_picks_highest_util() {
    let mut sel = PlanSelector::new();
    assert_eq!(sel.choose(&sessions(&[(&a(), 10), (&b(), 30)])), b());
}

#[test]
fn selector_switches_on_rise() {
    let mut sel = PlanSelector::new();
    sel.choose(&sessions(&[(&a(), 10), (&b(), 30)]));
    assert_eq!(sel.choose(&sessions(&[(&a(), 20), (&b(), 30)])), a());
}

#[test]
fn selector_sticky_when_no_movement() {
    let mut sel = PlanSelector::new();
    sel.choose(&sessions(&[(&a(), 10), (&b(), 30)]));
    sel.choose(&sessions(&[(&a(), 20), (&b(), 30)]));
    assert_eq!(sel.choose(&sessions(&[(&a(), 20), (&b(), 30)])), a());
}

#[test]
fn selector_reset_to_zero_is_not_activity() {
    let mut sel = PlanSelector::new();
    sel.choose(&sessions(&[(&a(), 10), (&b(), 30)]));
    sel.choose(&sessions(&[(&a(), 20), (&b(), 30)]));
    sel.choose(&sessions(&[(&a(), 20), (&b(), 45)]));
    assert_eq!(sel.choose(&sessions(&[(&a(), 20), (&b(), 0)])), b());
}

#[test]
fn selector_larger_rise_wins_same_cycle() {
    let mut sel = PlanSelector::new();
    sel.choose(&sessions(&[(&a(), 10), (&b(), 10)]));
    assert_eq!(sel.choose(&sessions(&[(&a(), 12), (&b(), 40)])), b());
}

#[test]
fn selector_tie_goes_to_the_first_plan_in_order() {
    let mut sel = PlanSelector::new();
    assert_eq!(sel.choose_ordered(&[(b(), 5), (a(), 5)]), b());
    let mut sel = PlanSelector::new();
    assert_eq!(sel.choose_ordered(&[(a(), 5), (b(), 5)]), a());
}

// ---- poll_claude / poll_active across sources -----------------------------------------------------

#[tokio::test]
async fn poll_active_payload_picks_active_and_skips_tokenless() {
    let fx = Fx::new();
    let (dir_a, dir_b) = (fx.claude_dir("a", "tA"), fx.empty_dir("b"));
    fx.config(&dirs_config(&[&dir_a, &dir_b], ""));
    usage_by_token(&fx, &[("tA", 25.0)]);
    let payload = poll_active_payload(&fx.env, &mut PollState::new())
        .await
        .unwrap();
    assert_eq!(payload.s(), Some(25)); // only A had a token
}

#[tokio::test]
async fn poll_active_payload_returns_none_when_all_fail() {
    let fx = Fx::new();
    let (dir_a, dir_b) = (fx.empty_dir("a"), fx.empty_dir("b"));
    fx.config(&dirs_config(&[&dir_a, &dir_b], ""));
    assert!(poll_active_payload(&fx.env, &mut PollState::new())
        .await
        .is_none());
}

#[tokio::test]
async fn poll_active_payload_selects_higher_util_plan() {
    let fx = Fx::new();
    let (dir_a, dir_b) = (fx.claude_dir("a", "tA"), fx.claude_dir("b", "tB"));
    fx.config(&dirs_config(&[&dir_a, &dir_b], ""));
    usage_by_token(&fx, &[("tA", 12.0), ("tB", 40.0)]);
    let payload = poll_active_payload(&fx.env, &mut PollState::new())
        .await
        .unwrap();
    assert_eq!(payload.s(), Some(40)); // startup -> highest util plan (B)
}

#[tokio::test]
async fn startup_tie_goes_to_the_first_configured_dir() {
    let fx = Fx::new();
    let (dir_a, dir_b) = (fx.claude_dir("a", "tA"), fx.claude_dir("b", "tB"));
    fx.config(&dirs_config(&[&dir_b, &dir_a], ""));
    usage_by_token(&fx, &[("tA", 10.0), ("tB", 11.0)]);
    let mut state = PollState::new();
    assert_eq!(
        poll_active_payload(&fx.env, &mut state).await.unwrap().s(),
        Some(11)
    );
}

#[tokio::test]
async fn cliproxy_off_never_touches_the_proxy() {
    let proxy = proxy_ok();
    let fx = Fx::new();
    let dir = fx.claude_dir("a", "tA");
    fx.config(&dirs_config(
        &[&dir],
        &format!("cliproxy_url = {}\n", proxy.url()),
    ));
    usage_by_token(&fx, &[("tA", 5.0)]);
    let (payload, dead) = poll_active(&fx.env, &mut PollState::new()).await;
    assert_eq!((payload.unwrap().s(), dead), (Some(5), false));
    assert!(proxy.server.requests().is_empty());
}

#[tokio::test]
async fn proxy_account_wins_when_it_is_the_one_in_use() {
    let upstream_util = Arc::new(AtomicU64::new(7));
    let seen = Arc::clone(&upstream_util);
    let proxy = FakeCliProxy::start(KEY, auth_files(), move |_| {
        (
            200,
            body_with_utilization(seen.load(Ordering::SeqCst) as f64),
        )
    });
    let fx = Fx::new();
    let dir = fx.claude_dir("a", "tA");
    fx.config(&dirs_config(
        &[&dir],
        &format!("cliproxy = on\ncliproxy_url = {}\n", proxy.url()),
    ));
    usage_by_token(&fx, &[("tA", 30.0)]);
    let mut state = PollState::new();
    let (first, _) = poll_active(&fx.env, &mut state).await;
    assert_eq!(first.unwrap().s(), Some(30)); // startup: highest %
    upstream_util.store(9, Ordering::SeqCst);
    fx.clock.advance(POLL_INTERVAL + 1.0);
    let (second, _) = poll_active(&fx.env, &mut state).await;
    assert_eq!(second.unwrap().s(), Some(9)); // proxy account rose
}

#[tokio::test]
async fn proxy_alone_carries_the_device_when_login_is_dead() {
    let proxy = proxy_ok();
    let fx = Fx::new();
    let dir = fx.empty_dir("a");
    fx.config(&dirs_config(
        &[&dir],
        &format!("cliproxy = on\ncliproxy_url = {}\n", proxy.url()),
    ));
    let (payload, dead) = poll_active(&fx.env, &mut PollState::new()).await;
    assert_eq!((payload.unwrap().s(), dead), (Some(7), false));
}

#[tokio::test]
async fn all_sources_dead_signals_no_data() {
    let proxy = proxy_ok();
    let mut fx = Fx::new();
    fx.env.cliproxy_key_reader = key_absent;
    let dir = fx.empty_dir("a");
    fx.config(&dirs_config(
        &[&dir],
        &format!("cliproxy = on\ncliproxy_url = {}\n", proxy.url()),
    ));
    let (payload, dead) = poll_active(&fx.env, &mut PollState::new()).await;
    assert!(payload.is_none() && dead);
}

#[tokio::test]
async fn proxy_down_with_live_but_silent_login_stays_quiet() {
    let fx = Fx::new();
    let dir = fx.claude_dir("a", "tA");
    fx.config(&dirs_config(
        &[&dir],
        "cliproxy = on\ncliproxy_url = http://127.0.0.1:1\n",
    ));
    fx.anthropic.usage_status(500, "boom");
    let (payload, dead) = poll_active(&fx.env, &mut PollState::new()).await;
    assert!(payload.is_none() && !dead);
}

#[tokio::test]
async fn unanswered_dir_backs_off_for_a_poll_interval() {
    let fx = Fx::new();
    let dir = fx.claude_dir("a", "tA");
    fx.config(&dirs_config(&[&dir], ""));
    fx.anthropic.usage_status(400, "credit balance too low");
    let mut state = PollState::new();
    let (payload, dead) = poll_active(&fx.env, &mut state).await;
    assert!(payload.is_none() && !dead);
    let (payload, dead) = poll_active(&fx.env, &mut state).await; // next 5 s tick
    assert!(payload.is_none() && !dead);
    assert_eq!(usage_requests(&fx), 1);
    fx.clock.advance(POLL_INTERVAL + 1.0); // interval elapsed
    poll_active(&fx.env, &mut state).await;
    assert_eq!(usage_requests(&fx), 2);
}

#[tokio::test]
async fn expired_token_in_every_dir_is_all_dead() {
    let fx = Fx::new();
    let dir = fx.claude_dir("a", "tA");
    fx.config(&dirs_config(&[&dir], ""));
    fx.anthropic.usage_status(401, "{}");
    let (payload, dead) = poll_active(&fx.env, &mut PollState::new()).await;
    assert!(payload.is_none() && dead);
}

#[tokio::test]
async fn poll_active_follows_the_source() {
    let fx = Fx::new();
    fx.config("clock = 24\n");
    fx.usage_source("off");
    let mut state = PollState::new();
    let (p, dead) = poll_active(&fx.env, &mut state).await;
    let p = p.unwrap();
    assert!(p.ok() == Some(false) && p.src() == Some("off") && p.t().is_some() && !dead);
    assert_eq!(
        serde_json::to_string(&p)
            .unwrap()
            .split(',')
            .next()
            .unwrap(),
        "{\"ok\":false"
    );

    fx.usage_source("codex");
    let (p, dead) = poll_active(&fx.env, &mut state).await;
    assert!(p.is_none() && dead, "no Codex logs yet");
    let rl = json!({"limit_id": "codex", "primary": {"used_percent": 5.0, "window_minutes": 300,
                    "resets_at": common::NOW + 600.0}});
    let row = json!({"timestamp": "2026-10-11T09:00:00Z", "payload": {"rate_limits": rl}});
    fx.cfg.write_home(
        ".codex/sessions/2026/10/11/rollout-a.jsonl",
        format!("{row}\n"),
    );
    let (p, dead) = poll_active(&fx.env, &mut state).await;
    let p = p.unwrap();
    assert!(p.src() == Some("codex") && p.s() == Some(5) && p.t().is_some() && !dead);
}

#[tokio::test]
async fn claude_source_with_a_dir_polls_only_that_dir() {
    let fx = Fx::new();
    let (dir_a, dir_b) = (fx.claude_dir("a", "tA"), fx.claude_dir("b", "tB"));
    fx.config(&dirs_config(&[&dir_a], ""));
    usage_by_token(&fx, &[("tA", 11.0), ("tB", 44.0)]);
    fx.usage_source(&format!("claude:{}", dir_b.display()));
    let (p, dead) = poll_active(&fx.env, &mut PollState::new()).await;
    let p = p.unwrap();
    assert!(p.src().is_none() && p.s() == Some(44) && !dead);
    assert_eq!(usage_requests(&fx), 1);
    fx.usage_source("claude");
    let (p, _) = poll_active(&fx.env, &mut PollState::new()).await;
    assert_eq!(p.unwrap().s(), Some(11));
}

#[tokio::test]
async fn poll_claude_only_restricts_to_one_dir() {
    let fx = Fx::new();
    let (dir_a, dir_b) = (fx.claude_dir("a", "tA"), fx.claude_dir("b", "tB"));
    fx.config(&dirs_config(&[&dir_a, &dir_b], ""));
    usage_by_token(&fx, &[("tA", 11.0), ("tB", 44.0)]);
    let (p, _) = poll_claude(&fx.env, &mut PollState::new(), Some(dir_a)).await;
    assert_eq!(p.unwrap().s(), Some(11));
}

#[tokio::test]
async fn payload_keys_keep_python_order_with_chime_and_clock() {
    let fx = Fx::new();
    let dir = fx.claude_dir("a", "tA");
    fx.config(&dirs_config(&[&dir], "chime = on\nclock = 12\n"));
    usage_by_token(&fx, &[("tA", 7.0)]);
    let (p, _) = poll_active(&fx.env, &mut PollState::new()).await;
    let p = p.unwrap();
    let keys: Vec<&str> = p.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        ["s", "sr", "w", "wr", "st", "acct", "ok", "c", "t", "tf"]
    );
    assert_same_json(&json!(p.tf()), &json!(12), "tf");
}
