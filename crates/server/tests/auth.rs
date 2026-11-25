//! Registration and password sign-in through the real router (#156; docs/server.md, Accounts
//! and sign-in).

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
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

/// Sends one request through the real router and returns the raw pieces.
async fn send(pool: &SqlitePool, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = iwbtg_server::app(pool.clone(), None)
        .oneshot(request)
        .await
        .expect("router must answer");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body")
        .to_vec();
    (status, headers, bytes)
}

async fn json_post(pool: &SqlitePool, uri: &str, body: Value) -> (StatusCode, HeaderMap, Value) {
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let (status, headers, bytes) = send(pool, request).await;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("a JSON body")
    };
    (status, headers, body)
}

async fn post(pool: &SqlitePool, uri: &str, body: Value) -> (StatusCode, Value) {
    let (status, _headers, body) = json_post(pool, uri, body).await;
    (status, body)
}

async fn get_page(pool: &SqlitePool, uri: &str) -> (StatusCode, HeaderMap, String) {
    let (status, headers, bytes) =
        send(pool, Request::get(uri).body(Body::empty()).expect("a request")).await;
    (status, headers, String::from_utf8(bytes).expect("HTML"))
}

async fn form_post(
    pool: &SqlitePool,
    uri: &str,
    fields: &[(&str, &str)],
    cookie: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
    let body = serde_urlencoded::to_string(fields).expect("a form body");
    let mut request = Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    let (status, headers, bytes) = send(pool, request.body(Body::from(body)).unwrap()).await;
    (status, headers, String::from_utf8(bytes).expect("HTML"))
}

/// The value of one `Set-Cookie` header, without its attributes.
fn set_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            let (key, rest) = value.split_once('=')?;
            (key == name).then(|| rest.split(';').next().unwrap_or_default().to_owned())
        })
}

