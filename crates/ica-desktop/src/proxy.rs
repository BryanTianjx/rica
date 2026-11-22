//! The blocking loopback HTTP forward, moved out of the Tauri host
//! (ARCH-P3 AC1).
//!
//! It carries no Tauri types: the host converts its `HeaderMap` into pairs
//! and the response back into a `tauri::http::Response`. Request/response
//! header filtering stays in the shared `ica_contracts::proxy_headers`
//! allowlist, so the proxy and a direct HTTP call still observe identical
//! business behaviour (SEC-02).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use ica_contracts::proxy_headers::{self, ForwardResponse};

/// Default read timeout for a forwarded request (the P1 host used 30s).
pub const FORWARD_TIMEOUT_MS: u64 = 30_000;

/// Forward one request to `127.0.0.1:<port>` and apply both header policies.
pub fn forward(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
    timeout_ms: u64,
) -> Result<ForwardResponse, String> {
    if port == 0 {
        return Err("backend not running (port 0)".to_string());
    }
    let selected = proxy_headers::select_request_headers(headers.iter().cloned())
        .map_err(|error| error.to_string())?;
    let request = proxy_headers::render_request(method, path, port, &selected, body.len())
        .map_err(|error| error.to_string())?;

    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&request)
        .map_err(|error| error.to_string())?;
    stream.write_all(body).map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| error.to_string())?;
    proxy_headers::parse_response(&response).map_err(|error| error.to_string())
}

/// Poll `/health` until it returns 200 or the deadline passes. Used by the
/// Sidecar Manager's handshake and by the host's startup diagnostics.
pub fn probe_health(port: u16, timeout_ms: u64, interval_ms: u64) -> bool {
    if port == 0 {
        return false;
    }
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if let Ok(reply) = forward(port, "GET", "health", &[], b"", interval_ms.max(1)) {
            if reply.status == 200 {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(interval_ms.clamp(1, 200)));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_zero_is_never_forwarded() {
        let err = forward(0, "GET", "health", &[], b"", FORWARD_TIMEOUT_MS)
            .expect_err("no backend means no request");
        assert!(err.contains("port 0"));
        assert!(!probe_health(0, 10, 5));
    }

    #[test]
    fn request_rendering_goes_through_the_shared_allowlist() {
        // A hop-by-hop header supplied by a webview must not reach a backend.
        let selected = proxy_headers::select_request_headers([("Connection", "keep-alive")])
            .expect("selection succeeds");
        assert!(selected.is_empty(), "hop-by-hop headers are filtered");
        let rendered = proxy_headers::render_request("GET", "health", 1234, &selected, 0)
            .expect("request renders");
        let head = String::from_utf8(rendered).expect("ascii head");
        assert!(head.starts_with("GET /health HTTP/1.1\r\n"));
        assert!(!head.to_ascii_lowercase().contains("keep-alive"));
        assert!(head.contains("Host: 127.0.0.1:1234\r\n"));
    }
}
