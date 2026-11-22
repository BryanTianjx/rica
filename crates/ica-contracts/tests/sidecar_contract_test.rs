//! Sidecar Contract Test (ARCH-P4 AC5).
//!
//! Independent of the supervisor and of any executor process: this suite
//! exercises the wire contract itself — encode/decode, version negotiation,
//! the golden DTO shapes and every rejection path. A language-neutral
//! executor can implement against these vectors without importing any Rust
//! module, and a future protocol bump must update them deliberately.
//!
//! No Python module, file path or runtime detail appears in these vectors.

use ica_contracts::sidecar_protocol::error_code;
use ica_contracts::sidecar_protocol::method;
use ica_contracts::sidecar_protocol::{
    decode_request, decode_response, encode_request, encode_response, negotiate_version,
    validate_request, CapabilityGrant, PathAccess, PathGrant, SidecarError, SidecarRequest,
    SidecarResponse, PROTOCOL_VERSION,
};
use serde_json::{json, Value};

fn grant(capability: &str) -> Vec<CapabilityGrant> {
    vec![CapabilityGrant {
        capability: capability.to_string(),
        allowed_paths: vec![],
        allowed_hosts: vec![],
    }]
}

fn request(method_name: &str, params: Value) -> SidecarRequest {
    SidecarRequest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        request_id: "r-contract-1".to_string(),
        method: method_name.to_string(),
        params,
        grants: grant(method_name),
        timeout_ms: 30_000,
    }
}

#[test]
fn golden_request_and_response_shapes_are_stable() {
    let request = request(method::OPEN, json!({"url": "https://example.com"}));
    let line = encode_request(&request).expect("encode request");
    let wire: Value = serde_json::from_str(&line).expect("wire is JSON");
    assert_eq!(
        wire,
        json!({
            "protocol_version": "1.0",
            "request_id": "r-contract-1",
            "method": "browser.open",
            "params": {"url": "https://example.com"},
            "grants": [{
                "capability": "browser.open",
                "allowed_paths": [],
                "allowed_hosts": []
            }],
            "timeout_ms": 30_000
        })
    );
    // Decoding another implementation's identical line yields the same DTO.
    assert_eq!(decode_request(&line).expect("decode request"), request);

    let reply = SidecarResponse::ok("r-contract-1", json!({"status": "ok"}));
    let reply_line = encode_response(&reply).expect("encode response");
    assert_eq!(
        serde_json::from_str::<Value>(&reply_line).unwrap(),
        json!({
            "protocol_version": "1.0",
            "request_id": "r-contract-1",
            "result": {"status": "ok"},
            "error": null
        })
    );
    assert_eq!(
        decode_response(&reply_line).expect("decode response"),
        reply
    );
}

#[test]
fn every_capability_method_has_a_contract_vector() {
    for method_name in method::ALL {
        let request = request(method_name, json!({}));
        let line =
            encode_request(&request).unwrap_or_else(|error| panic!("{method_name}: {error}"));
        assert_eq!(decode_request(&line).unwrap(), request, "{method_name}");
    }
}

#[test]
fn version_negotiation_is_explicit_and_symmetric() {
    assert_eq!(negotiate_version(PROTOCOL_VERSION), Ok(PROTOCOL_VERSION));
    for bad in ["0.9", "1.1", "2.0", "", "1"] {
        let error = decode_request(&format!(
            r#"{{"protocol_version":"{bad}","request_id":"r","method":"browser.read","grants":[],"timeout_ms":1}}"#
        ))
        .expect_err("unsupported version must be rejected");
        assert_eq!(error.code(), error_code::UNSUPPORTED_VERSION, "{bad}");
        let response = SidecarResponse::failure("r", error.to_dto());
        let line = encode_response(&response).unwrap();
        let wire: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            wire["error"]["code"],
            json!(error_code::UNSUPPORTED_VERSION)
        );
        assert_eq!(wire["error"]["retryable"], json!(false));
    }
}

#[test]
fn error_paths_produce_stable_wire_codes() {
    let cases: Vec<(SidecarRequest, &str)> = vec![
        (
            {
                let mut r = request("draft.approve", json!({}));
                r.grants = grant("draft.approve");
                r
            },
            error_code::BUSINESS_METHOD,
        ),
        (
            {
                let mut r = request(method::OPEN, json!({}));
                r.grants = vec![];
                r
            },
            error_code::CAPABILITY_DENIED,
        ),
        (
            request(method::OPEN, json!({"database_url": "sqlite:///ica.db"})),
            error_code::FORBIDDEN_PARAMETER,
        ),
        (
            request(method::OPEN, json!({"credentials": {"api_key": "x"}})),
            error_code::FORBIDDEN_PARAMETER,
        ),
        (
            {
                let mut r = request(method::SCREENSHOT, json!({}));
                r.grants = vec![CapabilityGrant {
                    capability: method::SCREENSHOT.to_string(),
                    allowed_paths: vec![PathGrant {
                        root: "/mnt/o/ica/backend/ica.db".to_string(),
                        access: PathAccess::Write,
                        purpose: "not allowed".to_string(),
                    }],
                    allowed_hosts: vec![],
                }];
                r
            },
            error_code::PATH_DENIED,
        ),
        (
            {
                let mut r = request(method::READ, json!({}));
                r.request_id.clear();
                r
            },
            error_code::INVALID_REQUEST,
        ),
        (
            {
                let mut r = request(method::READ, json!({}));
                r.timeout_ms = 0;
                r
            },
            error_code::INVALID_REQUEST,
        ),
    ];
    for (bad, expected_code) in cases {
        let error = validate_request(&bad).expect_err("request must be rejected");
        assert_eq!(error.code(), expected_code, "method={}", bad.method);
        // A rejection is renderable as a normal protocol error DTO.
        let response = SidecarResponse::failure(&bad.request_id, error.to_dto());
        // A validation response must itself be well formed. When the
        // offending request had no id, use the protocol's rejection id.
        let response = if response.request_id.is_empty() {
            SidecarResponse::failure("r-validation", error.to_dto())
        } else {
            response
        };
        let line = encode_response(&response).expect("error DTO must encode");
        assert!(line.contains(expected_code));
    }
}

#[test]
fn error_dto_redacts_credentials_in_message_and_details() {
    let reply = SidecarResponse::failure(
        "r-contract-1",
        SidecarError {
            code: "internal_error".to_string(),
            message: "request failed: Bearer abc.def".to_string(),
            details: json!({"env": "AI_API_KEY=sk-secret", "nested": ["password=hunter2"]}),
            retryable: false,
        },
    );
    let line = encode_response(&reply).unwrap();
    assert!(line.contains("[REDACTED]"));
    for secret in ["abc.def", "sk-secret", "hunter2", "Bearer"] {
        assert!(!line.contains(secret), "leaked {secret} in {line}");
    }
    let decoded = decode_response(&line).unwrap();
    let error = decoded.error.expect("error DTO");
    assert_eq!(error.message, "[REDACTED]");
    assert_eq!(error.details["env"], json!("[REDACTED]"));
}

#[test]
fn malformed_lines_never_decode_into_a_request_or_response() {
    for line in [
        "",
        "not json",
        "[]",
        "null",
        "{\"protocol_version\":\"1.0\"}\n{\"x\":1}",
        "{\"protocol_version\":\"1.0\",\"request_id\":\"r\",\"method\":\"browser.read\",\"grants\":[],\"timeout_ms\":1}\r",
    ] {
        assert!(decode_request(line).is_err(), "{line:?}");
        assert!(decode_response(line).is_err(), "{line:?}");
    }
}
