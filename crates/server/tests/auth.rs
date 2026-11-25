//! Registration and password sign-in through the real router (#156; docs/server.md, Accounts
//! and sign-in).

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

use iwbtg_server::db;

use crate::support::TempDir;

/// Exactly 15 characters: the documented floor without a breach list.
const PASSWORD: &str = "fifteen chars!!";

/// A real migrated file database: registration writes rows, so the in-memory schema-less pool
/// the other tests use is not enough.
async fn pool(tag: &str) -> (TempDir, SqlitePool) {
    let dir = TempDir::new(tag);
    let path = dir.path().join("iwbtg.db");
    let pool = db::connect(&path).await.expect("open and migrate the database");
    (dir, pool)
}

async fn post(pool: &SqlitePool, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = iwbtg_server::app(pool.clone(), None)
        .oneshot(
            Request::post(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .expect("router must answer");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body")
        .to_vec();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("a JSON body")
    };
    (status, body)
}

async fn register(
    pool: &SqlitePool,
    username: &str,
    email: Option<&str>,
    password: &str,
) -> (StatusCode, Value) {
    let mut body = json!({ "username": username, "password": password });
    if let Some(email) = email {
        body["email"] = json!(email);
    }
    post(pool, "/auth/register", body).await
}

async fn sign_in(pool: &SqlitePool, login: &str, password: &str) -> (StatusCode, Value) {
    post(
        pool,
        "/auth/login",
        json!({ "login": login, "password": password }),
    )
    .await
}

async fn count(pool: &SqlitePool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.expect("count rows")
}

#[tokio::test]
async fn register_creates_the_account_and_signs_in_by_name_canonical_or_email() {
    let (_dir, pool) = pool("register").await;

    let (status, body) = register(&pool, "Foo Bar", Some("Foo@Example.com"), PASSWORD).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["username"], "Foo Bar");
    assert!(body["id"].as_i64().expect("an id") > 0);

    // Stored as typed, canonical and with the address lowercased (docs/server.md).
    let (canonical, email): (String, Option<String>) =
        sqlx::query_as("SELECT username_canonical, email FROM accounts")
            .fetch_one(&pool)
            .await
            .expect("read the account");
    assert_eq!(canonical, "foo_bar");
    assert_eq!(email.as_deref(), Some("foo@example.com"));

    for login in ["Foo Bar", "foo_bar", "foo@example.com"] {
        let (status, body) = sign_in(&pool, login, PASSWORD).await;
        assert_eq!(status, StatusCode::OK, "signing in as {login}");
        assert_eq!(body["username"], "Foo Bar");
    }

    // The request field may also be called `username`, as the native client's form will.
    let (status, _) = post(
        &pool,
        "/auth/login",
        json!({ "username": "Foo Bar", "password": PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn the_password_is_argon2id_at_the_documented_parameters() {
    let (_dir, pool) = pool("argon2").await;
    register(&pool, "Kid", None, PASSWORD).await;

    let hash: String = sqlx::query_scalar("SELECT password_hash FROM password_credentials")
        .fetch_one(&pool)
        .await
        .expect("read the hash");
    // PHC string: algorithm, version, and m=19456 KiB, t=2, p=1 (docs/server.md).
    assert!(
        hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
        "unexpected PHC string: {hash}"
    );
    assert!(!hash.contains(PASSWORD), "the password leaked into the hash");
}

#[tokio::test]
async fn canonical_usernames_are_unique() {
    let (_dir, pool) = pool("canonical").await;

    let (status, _) = register(&pool, "Foo Bar", None, PASSWORD).await;
    assert_eq!(status, StatusCode::CREATED);

    // Same canonical form, different typing: one account only.
    let (status, _) = register(&pool, "foo_bar", None, PASSWORD).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM accounts").await, 1);
}

#[tokio::test]
async fn emails_are_unique_case_insensitively() {
    let (_dir, pool) = pool("email").await;

    let (status, _) = register(&pool, "One", Some("A@Example.com"), PASSWORD).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = register(&pool, "Two", Some("a@example.com"), PASSWORD).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM accounts").await, 1);
}

/// A duplicate username and a duplicate email must not be distinguishable from the answer.
#[tokio::test]
async fn both_collisions_share_one_message() {
    let (_dir, pool) = pool("collisions").await;
    register(&pool, "Taken", Some("taken@example.com"), PASSWORD).await;

    let (username_status, username_body) = register(&pool, "Taken", None, PASSWORD).await;
    let (email_status, email_body) = register(&pool, "Other", Some("taken@example.com"), PASSWORD).await;

    assert_eq!(username_status, StatusCode::CONFLICT);
    assert_eq!(email_status, StatusCode::CONFLICT);
    assert_eq!(username_body, email_body);
    assert!(username_body["error"].as_str().is_some());
}

#[tokio::test]
async fn usernames_follow_the_documented_rules() {
    let (_dir, pool) = pool("usernames").await;

    for invalid in [
        "ab",                        // too short
        "this name is way too long", // too long
        "bad!",                      // punctuation outside the allowed set
        "a  b",                      // doubled spaces
        "tab\tname",                 // whitespace that is not a space
        "José",                      // not ASCII letters/digits
    ] {
        let (status, body) = register(&pool, invalid, None, PASSWORD).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "accepted {invalid:?}");
        assert!(body["error"].as_str().is_some(), "no error for {invalid:?}");
    }

    // Surrounding spaces are trimmed before the name is stored.
    let (status, body) = register(&pool, "  Trimmed Name  ", None, PASSWORD).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["username"], "Trimmed Name");
}

#[tokio::test]
async fn passwords_need_fifteen_characters_and_are_capped() {
    let (_dir, pool) = pool("passwords").await;

    let (status, _) = register(&pool, "Fourteen", None, "fourteen chars").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = register(&pool, "Fifteen", None, "fifteen chars!!").await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = register(&pool, "TooLong", None, &"x".repeat(1025)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Login must not say whether it was the name or the password that was wrong, and the two cases
/// answer with the same body.
#[tokio::test]
async fn a_wrong_password_and_an_unknown_account_get_the_same_401() {
    let (_dir, pool) = pool("login401").await;
    register(&pool, "Known", None, PASSWORD).await;

    let (wrong_status, wrong_body) = sign_in(&pool, "Known", "wrong password!!").await;
    let (unknown_status, unknown_body) = sign_in(&pool, "Nobody", PASSWORD).await;

    assert_eq!(wrong_status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown_status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong_body, unknown_body);
}

#[tokio::test]
async fn email_is_optional_and_missing_addresses_do_not_collide() {
    let (_dir, pool) = pool("no-email").await;

    let (status, _) = register(&pool, "One", None, PASSWORD).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = register(&pool, "Two", Some("   "), PASSWORD).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM accounts").await, 2);
}

#[tokio::test]
async fn invalid_emails_are_rejected() {
    let (_dir, pool) = pool("bad-email").await;

    for invalid in [
        "no-at-sign",
        "foo@",
        "@example.com",
        "foo@example",
        "foo@@example.com",
    ] {
        let (status, _) = register(&pool, "Kid", Some(invalid), PASSWORD).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "accepted {invalid:?}");
    }
}
