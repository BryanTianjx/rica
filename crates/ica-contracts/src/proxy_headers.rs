//! SEC-02 — Tauri HTTP proxy header policy (transport-neutral).
//!
//! The shell host (`desktop/tauri-host/src/main.rs`) forwards `/api` requests
//! from the webview to a loopback backend. Before SEC-02 its `forward_to`
//! rebuilt the request head from scratch and dropped every business header
//! except a hardcoded `Content-Type`. This module is the **single source of
//! truth** for what may cross that boundary, in each direction.
//!
//! Why it lives here and not in the host: the host is a detached workspace
//! (`tauri` needs glib-2.0 / WebKit system libs that this Linux box lacks),
//! so it cannot be compiled or tested here. Following the F1-3/F1-4 pattern,
//! the auditable policy and its tests sink into `ica-contracts`, which is pure
//! std + data; the host keeps only thin socket wiring and calls into it.
//!
//! Design rules (remediation plan §4.2/§4.4/§4.5/§4.6):
//! - explicit allowlist, never "forward whatever the browser sent";
//! - request and response allowlists are **separate** — a header allowed on
//!   the way in is not automatically allowed on the way out;
//! - hop-by-hop headers (RFC 9110 §7.6.1) and headers named in `Connection`
//!   are never forwarded in either direction;
//! - `Content-Length` is computed from the actual body by the caller, never
//!   trusted from the request;
//! - names/values/request-line are validated before serialization, so CRLF
//!   injection and request smuggling are impossible;
//! - error messages carry header **names only**, never values (§4.5.7: never
//!   log tokens or encryption keys).

use std::fmt;

/// Request headers copied from the webview to the loopback backend.
///
/// Lowercase spelling. Review each addition against remediation plan §4.4 —
/// this list, not the browser, decides what reaches the backend.
pub const REQUEST_ALLOWLIST: &[&str] = &[
    "authorization",     // SEC-02 T01: Bearer token must reach the server.
    "x-encrypt-aes-key", // SEC-02 T02: AES-256-GCM export envelope key.
    "content-type",      // JSON request bodies.
    "accept",            // content negotiation.
    "x-request-id",      // correlation id forwarded by existing clients.
    // SEC-P1 AUTH-08: host-minted identity assertion for the Python sidecar.
    // Response allowlist is deliberately unchanged: this never leaves the host.
    "x-ica-internal-assertion",
];

/// Response headers copied from the backend back to the webview.
///
/// Deliberately a different set. Internal diagnostics headers
/// (`x-secret-internal`, `server`, etc.) never leave the proxy.
pub const RESPONSE_ALLOWLIST: &[&str] = &[
    "content-type",
    "content-length",
    "cache-control",
    "etag",
    "last-modified",
    "location",         // 307 tail-slash redirects must survive (baseline parity).
    "www-authenticate", // 401 challenge, SEC02-T09.
    "x-request-id",
];

/// Hop-by-hop headers, never forwarded in either direction (§4.4 "禁止直接转发").
/// `Host` is also here: the proxy generates its own loopback `Host` value.
pub const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
];

/// Per-header value cap. The encryption key is base64(32 bytes) ≈ 44 chars;
/// a JWT is normally well under 8 KiB.
pub const MAX_HEADER_VALUE_BYTES: usize = 8 * 1024;
/// Total serialized header budget (name + ": " + value + CRLF per pair).
pub const MAX_TOTAL_HEADER_BYTES: usize = 16 * 1024;

/// A policy violation. **Never contains a header value** — only names and
/// sizes — so displaying it cannot leak a token or an encryption key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderPolicyError {
    InvalidName(String),
    UnsafeValue {
        name: String,
    },
    InvalidRequestLine(String),
    HeaderTooLarge {
        name: String,
        len: usize,
        limit: usize,
    },
    HeadersTooLarge {
        bytes: usize,
        limit: usize,
    },
    MalformedResponse,
    InvalidStatus,
}

impl fmt::Display for HeaderPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "invalid header name: {name}"),
            Self::UnsafeValue { name } => {
                write!(f, "header {name} contains a CR/LF/control character")
            }
            Self::InvalidRequestLine(what) => {
                write!(f, "unsafe request {what} (CR/LF/space/control character)")
            }
            Self::HeaderTooLarge { name, len, limit } => {
                write!(f, "header {name} is {len} bytes (limit {limit})")
            }
            Self::HeadersTooLarge { bytes, limit } => {
                write!(f, "headers total {bytes} bytes (limit {limit})")
            }
            Self::MalformedResponse => write!(f, "malformed backend response"),
            Self::InvalidStatus => write!(f, "no valid status code in backend response"),
        }
    }
}

