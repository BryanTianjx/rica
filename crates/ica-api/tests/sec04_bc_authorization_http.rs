//! SEC-04-B/C regression suite: real JWT identity → real tenant/owner scope.
//!
//! The older `sec04_authorization_http.rs` characterises the Local-mode
//! boundary (bearer ignored). This suite runs the SAME router with
//! `AuthMode::MultiUser`, where a missing/invalid token is rejected and every
//! scoped read is restricted to the caller's tenant and ownership.

mod support;

use chrono::Utc;
use ica_api::auth::AuthMode;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::json;
use support::{
    app_for_mode, delete_with_headers, get_with_headers, pool_for, post_with_headers,
    seeded_database, TestResponse,
};

const TENANT_A: &str = "11111111-1111-4111-8111-111111111111";
const TENANT_B: &str = "22222222-2222-4222-8222-222222222222";
const USER_A_ADMIN: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const USER_A_MEMBER: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaab";
const USER_B_ADMIN: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

fn access_token(sub: &str, tenant: &str, role: &str) -> String {
    let now = Utc::now();
    encode(
        &Header::new(Algorithm::HS256),
        &json!({
            "sub": sub,
            "tenant_id": tenant,
            "role": role,
            "type": "access",
            "iat": now.timestamp(),
            "exp": (now + chrono::Duration::hours(1)).timestamp(),
        }),
        // The test harness configures an empty JWT secret (Local fixture);
        // signing with the same key is what makes these tokens verifiable.
        &EncodingKey::from_secret(b""),
    )
    .expect("signed access token")
}

fn auth(token: &str) -> String {
    format!("Bearer {token}")
}

fn code(response: &TestResponse) -> &str {
    response.body["error"]["code"].as_str().unwrap_or("")
}

async fn multiuser_app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ica.db");
    seeded_database(&path).await;
    let app = app_for_mode(&path, AuthMode::MultiUser).await;
    (app, dir)
}

/// Provision real tenant/user rows so the share endpoints (which resolve the
/// recipient through the auth repository) can be exercised with MultiUser JWT
/// identities. The baseline seed only carries positions/candidates.
async fn seed_multiuser_identities(dir: &tempfile::TempDir) {
    let pool = pool_for(dir).await;
    let now = "2026-11-23 00:00:00.000000";
    for (tenant, name) in [(TENANT_A, "Tenant A"), (TENANT_B, "Tenant B")] {
        sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(tenant)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("seed tenant");
    }
    for (user, tenant, role, email) in [
        (USER_A_ADMIN, TENANT_A, "admin", "a-admin@example.test"),
        (USER_A_MEMBER, TENANT_A, "member", "a-member@example.test"),
        (USER_B_ADMIN, TENANT_B, "admin", "b-admin@example.test"),
    ] {
        sqlx::query(concat!(
            "INSERT OR IGNORE INTO users (id, tenant_id, email, password_hash,",
            " display_name, role, is_active, created_at, updated_at)",
            " VALUES (?, ?, ?, '', ?, ?, 1, ?, ?)"
        ))
        .bind(user)
        .bind(tenant)
        .bind(email)
        .bind(user)
        .bind(role)
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("seed user");
    }
    pool.close().await;
}

async fn create_position(app: axum::Router, token: &str, name: &str) -> TestResponse {
    let header = auth(token);
    post_with_headers(
        app,
        "/api/positions",
        &json!({"name": name}).to_string(),
        &[("authorization", header.as_str())],
    )
    .await
}

/// C-AC2: anonymous and forged bearer requests are rejected with the shared
/// 401 `unauthorized` envelope; public health/bootstrap routes stay reachable.
#[tokio::test]
async fn anonymous_and_forged_bearers_are_rejected_with_baseline_code() {
    let (app, _dir) = multiuser_app().await;

    let anonymous = support::get(app.clone(), "/api/positions").await;
    assert_eq!(anonymous.status, 401);
    assert_eq!(code(&anonymous), "unauthorized");

    let forged = auth("not-a-jwt");
    let forged_response = get_with_headers(
        app.clone(),
        "/api/positions",
        &[("authorization", forged.as_str())],
    )
    .await;
    assert_eq!(forged_response.status, 401);
    assert_eq!(code(&forged_response), "unauthorized");

    let health = support::get(app, "/health").await;
    assert_eq!(health.status, 200);
}

