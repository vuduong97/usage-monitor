# 2026-10-11: daemon reads the usage endpoint and CLIProxyAPI accounts

Branch: `feat/daemon-http-push` (commits `0d46ccf`, `2914388`). Nothing has been pushed.

## What happened

- **No token after login.** The user ran `claude login`, but the daemon still found no token.
  - Claude Code had auto-updated from 2.1.290 to 2.1.296 just before the login.
  - The new version ("storage v5") keeps the login in `~/.config/anthropic/{active_config, credentials/<profile>.json}`.
  - The old `Claude Code-credentials` Keychain item now holds only `{"mcpOAuth":{}}`.
  - Fix in `0d46ccf`: for the default dir, read the store first, then `.credentials.json`, then the Keychain.
- **The login was an API organization.** The new token belonged to "Luminade Commerce", an API org.
  - `/api/oauth/usage` answered "Usage limits are not applicable to API organizations".
  - The Haiku probe answered "credit balance too low".
  - So there is no 5h/7d data to show from this account.
- **The user's Claude Max 20x account lives in a local CLIProxyAPI.**
  - The proxy's management `POST /v8/management/requests/api-call` substitutes `$TOKEN$` proxy-side.
  - `GET /api/oauth/usage` through it returned real numbers (session 7%, weekly 8%), and the token never left the proxy.
  - Account listing is `/v0/management/auth-files`. `/v8/.../auth-files` returns 404, so the path versions differ.

## Decisions (user, via ak:advise)

- Proxy accounts become extra plans in `PlanSelector`, and the plan whose usage rose most recently is shown.
- The management key is stored only in the Keychain (service `clawdmeter-cliproxy`).
- Every active Claude account in the proxy is discovered automatically.
- Both sources use `/api/oauth/usage`. The Haiku probe is the fallback when the endpoint is not applicable.

## Implementation (`2914388`)

- `usage_from_oauth_usage` maps the response:
  - `five_hour` → `s`/`sr`, `seven_day` → `w`/`wr`
  - `st` comes from `locked_reason` and `limits[].severity`
- `_usage_result` classifies the answer:
  - 401/403 → `TokenExpired`
  - 404 or a 400 "not applicable" → `UsageNotApplicable`
  - any other error → `None`
- `poll_token` tries the endpoint first and falls back to `poll_api` (Haiku).
- `CliProxySource`:
  - caches the account list for 5 minutes
  - polls each account through `api-call`
  - keeps a backoff per account
  - logs source-level failures once
- `poll_active` merges config dirs and proxy accounts.
- Backoff: a source that answered without usable data is not retried for 60 s. Before this, the 5 s tick hammered the API at about 12 requests per minute.
- Tests: `daemon/tests/test_usage_sources.py` (40 tests), using a sanitized fixture of the real response. The full suite is 188 passed, 2 skipped.

## Verified live

- A one-shot run with the key passed in memory polled the proxy account and pushed `s=7 w=8 st=allowed acct=pro` to the clock at 192.168.1.48.
- `/api/heap` reported `splash:false`.
- There were 0 `v1/messages` calls.
- A wrong key gave HTTP 401 → source skipped. A missing key gave a single log line.
- Keychain read was tested with a dummy item, which was then deleted.

## Open

- The user must rotate the CLIProxyAPI management key, because it was pasted in chat, and add the new key: `security add-generic-password -U -s clawdmeter-cliproxy -a "$USER" -w`.
- The LaunchAgent already has `cliproxy = on`. It shows usage as soon as the key is in the Keychain.
- `/api/oauth/usage` is undocumented. Unverified pieces:
  - the shape of an Enterprise response
  - `severity` values other than `normal`
  - a direct call with a subscription `claude login` token (only tested through the proxy)
- pyright's `Import "httpx" could not be resolved` is an environment false positive: it checks against Python 3.9.
