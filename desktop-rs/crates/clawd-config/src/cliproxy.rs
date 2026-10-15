//! CLIProxyAPI settings (`usage.py:107-113, 753-777`): the management key lives only in the
//! macOS Keychain; the URL comes from the `cliproxy_url` option. The proxy calls themselves are
//! `clawd_usage::CliProxySource`.

use clawd_contracts::AppPaths;

use crate::command::CommandRunner;

/// `usage.CLIPROXY_DEFAULT_URL`.
pub const CLIPROXY_DEFAULT_URL: &str = "http://localhost:8317";
/// `usage.CLIPROXY_KEYCHAIN_SERVICE`.
pub const CLIPROXY_KEYCHAIN_SERVICE: &str = "clawd-cliproxy";
/// `usage.CLIPROXY_LEGACY_SERVICE`: the name before the rename; keys saved then still work.
pub const CLIPROXY_LEGACY_SERVICE: &str = "clawdmeter-cliproxy";
/// `usage.CLIPROXY_AUTH_FILES_PATH`.
pub const CLIPROXY_AUTH_FILES_PATH: &str = "/v0/management/auth-files";
/// `usage.CLIPROXY_API_CALL_PATH`.
pub const CLIPROXY_API_CALL_PATH: &str = "/v8/management/requests/api-call";

/// `usage.read_cliproxy_key()`: the management key from the Keychain (current service name, else
/// the one used before the rename) via `security find-generic-password -s <service> -w`, with the
/// hex-dump decode; `None` off macOS or when absent.
pub fn read_cliproxy_key(runner: &dyn CommandRunner) -> Option<String> {
    todo!("phase 06")
}

/// `usage.read_cliproxy_url()`: `cliproxy_url` (trailing `/` stripped, `http://` added when it
/// has no scheme), default [`CLIPROXY_DEFAULT_URL`].
pub fn read_cliproxy_url(paths: &AppPaths) -> String {
    todo!("phase 06")
}