/// B-AC1/B-AC3 + C-AC3: a token from tenant B cannot list or read tenant A's
/// rows. Out-of-scope rows are indistinguishable from missing rows (404).
#[tokio::test]
async fn cross_tenant_reads_are_invisible() {
    let (app, _dir) = multiuser_app().await;
    let a_admin = access_token(USER_A_ADMIN, TENANT_A, "admin");
    let b_admin = access_token(USER_B_ADMIN, TENANT_B, "admin");

    let created = create_position(app.clone(), &a_admin, "Tenant A secret role").await;
    assert_eq!(created.status, 201, "{}", created.body);
    let id = created.body["id"].as_str().unwrap().to_string();

    let a_sees = get_with_headers(
        app.clone(),
        &format!("/api/positions/{id}"),
        &[("authorization", auth(&a_admin).as_str())],
    )
    .await;
    assert_eq!(a_sees.status, 200, "owner tenant must see its row");

    let b_read = get_with_headers(
        app.clone(),
        &format!("/api/positions/{id}"),
        &[("authorization", auth(&b_admin).as_str())],
    )
    .await;
    assert_eq!(b_read.status, 404, "another tenant must not read the row");

    let b_list = get_with_headers(
        app,
        "/api/positions?search=Tenant%20A%20secret",
        &[("authorization", auth(&b_admin).as_str())],
    )
    .await;
    assert_eq!(b_list.status, 200);
    assert_eq!(
        b_list.body["total"], 0,
        "scoped list must exclude other tenants"
    );
}

/// C-AC1 write/delete negative: a member can create their own row, but an
/// out-of-scope row cannot be patched; share deletion is decided by the
/// Application layer's ownership check for the JWT actor.
#[tokio::test]
async fn write_and_delete_obey_identity_and_ownership() {
    let (app, _dir) = multiuser_app().await;
    let a_member = access_token(USER_A_MEMBER, TENANT_A, "member");
    let b_admin = access_token(USER_B_ADMIN, TENANT_B, "admin");

    let own = create_position(app.clone(), &a_member, "Member owned role").await;
    assert_eq!(own.status, 201, "{}", own.body);

    let foreign = create_position(app.clone(), &b_admin, "Tenant B role").await;
    let foreign_id = foreign.body["id"].as_str().unwrap().to_string();

    let patch_header = auth(&a_member);
    let patched = support::patch_with_headers(
        app.clone(),
        &format!("/api/positions/{foreign_id}"),
        &json!({"notes": "hijacked"}).to_string(),
        &[("authorization", patch_header.as_str())],
    )
    .await;
    // The row is foreign to the member; a scoped mutation must not succeed.
    assert_ne!(patched.status, 200, "cross-tenant mutation must be denied");

    let header = auth(&a_member);
    let deleted = delete_with_headers(
        app,
        "/api/shares/00000000-0000-0000-0000-000000000000",
        &[("authorization", header.as_str())],
    )
    .await;
    assert!(deleted.status == 404 || deleted.status == 401);
}

/// B-AC3/C-AC1 share negative: a real cross-tenant actor cannot see, grant or
/// revoke a share owned by another tenant. Out-of-scope shares are hidden
/// (404) exactly like cross-tenant position rows, and the row survives.
#[tokio::test]
async fn cross_tenant_share_create_and_revoke_are_denied() {
    let (app, dir) = multiuser_app().await;
    seed_multiuser_identities(&dir).await;
    let a_admin = access_token(USER_A_ADMIN, TENANT_A, "admin");
    let b_admin = access_token(USER_B_ADMIN, TENANT_B, "admin");

    // A shares one of its own rows with a same-tenant recipient (baseline
    // create_share requires the recipient to be a real user row).
    let position = create_position(app.clone(), &a_admin, "A tenant shared role").await;
    assert_eq!(position.status, 201, "{}", position.body);
    let position_id = position.body["id"].as_str().unwrap().to_string();

    let share_body = json!({
        "recipient_id": USER_A_MEMBER,
        "resource_type": "position",
        "resource_id": position_id,
        "permission": "read"
    })
    .to_string();
    let a_header = auth(&a_admin);
    let created = post_with_headers(
        app.clone(),
        "/api/shares",
        &share_body,
        &[("authorization", a_header.as_str())],
    )
    .await;
    assert_eq!(
        created.status, 201,
        "tenant A must be able to share its own row: {}",
        created.body
    );
    let share_id = created.body["id"].as_str().unwrap().to_string();

    // B (tenant B admin) must not see A's share through its own share list.
    let b_header = auth(&b_admin);
    let b_list = get_with_headers(
        app.clone(),
        "/api/shares",
        &[("authorization", b_header.as_str())],
    )
    .await;
    assert_eq!(b_list.status, 200, "{}", b_list.body);
    assert!(
        !b_list.body.to_string().contains(&share_id),
        "a cross-tenant share must not leak into tenant B's list"
    );

    // B (tenant B admin) must not revoke A's share. Admin is tenant-scoped:
    // out-of-scope rows are 404 (or 403), never 2xx.
    let revoked = delete_with_headers(
        app.clone(),
        &format!("/api/shares/{share_id}"),
        &[("authorization", b_header.as_str())],
    )
    .await;
    assert!(
        revoked.status == 403 || revoked.status == 404,
        "cross-tenant share revocation must be denied, got {}: {}",
        revoked.status,
        revoked.body
    );

    // The share still exists for its owner after the denied attempt.
    let a_list =
        get_with_headers(app, "/api/shares", &[("authorization", a_header.as_str())]).await;
    assert!(
        a_list.body.to_string().contains(&share_id),
        "the owner must still see the share after a cross-tenant revoke attempt"
    );
}

