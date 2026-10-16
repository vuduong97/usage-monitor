//! The HTTP side of the updater: [`Transport`] (what `httpx.BaseTransport` is in the Python
//! tests), the real [`ReqwestTransport`] that never follows a redirect by itself, `trusted()`
//! and the hop loops that call it on **every** hop.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use clawd_contracts::UpdateError;
use reqwest::Url;

/// A boxed `Send` future that may borrow (the transport traits below).
pub type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The body of a [`Reply`], read chunk by chunk (downloads are streamed, never held whole).
pub trait Body: Send {
    /// The next piece, `Ok(None)` at the end. Errors are already worded for the user
    /// (`ConnectError: ...`, `TimeoutException: ...`).
    fn chunk(&mut self) -> Fut<'_, Result<Option<Vec<u8>>, UpdateError>>;
}

/// An answer: status, the `Location` header (redirects) and the body stream.
pub struct Reply {
    pub status: u16,
    pub location: Option<String>,
    pub body: Box<dyn Body>,
}

/// One `GET`, no redirect following: the caller walks the hops (calling `trusted()` on each).
/// `timeout` applies to each operation (connect, each read), like `httpx` timeouts.
pub trait Transport: Send + Sync {
    fn get(&self, url: &str, timeout: Duration) -> Fut<'_, Result<Reply, UpdateError>>;
}

/// Redirect statuses the hop loops follow.
pub fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Hops a download or a small file may take (`for _ in range(4)`).
pub const MAX_HOPS: usize = 4;

/// `Source.trusted(url)`: where a file may come from. `https` only to GitHub and its asset CDN
/// (`github.com`, `api.github.com`, any `*.githubusercontent.com`); `http` only to a loopback
/// host and only when the API itself is loopback (`loopback_api`: tests, dev). The port is not
/// looked at (as in Python); the URL must be written `scheme://host...`.
pub fn trusted(url: &str, loopback_api: bool) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let scheme = parsed.scheme();
    if !url
        .to_ascii_lowercase()
        .starts_with(&format!("{scheme}://"))
    {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    match scheme {
        "https" => {
            matches!(host, "github.com" | "api.github.com")
                || host.ends_with(".githubusercontent.com")
        }
        "http" => loopback_api && matches!(host, "127.0.0.1" | "localhost"),
        _ => false,
    }
}

/// The host of `url` is `127.0.0.1` / `localhost` (`Source._loopback`).
pub fn is_loopback_host(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| matches!(h, "127.0.0.1" | "localhost")))
        .unwrap_or(false)
}

/// `httpx.URL(url).join(location)`: the next hop. `None` when it does not parse.
pub fn join_location(url: &str, location: &str) -> Option<String> {
    Url::parse(url).ok()?.join(location).ok().map(Into::into)
}

/// Read a whole body, refusing more than `limit` bytes (`None` = over the limit).
pub async fn read_limited(body: &mut dyn Body, limit: u64) -> Result<Option<Vec<u8>>, UpdateError> {
    let mut out = Vec::new();
    while let Some(chunk) = body.chunk().await? {
        if (out.len() + chunk.len()) as u64 > limit {
            return Ok(None);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(Some(out))
}

/// The production [`Transport`]: `reqwest` + rustls, `redirect::Policy::none()`.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new(user_agent: &str) -> Result<Self, UpdateError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
        );
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(30))
            .user_agent(user_agent)
            .default_headers(headers)
            .build()
            .map_err(|e| UpdateError(net_error(&e)))?;
        Ok(Self { client })
    }
}

/// `f"{type(e).__name__}: {e}"` for a failed request.
fn net_error(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "TimeoutException"
    } else if e.is_connect() {
        "ConnectError"
    } else {
        "RequestError"
    };
    let mut text = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(inner) = source {
        text.push_str(": ");
        text.push_str(&inner.to_string());
        source = inner.source();
    }
    format!("{kind}: {text}")
}

struct ReqwestBody {
    response: reqwest::Response,
    timeout: Duration,
}

impl Body for ReqwestBody {
    fn chunk(&mut self) -> Fut<'_, Result<Option<Vec<u8>>, UpdateError>> {
        Box::pin(async move {
            match tokio::time::timeout(self.timeout, self.response.chunk()).await {
                Err(_) => Err(UpdateError("TimeoutException: ReadTimeout".to_owned())),
                Ok(Err(e)) => Err(UpdateError(net_error(&e))),
                Ok(Ok(chunk)) => Ok(chunk.map(|b| b.to_vec())),
            }
        })
    }
}

impl Transport for ReqwestTransport {
    fn get(&self, url: &str, timeout: Duration) -> Fut<'_, Result<Reply, UpdateError>> {
        let url = url.to_owned();
        Box::pin(async move {
            let sent = tokio::time::timeout(timeout, self.client.get(&url).send()).await;
            let response = match sent {
                Err(_) => return Err(UpdateError("TimeoutException: ReadTimeout".to_owned())),
                Ok(Err(e)) => return Err(UpdateError(net_error(&e))),
                Ok(Ok(r)) => r,
            };
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            Ok(Reply {
                status: response.status().as_u16(),
                location,
                body: Box::new(ReqwestBody { response, timeout }),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_is_https_github_or_loopback_http_with_a_loopback_api() {
        for url in [
            "https://api.github.com/repos/o/r/releases/latest",
            "https://github.com/o/r/releases/download/v1/x",
            "https://release-assets.githubusercontent.com/o/x",
            "https://objects.githubusercontent.com/x",
            "https://raw.githubusercontent.com/o/r/main/x",
            "https://gist.githubusercontent.com/o/x",
            "HTTPS://API.GITHUB.COM/x",
        ] {
            assert!(trusted(url, false), "{url}");
        }
        for url in [
            "http://api.github.com/x",
            "https://github.com.evil.example/x",
            "https://evilgithubusercontent.com/x",
            "https://githubusercontent.com/x",
            "https://evil.example/github.com",
            "https://github.com@evil.example/x",
            "https:github.com/x",
            "ftp://github.com/x",
            "",
            "github.com/x",
            "http://127.0.0.1:1/x",
            "https://127.0.0.1/x",
        ] {
            assert!(!trusted(url, false), "{url}");
        }
        assert!(trusted("http://127.0.0.1:8080/x", true));
        assert!(trusted("http://localhost:1/x", true));
        assert!(!trusted("http://evil.example/x", true));
        assert!(!trusted("http://[::1]:1/x", true));
    }

    #[test]
    fn locations_join_like_httpx() {
        assert_eq!(
            join_location("https://github.com/a/b", "/c").as_deref(),
            Some("https://github.com/c")
        );
        assert_eq!(
            join_location(
                "https://github.com/a/b",
                "https://cdn.githubusercontent.com/x"
            )
            .as_deref(),
            Some("https://cdn.githubusercontent.com/x")
        );
        assert_eq!(join_location("not a url", "/x"), None);
    }
}
