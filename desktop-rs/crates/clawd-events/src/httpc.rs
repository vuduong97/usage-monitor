//! The little HTTP/1.1 client the pusher and the daemon probe need: one request per connection,
//! plain `http://` only (clocks and the loopback server speak nothing else), hard time limit.

use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::header::{CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST};
use hyper::{Method, Request, Uri};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

/// Replies the pusher reads are tiny (`/api/heap`, `/state`); anything bigger is not a clock.
const MAX_REPLY: usize = 1024 * 1024;

/// One finished request: status and body.
pub(crate) struct Reply {
    pub status: u16,
    pub body: Bytes,
}

/// `method url` with an optional JSON body; `Err(why)` for anything that is not a complete
/// reply within `timeout` (connect, send, read: the whole exchange).
pub(crate) async fn request(
    method: Method,
    url: &str,
    json_body: Option<String>,
    timeout: Duration,
) -> Result<Reply, String> {
    match tokio::time::timeout(timeout, exchange(method, url, json_body)).await {
        Ok(result) => result,
        Err(_) => Err("timed out".to_owned()),
    }
}

async fn exchange(method: Method, url: &str, json_body: Option<String>) -> Result<Reply, String> {
    let uri: Uri = url.parse().map_err(|e| format!("bad url: {e}"))?;
    if uri.scheme_str() != Some("http") {
        return Err("only http:// URLs are supported".to_owned());
    }
    let host = uri.host().ok_or("url has no host")?.to_owned();
    let port = uri.port_u16().unwrap_or(80);
    let authority = uri
        .authority()
        .map(|a| a.as_str().to_owned())
        .unwrap_or_else(|| host.clone());
    let path = uri.path_and_query().map_or("/", |p| p.as_str()).to_owned();

    let stream = TcpStream::connect((host.trim_matches(|c| c == '[' || c == ']'), port))
        .await
        .map_err(|e| e.to_string())?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|e| e.to_string())?;
    let driver = tokio::spawn(async move {
        let _ = connection.await;
    });

    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(HOST, authority)
        .header(CONNECTION, "close");
    let body = match json_body {
        Some(text) => {
            builder = builder
                .header(CONTENT_TYPE, "application/json")
                .header(CONTENT_LENGTH, text.len());
            Full::new(Bytes::from(text))
        }
        None => Full::new(Bytes::new()),
    };
    let request = builder.body(body).map_err(|e| e.to_string())?;
    let result = async {
        let response = sender
            .send_request(request)
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let body = Limited::new(response.into_body(), MAX_REPLY)
            .collect()
            .await
            .map_err(|e| e.to_string())?
            .to_bytes();
        Ok(Reply { status, body })
    }
    .await;
    driver.abort();
    result
}
