//! CLIProxyAPI settings (`usage.py:107-113, 753-777`): the management key lives only in the
//! macOS Keychain; the URL comes from the `cliproxy_url` option. The proxy calls themselves are
//! `clawd_usage::CliProxySource`.

use std::time::Duration;

use clawd_contracts::AppPaths;

use crate::command::CommandRunner;
use crate::config_file::read_config_value;
use crate::host::Host;
use crate::pytext::strip;
use crate::tokens::decode_keychain_blob;

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

/// `subprocess.run(..., timeout=10)` of `read_cliproxy_key`.
const KEYCHAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// `usage.read_cliproxy_key()`: the management key from the Keychain (current service name, else
/// the one used before the rename) via `security find-generic-password -s <service> -w`, with the
/// hex-dump decode; `None` off macOS or when absent.
pub fn read_cliproxy_key(runner: &dyn CommandRunner) -> Option<String> {
    read_cliproxy_key_on(runner, &Host::system())
}

/// [`read_cliproxy_key`] for an explicit [`Host`] (`sys.platform`).
pub fn read_cliproxy_key_on(runner: &dyn CommandRunner, host: &Host) -> Option<String> {
    if !host.macos {
        return None;
    }
    for service in [CLIPROXY_KEYCHAIN_SERVICE, CLIPROXY_LEGACY_SERVICE] {
        let args = ["find-generic-password", "-s", service, "-w"];
        match runner.run("security", &args, KEYCHAIN_TIMEOUT) {
            // `security` not installed or hung: give up (a failed lookup tries the next name)
            Err(e) => {
                tracing::warn!("cliproxy: Keychain access error: {e}");
                return None;
            }
            Ok(out) if out.status != 0 => continue,
            Ok(out) => {
                let key = strip(&decode_keychain_blob(&out.stdout)).to_owned();
                if !key.is_empty() {
                    return Some(key);
                }
            }
        }
    }
    None
}

/// `usage.read_cliproxy_url()`: `cliproxy_url` (trailing `/` stripped, `http://` added when it
/// has no scheme), default [`CLIPROXY_DEFAULT_URL`].
pub fn read_cliproxy_url(paths: &AppPaths) -> String {
    let value = read_config_value(paths, "cliproxy_url")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| CLIPROXY_DEFAULT_URL.to_owned());
    let url = strip(&value).trim_end_matches('/');
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_owned()
    } else {
        format!("http://{url}")
    }
}