/// The hidden CSRF field the form page rendered.
fn hidden_csrf(html: &str) -> String {
    let marker = "name=\"csrf\" value=\"";
    let start = html.find(marker).expect("a csrf field") + marker.len();
    let rest = &html[start..];
    rest[..rest.find('"').expect("the closing quote")].to_owned()
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

/// Each sign-in is a fresh opaque session: a new token, only its hash stored, and the documented
/// `__Host-` cookie attributes (docs/server.md, Accounts and sign-in; #157).
#[tokio::test]
async fn a_sign_in_creates_a_new_hashed_session_and_sets_the_host_cookie() {
    let (_dir, pool) = pool("session").await;
    register(&pool, "Foo Bar", None, PASSWORD).await;

    let (status, first_headers, _) = json_post(
        &pool,
        "/auth/login",
        json!({ "login": "Foo Bar", "password": PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let first_token = set_cookie(&first_headers, "__Host-iwbtg_session").expect("a session cookie");
    assert_eq!(first_token.len(), 64, "the token is 32 bytes of hex");
    let raw_cookie = first_headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("__Host-iwbtg_session="))
        .expect("the session Set-Cookie");
    for attribute in ["Path=/", "HttpOnly", "Secure", "SameSite=Lax", "Max-Age=2592000"] {
        assert!(
            raw_cookie.contains(attribute),
            "missing {attribute}: {raw_cookie}"
        );
    }

    let (_, second_headers, _) = json_post(
        &pool,
        "/auth/login",
        json!({ "login": "Foo Bar", "password": PASSWORD }),
    )
    .await;
    let second_token = set_cookie(&second_headers, "__Host-iwbtg_session").expect("a second session cookie");
    assert_ne!(first_token, second_token, "a sign-in reused a token");

    assert_eq!(count(&pool, "SELECT COUNT(*) FROM sessions").await, 2);
    let hashes: Vec<Vec<u8>> = sqlx::query_scalar("SELECT token_hash FROM sessions")
        .fetch_all(&pool)
        .await
        .expect("read the token hashes");
    assert_eq!(hashes.len(), 2);
    for hash in hashes {
        assert_eq!(hash.len(), 32, "not a SHA-256");
        assert_ne!(hash, first_token.as_bytes(), "the token itself was stored");
    }
}

/// The form pages set the double-submit cookie and embed the same token.
#[tokio::test]
async fn form_pages_set_a_csrf_cookie_and_embed_the_token() {
    let (_dir, pool) = pool("csrf-pages").await;

    let (status, headers, html) = get_page(&pool, "/login").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers
            .get(header::CONTENT_TYPE)
            .expect("a content type")
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    let login_csrf = set_cookie(&headers, "__Host-iwbtg_csrf").expect("a csrf cookie");
    assert_eq!(hidden_csrf(&html), login_csrf);
    assert!(html.contains("autocomplete=\"current-password\""));
    assert!(html.contains("name=\"csrf\""));

    let (_, headers, html) = get_page(&pool, "/register").await;
    let register_csrf = set_cookie(&headers, "__Host-iwbtg_csrf").expect("a csrf cookie");
    assert_eq!(hidden_csrf(&html), register_csrf);
    assert!(html.contains("autocomplete=\"new-password\""));
    assert_ne!(login_csrf, register_csrf, "a token was reused across pages");
}

/// A form without the matching cookie/token pair never reaches the credential check.
#[tokio::test]
async fn a_form_post_without_the_csrf_pair_is_forbidden_and_changes_nothing() {
    let (_dir, pool) = pool("csrf-reject").await;
    register(&pool, "Foo Bar", None, PASSWORD).await;

    let (_, _headers, html) = get_page(&pool, "/login").await;
    let csrf = hidden_csrf(&html);
    let cookie = format!("__Host-iwbtg_csrf={csrf}");

    // No cookie at all.
    let (status, _, _) = form_post(
        &pool,
        "/login",
        &[("login", "Foo Bar"), ("password", PASSWORD), ("csrf", &csrf)],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A cookie, but a different hidden token.
    let (status, _, html) = form_post(
        &pool,
        "/login",
        &[("login", "Foo Bar"), ("password", PASSWORD), ("csrf", "00")],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(html.contains("form expired"));

    assert_eq!(count(&pool, "SELECT COUNT(*) FROM sessions").await, 0);
}

/// The happy path: a CSRF-checked form sign-in creates a session and shows who it is.
#[tokio::test]
async fn a_form_sign_in_with_csrf_creates_a_session() {
    let (_dir, pool) = pool("form-login").await;
    register(&pool, "Foo Bar", None, PASSWORD).await;

    let (_, _headers, html) = get_page(&pool, "/login").await;
    let csrf = hidden_csrf(&html);
    let cookie = format!("__Host-iwbtg_csrf={csrf}");

    let (status, headers, html) = form_post(
        &pool,
        "/login",
        &[("login", "Foo Bar"), ("password", PASSWORD), ("csrf", &csrf)],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Signed in as Foo Bar."), "{html}");
    assert!(set_cookie(&headers, "__Host-iwbtg_session").is_some());
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM sessions").await, 1);
}

/// The register form goes through the same rules and creates the account, without a session.
#[tokio::test]
async fn a_form_registration_with_csrf_creates_the_account() {
    let (_dir, pool) = pool("form-register").await;

    let (_, _headers, html) = get_page(&pool, "/register").await;
    let csrf = hidden_csrf(&html);
    let cookie = format!("__Host-iwbtg_csrf={csrf}");

    let (status, _, html) = form_post(
        &pool,
        "/register",
        &[
            ("username", "Form Kid"),
            ("email", "Form@Example.com"),
            ("password", PASSWORD),
            ("csrf", &csrf),
        ],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Account created, Form Kid."), "{html}");

    let (canonical, email): (String, Option<String>) =
        sqlx::query_as("SELECT username_canonical, email FROM accounts")
            .fetch_one(&pool)
            .await
            .expect("read the account");
    assert_eq!(canonical, "form_kid");
    assert_eq!(email.as_deref(), Some("form@example.com"));
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM sessions").await, 0);
}

/// The form path reports the same validation answers as the JSON one, as HTML.
#[tokio::test]
async fn a_form_error_re_renders_the_form_with_the_message() {
    let (_dir, pool) = pool("form-error").await;

    let (_, _headers, html) = get_page(&pool, "/register").await;
    let csrf = hidden_csrf(&html);
    let cookie = format!("__Host-iwbtg_csrf={csrf}");

    let (status, _, html) = form_post(
        &pool,
        "/register",
        &[("username", "ab"), ("password", PASSWORD), ("csrf", &csrf)],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(html.contains("username must be 3 to 15 characters"), "{html}");
    // The re-rendered form still carries a usable token and its cookie.
    assert!(html.contains("name=\"csrf\""));
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM accounts").await, 0);
}