impl std::error::Error for HeaderPolicyError {}

/// True for a hop-by-hop header (lowercase input).
pub fn is_hop_by_hop(lower_name: &str) -> bool {
    HOP_BY_HOP.contains(&lower_name)
}

/// RFC 9110 `tchar` — the only bytes legal in a header name or an HTTP method.
fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn validate_name(name: &str) -> Result<(), HeaderPolicyError> {
    if name.is_empty() || !name.bytes().all(is_token_byte) {
        return Err(HeaderPolicyError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Reject CR, LF, NUL and every other control byte except HTAB. This is the
/// CRLF-injection / response-splitting guard (§4.5.6).
fn validate_value(name: &str, value: &str) -> Result<(), HeaderPolicyError> {
    if value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
        return Err(HeaderPolicyError::UnsafeValue {
            name: name.to_string(),
        });
    }
    Ok(())
}

fn validate_request_line(what: &str, value: &str) -> Result<(), HeaderPolicyError> {
    if value.is_empty() || value.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(HeaderPolicyError::InvalidRequestLine(what.to_string()));
    }
    Ok(())
}

/// Apply an allowlist to `(name, value)` pairs, dropping hop-by-hop headers
/// and everything else not explicitly listed.
fn select<I, K, V>(
    headers: I,
    allowlist: &[&str],
) -> Result<Vec<(String, String)>, HeaderPolicyError>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let items: Vec<(String, String)> = headers
        .into_iter()
        .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
        .collect();

    // §4.4: tokens declared by `Connection` are hop-by-hop too, even when
    // their name happens to appear on an allowlist by mistake.
    let mut declared_hop: Vec<String> = Vec::new();
    for (name, value) in &items {
        if name.eq_ignore_ascii_case("connection") {
            for token in value.split(',') {
                let token = token.trim().to_ascii_lowercase();
                if !token.is_empty() {
                    declared_hop.push(token);
                }
            }
        }
    }

    let mut out: Vec<(String, String)> = Vec::new();
    let mut total = 0usize;
    for (name, value) in items {
        let lower = name.to_ascii_lowercase();
        if is_hop_by_hop(&lower) || declared_hop.iter().any(|t| t == &lower) {
            continue;
        }
        if !allowlist.contains(&lower.as_str()) {
            continue;
        }
        validate_name(&name)?;
        validate_value(&name, &value)?;
        let value = value.trim().to_string();
        if value.len() > MAX_HEADER_VALUE_BYTES {
            return Err(HeaderPolicyError::HeaderTooLarge {
                name,
                len: value.len(),
                limit: MAX_HEADER_VALUE_BYTES,
            });
        }
        total += name.len() + value.len() + 4; // ": " + CRLF
        if total > MAX_TOTAL_HEADER_BYTES {
            return Err(HeaderPolicyError::HeadersTooLarge {
                bytes: total,
                limit: MAX_TOTAL_HEADER_BYTES,
            });
        }
        out.push((lower, value));
    }
    Ok(out)
}

/// Reduce webview request headers to the explicit request allowlist.
pub fn select_request_headers<I, K, V>(
    headers: I,
) -> Result<Vec<(String, String)>, HeaderPolicyError>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    select(headers, REQUEST_ALLOWLIST)
}

/// Reduce backend response headers to the explicit response allowlist.
pub fn select_response_headers<I, K, V>(
    headers: I,
) -> Result<Vec<(String, String)>, HeaderPolicyError>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    select(headers, RESPONSE_ALLOWLIST)
}

