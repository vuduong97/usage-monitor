//! A minimal HTTP/1.1 client over `std::net::TcpStream`: byte-exact requests (a bad `Host`, a
//! lying `Content-Length`, a half-sent head) that `reqwest` would refuse or fix up. Loopback only.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::Value;

/// How long a test waits for an answer before it calls the server hung.
pub const READ_TIMEOUT: Duration = Duration::from_secs(15);

/// What the server answered.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    /// Header names lower-cased, values as sent.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("answer is not JSON ({e}): {}", self.text()))
    }
}

/// A request as it goes on the wire.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub target: String,
    /// `Some(h)` sends `Host: h`; `None` sends none. Default: `127.0.0.1:<port>`.
    pub host: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Send this `Content-Length` instead of the body's length.
    pub declared_length: Option<u64>,
    /// Send the body as one chunk with `Transfer-Encoding: chunked` instead of a length.
    pub chunked: bool,
}

impl Request {
    pub fn new(port: u16, method: &str, target: &str) -> Self {
        Self {
            method: method.to_owned(),
            target: target.to_owned(),
            host: Some(format!("127.0.0.1:{port}")),
            headers: Vec::new(),
            body: Vec::new(),
            declared_length: None,
            chunked: false,
        }
    }

    pub fn get(port: u16, target: &str) -> Self {
        Self::new(port, "GET", target)
    }

    pub fn post(port: u16, target: &str, body: &[u8]) -> Self {
        let mut request = Self::new(port, "POST", target);
        request.body = body.to_vec();
        request
    }

    pub fn host(mut self, host: Option<&str>) -> Self {
        self.host = host.map(str::to_owned);
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn declared_length(mut self, n: u64) -> Self {
        self.declared_length = Some(n);
        self
    }

    pub fn chunked(mut self) -> Self {
        self.chunked = true;
        self
    }

    /// The head, ready for [`send_raw`] (the body is appended separately).
    fn head(&self) -> Vec<u8> {
        let mut head = format!("{} {} HTTP/1.1\r\n", self.method, self.target);
        if let Some(host) = &self.host {
            head.push_str(&format!("Host: {host}\r\n"));
        }
        for (k, v) in &self.headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str("Connection: close\r\n");
        if self.chunked {
            head.push_str("Transfer-Encoding: chunked\r\n");
        } else if self.method != "GET" || !self.body.is_empty() {
            let n = self.declared_length.unwrap_or(self.body.len() as u64);
            head.push_str(&format!("Content-Length: {n}\r\n"));
        }
        head.push_str("\r\n");
        head.into_bytes()
    }

    fn wire(&self) -> Vec<u8> {
        let mut bytes = self.head();
        if self.chunked {
            bytes.extend_from_slice(format!("{:x}\r\n", self.body.len()).as_bytes());
            bytes.extend_from_slice(&self.body);
            bytes.extend_from_slice(b"\r\n0\r\n\r\n");
        } else {
            bytes.extend_from_slice(&self.body);
        }
        bytes
    }
}

pub fn connect(port: u16) -> io::Result<TcpStream> {
    let stream = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_secs(5),
    )?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(READ_TIMEOUT))?;
    Ok(stream)
}

/// Parse everything a server wrote until it closed the connection.
pub fn parse_reply(raw: &[u8]) -> io::Result<Reply> {
    let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_owned());
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| bad("no end of head"))?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| bad("bad status line"))?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    Ok(Reply {
        status,
        headers,
        body: raw[split + 4..].to_vec(),
    })
}

/// Write `bytes`, then read the answer to the end. A write that fails half way (the server
/// answered and hung up early) is not an error as long as an answer can be read.
pub fn send_raw(port: u16, bytes: &[u8]) -> io::Result<Reply> {
    let mut stream = connect(port)?;
    let _ = stream.write_all(bytes);
    let _ = stream.flush();
    read_reply(&mut stream)
}

pub fn read_reply(stream: &mut TcpStream) -> io::Result<Reply> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buffer[..n]),
            // A reset after the answer was sent is how a server closes on unread input.
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset && !raw.is_empty() => break,
            Err(e) => return Err(e),
        }
    }
    parse_reply(&raw)
}

/// Send `request` and read the answer.
pub fn send(port: u16, request: &Request) -> io::Result<Reply> {
    send_raw(port, &request.wire())
}

/// `POST /hook/<agent>?e=<hint>` with a JSON body, the way the hook script does.
pub fn post_hook(port: u16, agent: &str, hint: &str, payload: &Value) -> io::Result<Reply> {
    let body = serde_json::to_vec(payload).expect("json");
    send(
        port,
        &Request::post(port, &format!("/hook/{agent}?e={hint}"), &body)
            .header("Content-Type", "application/json"),
    )
}

pub fn get_state(port: u16) -> io::Result<Reply> {
    send(port, &Request::get(port, "/state"))
}

pub fn post_poke(port: u16, body: &Value) -> io::Result<Reply> {
    let body = serde_json::to_vec(body).expect("json");
    send(
        port,
        &Request::post(port, "/poke", &body).header("Content-Type", "application/json"),
    )
}