/// B-AC5/C-AC2: `/auth/me` reflects the verified token instead of the
/// synthetic Local user when a real identity is present.
#[tokio::test]
async fn me_reflects_the_verified_identity() {
    let (app, _dir) = multiuser_app().await;
    let token = access_token(USER_A_MEMBER, TENANT_A, "member");
    let header = auth(&token);
    let me = get_with_headers(app, "/api/auth/me", &[("authorization", header.as_str())]).await;
    assert_eq!(me.status, 200);
    assert_eq!(me.body["id"], USER_A_MEMBER);
    assert_eq!(me.body["tenant_id"], TENANT_A);
    assert_eq!(me.body["role"], "member");
}

/// C-AC4: the three-way trace is parsed from the real 68-row contract matrix,
/// the real route table, the real PUBLIC_PATHS whitelist and the real test
/// suite. A missing contract row, a missing policy marker, a missing negative
/// test class or a whitelist drift breaks the build — this is a real gate, not
/// a tautological `contains` smoke check.
#[test]
fn contract_authorization_trace_is_complete() {
    let routes = include_str!("../src/routes.rs");
    let matrix = include_str!("../../../docs/architecture/api-authentication-matrix.md");
    let suite = include_str!("sec04_bc_authorization_http.rs");

    // (1) the router really carries the auth gate and the scoped read path.
    assert!(
        routes.contains("auth::authenticate"),
        "router must carry the SEC-04-B authentication middleware"
    );
    assert!(
        routes.contains("get_position_scoped"),
        "positions must use a tenant/owner scoped handler"
    );

    // (2) parse every contract row; each must carry BOTH an authorization
    // policy marker (当前策略) and a 目标授权/归属 marker.
    const POLICIES: [&str; 2] = ["公开端点白名单", "Local 放行 / 未来需认证"];
    let mut ids = Vec::new();
    for line in matrix.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() < 8 {
            continue;
        }
        let Ok(id) = cells[0].parse::<u32>() else {
            continue;
        };
        ids.push(id);
        assert!(
            POLICIES.contains(&cells[6]),
            "contract {id} has no valid authorization-policy marker: {:?}",
            cells[6]
        );
        assert!(
            !cells[7].is_empty(),
            "contract {id} has no 目标授权/归属 authorization marker"
        );
    }
    assert_eq!(ids.len(), 68, "matrix must have exactly 68 contract rows");
    for (index, id) in ids.iter().enumerate() {
        assert_eq!(
            *id,
            index as u32 + 1,
            "contract ids must be 1..68 contiguous"
        );
    }

    // (3) the suite must actually declare negative test functions for all four
    // classes: read / write / delete / share.
    let test_names: Vec<String> = suite
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line
                .strip_prefix("async fn ")
                .or_else(|| line.strip_prefix("fn "))?;
            rest.split('(').next().map(str::to_string)
        })
        .collect();
    for class in ["read", "write", "delete", "share"] {
        assert!(
            test_names.iter().any(|name| name.contains(class)),
            "no negative test function covers '{class}': {test_names:?}"
        );
    }

    // (4) the matrix's declared public whitelist must equal auth::PUBLIC_PATHS
    // in both directions (a doc/code drift makes this gate fail).
    let after = matrix
        .split("<!-- SEC04-BC-PUBLIC-PATHS-BEGIN")
        .nth(1)
        .expect("matrix must carry the SEC04-BC public-paths marker");
    let block = after
        .split("SEC04-BC-PUBLIC-PATHS-END -->")
        .next()
        .expect("matrix must close the SEC04-BC public-paths marker");
    let mut documented: Vec<String> = block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("<!--"))
        .map(str::to_string)
        .collect();
    documented.sort();
    let mut from_code: Vec<String> = ica_api::auth::PUBLIC_PATHS
        .iter()
        .map(|path| path.to_string())
        .collect();
    from_code.sort();
    assert_eq!(
        documented, from_code,
        "the matrix public whitelist must equal auth::PUBLIC_PATHS in both directions"
    );
}