/// Serialize a complete HTTP/1.1 request head (ending in `\r\n\r\n`).
///
/// `body_len` is computed by the caller from the real body bytes; a
/// client-supplied `Content-Length` is never trusted. `Host`, `Connection`
/// and `Content-Length` are generated here, so forwarded headers can never
/// duplicate or inject them.
pub fn render_request(
    method: &str,
    path: &str,
    port: u16,
    headers: &[(String, String)],
    body_len: usize,
) -> Result<Vec<u8>, HeaderPolicyError> {
    validate_request_line("method", method)?;
    validate_request_line("target", path)?;
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(HeaderPolicyError::InvalidRequestLine("target".to_string()));
    }

    let mut out = Vec::with_capacity(256 + headers.len() * 64);
    out.extend_from_slice(format!("{method} /{path} HTTP/1.1\r\n").as_bytes());
    out.extend_from_slice(format!("Host: 127.0.0.1:{port}\r\n").as_bytes());
    for (name, value) in headers {
        // Defence in depth: this function may be called by a future adapter
        // that skipped `select_request_headers`.
        validate_name(name)?;
        validate_value(name, value)?;
        let lower = name.to_ascii_lowercase();
        if is_hop_by_hop(&lower) || lower == "content-length" {
            continue;
        }
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(value.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"Connection: close\r\n");
    out.extend_from_slice(format!("Content-Length: {body_len}\r\n").as_bytes());
    out.extend_from_slice(b"\r\n");
    Ok(out)
}

/// A backend response after policy filtering: status, allowed headers and
/// the raw body bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Parse a raw HTTP/1.1 response and apply the **response** allowlist.
///
/// The proxy always asks for `Connection: close`, so the body is delimited by
/// connection close; `transfer-encoding` is a hop-by-hop header and is never
/// copied to the webview.
pub fn parse_response(raw: &[u8]) -> Result<ForwardResponse, HeaderPolicyError> {
    let header_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or(HeaderPolicyError::MalformedResponse)?;
    let head = &raw[..header_end];
    let mut lines = head.split(|&b| b == b'\n');
    let status_line = std::str::from_utf8(lines.next().unwrap_or_default())
        .map_err(|_| HeaderPolicyError::MalformedResponse)?
        .trim_end_matches('\r');
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|code| (100..600).contains(code))
        .ok_or(HeaderPolicyError::InvalidStatus)?;

    let mut pairs: Vec<(String, String)> = Vec::new();
    for line in lines {
        let line = std::str::from_utf8(line)
            .map_err(|_| HeaderPolicyError::MalformedResponse)?
            .trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue; // obsolete / malformed line: never forwarded
        };
        pairs.push((
            line[..colon].to_string(),
            line[colon + 1..].trim().to_string(),
        ));
    }
    let headers = select_response_headers(pairs)?;
    Ok(ForwardResponse {
        status,
        headers,
        body: raw[header_end + 4..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    const RESPONSE: &[u8] = b"HTTP/1.1 401 Unauthorized\r\n\
content-type: application/json\r\n\
content-length: 2\r\n\
location: /api/auth/refresh\r\n\
www-authenticate: Bearer\r\n\
x-secret-internal: must-not-leak\r\n\
transfer-encoding: chunked\r\n\
connection: close\r\n\r\n{}";

    /// Tiny std-only loopback backend. Each accepted connection records its
    /// request head on `rx` and answers with `RESPONSE` (Connection: close).
    fn backend(connections: usize) -> (u16, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for _ in 0..connections {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buf = Vec::new();
                let mut one = [0u8; 1];
                loop {
                    if buf.ends_with(b"\r\n\r\n") {
                        break;
                    }
                    if stream.read(&mut one).unwrap() == 0 {
                        break;
                    }
                    buf.push(one[0]);
                    assert!(buf.len() < 64 * 1024, "request head too large");
                }
                let head_str = String::from_utf8_lossy(&buf);
                tx.send(head_str.to_string()).unwrap();
                // Drain the declared body BEFORE replying: closing a socket
                // with unread request bytes turns the client's read into a
                // TCP RST instead of a clean EOF (a known harness race).
                let body_len: usize = head_str
                    .lines()
                    .filter_map(|line| {
                        let (name, value) = line.trim_end_matches('\r').split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse().ok()
                        } else {
                            None
                        }
                    })
                    .next()
                    .unwrap_or(0);
                if body_len > 0 {
                    let mut body = vec![0u8; body_len];
                    stream.read_exact(&mut body).unwrap();
                }
                stream.write_all(RESPONSE).unwrap();
            }
        });
        (port, rx)
    }

    fn raw_call(port: u16, head: &[u8], body: &[u8]) -> ForwardResponse {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(head).unwrap();
        stream.write_all(body).unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        parse_response(&raw).unwrap()
    }

    fn proxy_call(
        port: u16,
        method: &str,
        path: &str,
        input: Vec<(&str, &str)>,
        body: &[u8],
    ) -> ForwardResponse {
        let selected = select_request_headers(input).unwrap();
        let head = render_request(method, path, port, &selected, body.len()).unwrap();
        raw_call(port, &head, body)
    }

    // SEC02-T01/T02/T03 — business headers survive the proxy.
    #[test]
    fn business_headers_reach_backend_and_browser_junk_does_not() {
        let (port, rx) = backend(1);
        let body = br#"{"a":1}"#;
        let response = proxy_call(
            port,
            "POST",
            "api/export",
            vec![
                ("authorization", "Bearer [REDACTED]"),
                ("x-encrypt-aes-key", "YT9i"),
                ("content-type", "application/json"),
                ("accept", "application/json"),
                ("x-request-id", "req-42"),
                ("x-ica-internal-assertion", "header.payload.signature"),
                // browser-only / forbidden headers
                ("user-agent", "Mozilla/5.0"),
                ("sec-fetch-mode", "cors"),
                ("cookie", "session=leak"),
                ("connection", "keep-alive, x-internal-drop"),
                ("x-internal-drop", "must-not-arrive"),
                ("proxy-authorization", "Basic [REDACTED]"),
                ("host", "evil.example"),
                ("upgrade", "websocket"),
            ],
            body,
        );
        let head = rx.recv().unwrap();
        for expected in [
            "authorization: Bearer [REDACTED]",
            "x-encrypt-aes-key: YT9i",
            "content-type: application/json",
            "accept: application/json",
            "x-request-id: req-42",
            "x-ica-internal-assertion: header.payload.signature",
            "connection: close",
        ] {
            assert!(
                head.to_ascii_lowercase()
                    .contains(&expected.to_ascii_lowercase()),
                "missing {expected:?} in forwarded head:\n{head}"
            );
        }
        for forbidden in [
            "user-agent",
            "sec-fetch-mode",
            "cookie",
            "proxy-authorization",
            "x-internal-drop",
            "upgrade",
            "evil.example",
        ] {
            assert!(
                !head.to_ascii_lowercase().contains(forbidden),
                "forbidden {forbidden:?} leaked into forwarded head:\n{head}"
            );
        }
        assert!(head.contains(&format!("Host: 127.0.0.1:{port}\r\n")));
        assert!(head.contains(&format!("Content-Length: {}\r\n", body.len())));
        // SEC02-T09/T10: status and body are preserved through the proxy.
        assert_eq!(response.status, 401);
        assert_eq!(response.body, b"{}");
    }

    // SEC02-T05 — CR/LF injection and oversized headers are rejected.
    #[test]
    fn malicious_headers_are_rejected() {
        // A CRLF-bearing *name* is not on the allowlist, so it is silently
        // dropped rather than forwarded — safe filtering (§4.5.6).
        assert!(
            select_request_headers(vec![("authorization\r\nx-evil: 1", "v")])
                .unwrap()
                .is_empty()
        );

        let bad = select_request_headers(vec![("authorization", "Bearer a\r\nx-evil: 1")]);
        assert_eq!(
            bad,
            Err(HeaderPolicyError::UnsafeValue {
                name: "authorization".to_string()
            })
        );
        // Error text must disclose the name but never the value.
        assert!(!bad.unwrap_err().to_string().contains("x-evil: 1"));

        let too_big = select_request_headers(vec![(
            "authorization",
            "x".repeat(MAX_HEADER_VALUE_BYTES + 1),
        )]);
        assert!(matches!(
            &too_big,
            Err(HeaderPolicyError::HeaderTooLarge { .. })
        ));
        assert!(!too_big.unwrap_err().to_string().contains(&"x".repeat(32)));

        assert!(matches!(
            render_request("GET\r\nX: 1", "api/x", 1, &[], 0),
            Err(HeaderPolicyError::InvalidRequestLine(_))
        ));
        assert!(matches!(
            render_request("GET", "api/x\r\nX: 1", 1, &[], 0),
            Err(HeaderPolicyError::InvalidRequestLine(_))
        ));
        assert!(matches!(
            render_request(
                "GET",
                "api/x",
                1,
                &[("authorization\r\nx".to_string(), "v".to_string())],
                0
            ),
            Err(HeaderPolicyError::InvalidName(_))
        ));
    }

    #[test]
    fn connection_declared_hop_headers_are_dropped_and_content_length_is_computed() {
        let selected = select_request_headers(vec![
            ("connection", "accept, x-request-id"),
            ("accept", "application/json"),
            ("x-request-id", "req-9"),
            ("authorization", "Bearer t"),
        ])
        .unwrap();
        assert_eq!(
            selected,
            vec![("authorization".to_string(), "Bearer t".to_string())]
        );
        let head = render_request("POST", "api/x", 9, &selected, 123);
        let head = String::from_utf8(head.unwrap()).unwrap();
        // A forged Content-Length from the caller can never appear twice.
        assert_eq!(head.matches("Content-Length:").count(), 1);
        assert!(head.contains("Content-Length: 123\r\n"));
        assert_eq!(head.matches("Host:").count(), 1);
        assert_eq!(head.matches("Connection:").count(), 1);
    }

    // SEC02-T04/T09/T10 + §4.6: response allowlist is separate from request.
    #[test]
    fn response_headers_are_filtered_with_a_separate_allowlist() {
        let parsed = parse_response(RESPONSE).unwrap();
        assert_eq!(parsed.status, 401);
        assert_eq!(parsed.body, b"{}");
        let names: Vec<&str> = parsed.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "content-type",
                "content-length",
                "location",
                "www-authenticate"
            ]
        );
        // A response header that is allowed on the request side is still
        // filtered here unless it is also on RESPONSE_ALLOWLIST.
        assert!(!names.contains(&"authorization"));
        assert!(parse_response(b"not http").is_err());
    }

    // SEC02-T11 — proxy and a direct HTTP client produce the same backend
    // behaviour for the business headers (the proxy additionally strips
    // browser noise, which the backend never needed).
    #[test]
    fn proxy_matches_direct_http_for_business_headers() {
        let (port, rx) = backend(2);
        let body = b"payload";

        let direct_head = format!(
            "POST /api/export HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
authorization: Bearer same-token\r\n\
x-encrypt-aes-key: same-key\r\n\
content-type: application/json\r\n\
accept: application/json\r\n\
x-request-id: same-id\r\n\
content-length: {}\r\n\
connection: close\r\n\r\n",
            body.len()
        );
        let direct = raw_call(port, direct_head.as_bytes(), body);
        let direct_seen = rx.recv().unwrap();

        let proxied = proxy_call(
            port,
            "POST",
            "api/export",
            vec![
                ("authorization", "Bearer same-token"),
                ("x-encrypt-aes-key", "same-key"),
                ("content-type", "application/json"),
                ("accept", "application/json"),
                ("x-request-id", "same-id"),
                ("user-agent", "browser-noise"),
                ("sec-fetch-dest", "empty"),
            ],
            body,
        );
        let proxy_seen = rx.recv().unwrap();

        for name in [
            "authorization",
            "x-encrypt-aes-key",
            "content-type",
            "accept",
            "x-request-id",
        ] {
            let pick = |head: &str| -> Vec<String> {
                head.lines()
                    .filter(|l| l.to_ascii_lowercase().starts_with(name))
                    .map(|l| l.trim_end_matches('\r').to_string())
                    .collect()
            };
            assert_eq!(pick(&direct_seen), pick(&proxy_seen), "{name} diverged");
        }
        assert_eq!(direct.status, proxied.status);
        assert_eq!(direct.headers, proxied.headers);
        assert_eq!(direct.body, proxied.body);
    }

    // F1-4-style drift guard: this Linux box cannot compile the Tauri host,
    // so assert at the source level that the shell still routes through the
    // shared policy. ARCH-P3 moved the call into `ica-desktop::proxy` (the
    // compilable layer) and left the host a thin `forward` call, so the
    // guard follows the delegation instead of asserting the old host body.
    #[test]
    fn host_wires_the_shared_policy() {
        let host = include_str!("../../../desktop/tauri-host/src/main.rs");
        let proxy = include_str!("../../../crates/ica-desktop/src/proxy.rs");
        assert!(
            host.contains("ica_desktop::proxy::forward("),
            "host must delegate the forward to ica-desktop"
        );
        assert!(host.contains("request.headers().clone()"));
        assert!(proxy.contains("proxy_headers::select_request_headers"));
        assert!(proxy.contains("proxy_headers::render_request"));
        assert!(proxy.contains("proxy_headers::parse_response"));
        assert!(
            !host.contains("Authorization: Bearer"),
            "no hand-built Authorization header may remain in the host"
        );
    }
}
