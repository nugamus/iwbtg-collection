//! Password reset by emailed code, end to end (#160; docs/server.md, Accounts and sign-in).
//!
//! The mailer points at a local SMTP listener, so the test sees the real message the server
//! would send and extracts the six-digit code from it.

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tower::ServiceExt;

use iwbtg_server::db;
use iwbtg_server::mail::Mailer;

use crate::support::TempDir;

const OLD_PASSWORD: &str = "fifteen chars!!";
const NEW_PASSWORD: &str = "sixteen chars!!!";
const EMAIL: &str = "kid@example.com";

/// The smallest SMTP server that accepts one message per connection and reports its body.
async fn mock_smtp() -> (SocketAddr, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the mock SMTP listener");
    let addr = listener.local_addr().expect("mock SMTP address");
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let tx = tx.clone();
            tokio::spawn(async move { serve_smtp(socket, tx).await });
        }
    });
    (addr, rx)
}

async fn serve_smtp(socket: TcpStream, tx: mpsc::Sender<String>) {
    let (read_half, mut write_half) = socket.into_split();
    let mut lines = BufReader::new(read_half).lines();
    let _ = write_half.write_all(b"220 mock ESMTP\r\n").await;
    while let Ok(Some(line)) = lines.next_line().await {
        let command = line.trim_end();
        if command.starts_with("EHLO") || command.starts_with("HELO") {
            // No STARTTLS and no AUTH advertised: the test stays on plain SMTP.
            let _ = write_half.write_all(b"250-mock\r\n250 OK\r\n").await;
        } else if command.starts_with("MAIL FROM") || command.starts_with("RCPT TO") {
            let _ = write_half.write_all(b"250 OK\r\n").await;
        } else if command == "DATA" {
            let _ = write_half.write_all(b"354 go ahead\r\n").await;
            let mut body = String::new();
            while let Ok(Some(data)) = lines.next_line().await {
                if data == "." {
                    break;
                }
                body.push_str(&data);
                body.push('\n');
            }
            let _ = tx.send(body).await;
            let _ = write_half.write_all(b"250 queued\r\n").await;
        } else if command == "QUIT" {
            let _ = write_half.write_all(b"221 bye\r\n").await;
            break;
        } else {
            let _ = write_half.write_all(b"250 OK\r\n").await;
        }
    }
}

struct Fixture {
    _dir: TempDir,
    pool: SqlitePool,
    router: Router,
    emails: mpsc::Receiver<String>,
}

async fn fixture(tag: &str) -> Fixture {
    let dir = TempDir::new(tag);
    let pool = db::connect(&dir.path().join("iwbtg.db"))
        .await
        .expect("open and migrate the database");
    let (addr, emails) = mock_smtp().await;
    let mailer = Mailer::from_url(&format!("smtp://{addr}"), "noreply@example.com").expect("a mock mailer");
    let router = iwbtg_server::app_with_mailer(pool.clone(), None, mailer);
    Fixture {
        _dir: dir,
        pool,
        router,
        emails,
    }
}

async fn post(router: &Router, uri: &str, ip: &str, body: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::post(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .header("cf-connecting-ip", ip)
                .body(Body::from(body.to_string()))
                .expect("a request"),
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

async fn get_page(router: &Router, uri: &str) -> (StatusCode, HeaderMap, String) {
    let response = router
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).expect("a request"))
        .await
        .expect("router must answer");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body")
        .to_vec();
    (status, headers, String::from_utf8(bytes).expect("HTML"))
}

async fn form_post(
    router: &Router,
    uri: &str,
    ip: &str,
    fields: &[(&str, &str)],
    cookie: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
    let body = serde_urlencoded::to_string(fields).expect("a form body");
    let mut request = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("cf-connecting-ip", ip);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body)).expect("a request"))
        .await
        .expect("router must answer");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body")
        .to_vec();
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

/// The hidden CSRF field a reset page rendered.
fn hidden_csrf(html: &str) -> String {
    let marker = "name=\"csrf\" value=\"";
    let start = html.find(marker).expect("a csrf field") + marker.len();
    let rest = &html[start..];
    rest[..rest.find('"').expect("the closing quote")].to_owned()
}

/// Opens a reset page and returns the hidden token with the matching `Cookie` header value.
async fn reset_form_token(router: &Router, uri: &str) -> (String, String) {
    let (status, headers, html) = get_page(router, uri).await;
    assert_eq!(status, StatusCode::OK);
    let token = set_cookie(&headers, "__Host-iwbtg_csrf").expect("a csrf cookie");
    assert_eq!(hidden_csrf(&html), token, "the page's two token halves disagree");
    (token.clone(), format!("__Host-iwbtg_csrf={token}"))
}

async fn register(fixture: &Fixture) {
    let (status, _) = post(
        &fixture.router,
        "/auth/register",
        "203.0.113.20",
        json!({ "username": "Reset Kid", "email": EMAIL, "password": OLD_PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn request_reset(fixture: &Fixture, ip: &str, email: &str) -> (StatusCode, Value) {
    post(
        &fixture.router,
        "/auth/reset/request",
        ip,
        json!({ "email": email }),
    )
    .await
}

async fn confirm(
    fixture: &Fixture,
    ip: &str,
    email: &str,
    code: &str,
    password: &str,
) -> (StatusCode, Value) {
    post(
        &fixture.router,
        "/auth/reset/confirm",
        ip,
        json!({ "email": email, "code": code, "password": password }),
    )
    .await
}

async fn login(fixture: &Fixture, ip: &str, password: &str) -> StatusCode {
    post(
        &fixture.router,
        "/auth/login",
        ip,
        json!({ "login": EMAIL, "password": password }),
    )
    .await
    .0
}

async fn count(pool: &SqlitePool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.expect("count rows")
}

/// The six-digit code is the only long digit run in the message body.
fn code_from(email: &str) -> String {
    let characters: Vec<char> = email.chars().collect();
    for window in characters.windows(6) {
        if window.iter().all(char::is_ascii_digit) {
            return window.iter().collect();
        }
    }
    panic!("no six-digit code in the email: {email}");
}

async fn next_email(fixture: &mut Fixture) -> String {
    tokio::time::timeout(Duration::from_secs(10), fixture.emails.recv())
        .await
        .expect("no email arrived")
        .expect("the mock SMTP listener is alive")
}

#[tokio::test]
async fn a_reset_changes_the_password_and_revokes_every_session() {
    let mut fixture = fixture("reset-happy").await;
    register(&fixture).await;
    assert_eq!(
        login(&fixture, "203.0.113.21", OLD_PASSWORD).await,
        StatusCode::OK
    );
    assert_eq!(count(&fixture.pool, "SELECT COUNT(*) FROM sessions").await, 1);

    let (status, body) = request_reset(&fixture, "203.0.113.22", EMAIL).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(body, Value::Null);

    let email = next_email(&mut fixture).await;
    let code = code_from(&email);
    assert_eq!(code.len(), 6);

    let (status, _) = confirm(&fixture, "203.0.113.23", EMAIL, &code, NEW_PASSWORD).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // The code is single-use, and the old sign-in no longer works.
    assert_eq!(
        count(&fixture.pool, "SELECT COUNT(*) FROM password_resets").await,
        0
    );
    assert_eq!(count(&fixture.pool, "SELECT COUNT(*) FROM sessions").await, 0);
    assert_eq!(
        login(&fixture, "203.0.113.24", OLD_PASSWORD).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&fixture, "203.0.113.25", NEW_PASSWORD).await,
        StatusCode::OK
    );
}

/// The whole point of the issue: an address with no account gets the same answer and no mail.
#[tokio::test]
async fn an_unknown_address_gets_the_same_answer_and_no_email() {
    let mut fixture = fixture("reset-unknown").await;
    register(&fixture).await;

    let (known_status, known_body) = request_reset(&fixture, "203.0.113.30", EMAIL).await;
    let _ = next_email(&mut fixture).await;

    let (unknown_status, unknown_body) = request_reset(&fixture, "203.0.113.30", "nobody@example.com").await;

    assert_eq!(known_status, unknown_status);
    assert_eq!(known_body, unknown_body);
    assert_eq!(known_status, StatusCode::NO_CONTENT);

    // No second message, within a generous window.
    assert!(
        tokio::time::timeout(Duration::from_millis(500), fixture.emails.recv())
            .await
            .is_err(),
        "an unknown address still produced an email"
    );
}

#[tokio::test]
async fn a_wrong_code_is_rejected_counted_and_capped() {
    let mut fixture = fixture("reset-wrong").await;
    register(&fixture).await;
    request_reset(&fixture, "203.0.113.40", EMAIL).await;
    let code = code_from(&next_email(&mut fixture).await);
    let wrong = if code == "000000" { "111111" } else { "000000" };

    for attempt in 1..=5 {
        let (status, body) = confirm(&fixture, "203.0.113.41", EMAIL, wrong, NEW_PASSWORD).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "attempt {attempt}");
        assert_eq!(body["error"], "invalid or expired code");
    }
    // Even the real code is refused once the attempt cap is reached.
    let (status, body) = confirm(&fixture, "203.0.113.42", EMAIL, &code, NEW_PASSWORD).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid or expired code");

    // An unknown address gets the same answer as a real one with a wrong code.
    let (unknown_status, unknown_body) = confirm(
        &fixture,
        "203.0.113.43",
        "nobody@example.com",
        wrong,
        NEW_PASSWORD,
    )
    .await;
    let (_, wrong_body) = confirm(&fixture, "203.0.113.44", EMAIL, wrong, NEW_PASSWORD).await;
    assert_eq!(unknown_status, StatusCode::BAD_REQUEST);
    assert_eq!(unknown_body, wrong_body);
}

#[tokio::test]
async fn an_expired_code_is_rejected() {
    let mut fixture = fixture("reset-expired").await;
    register(&fixture).await;
    request_reset(&fixture, "203.0.113.50", EMAIL).await;
    let code = code_from(&next_email(&mut fixture).await);

    sqlx::query("UPDATE password_resets SET expires_at = '2000-01-01T00:00:00.000Z'")
        .execute(&fixture.pool)
        .await
        .expect("expire the code");

    let (status, body) = confirm(&fixture, "203.0.113.51", EMAIL, &code, NEW_PASSWORD).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid or expired code");
    assert_eq!(
        login(&fixture, "203.0.113.52", NEW_PASSWORD).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn reset_requests_are_rate_limited_per_address() {
    let fixture = fixture("reset-limit").await;
    let ip = "203.0.113.60";

    // Unknown, distinct addresses: only the IP window is shared.
    for index in 0..5 {
        let (status, _) = request_reset(&fixture, ip, &format!("nobody {index}@example.com")).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "request {index}");
    }
    let (status, body) = request_reset(&fixture, ip, "nobody 5@example.com").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "too many requests; retry shortly");

    let (status, _) = request_reset(&fixture, "203.0.113.61", "nobody 6@example.com").await;
    assert_eq!(status, StatusCode::NO_CONTENT, "another address was limited");
}

/// Each reset page renders its form with a token in a hidden field and the same token in the
/// `__Host-iwbtg_csrf` cookie (#1545).
#[tokio::test]
async fn reset_pages_set_a_csrf_cookie_and_embed_the_token() {
    let fixture = fixture("reset-pages").await;

    let (status, headers, html) = get_page(&fixture.router, "/reset").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers
            .get(header::CONTENT_TYPE)
            .expect("a content type")
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    let request_token = set_cookie(&headers, "__Host-iwbtg_csrf").expect("a csrf cookie");
    assert_eq!(hidden_csrf(&html), request_token);
    assert!(html.contains("action=\"/reset\""), "{html}");
    assert!(html.contains("<label for=\"email\">Email</label>"), "{html}");
    assert!(html.contains("autocomplete=\"email\""), "{html}");
    assert!(html.contains(":focus-visible"), "no focus outline in the page");

    let (status, headers, html) = get_page(&fixture.router, "/reset/code").await;
    assert_eq!(status, StatusCode::OK);
    let code_token = set_cookie(&headers, "__Host-iwbtg_csrf").expect("a csrf cookie");
    assert_eq!(hidden_csrf(&html), code_token);
    assert_ne!(request_token, code_token, "a token was reused across pages");
    assert!(html.contains("action=\"/reset/confirm\""), "{html}");
    assert!(html.contains("autocomplete=\"one-time-code\""), "{html}");
    assert!(html.contains("autocomplete=\"new-password\""), "{html}");
    assert!(html.contains("autocomplete=\"email\""), "{html}");
    assert!(html.contains(":focus-visible"), "no focus outline in the page");
}

/// The request form answers the same neutral sentence for a known and an unknown address, and
/// only the known one gets mail — the form path keeps the #160 no-oracle rule.
#[tokio::test]
async fn a_form_reset_request_answers_the_same_for_any_address() {
    let mut fixture = fixture("reset-form-request").await;
    register(&fixture).await;

    let (csrf, cookie) = reset_form_token(&fixture.router, "/reset").await;
    let (known_status, _, known_html) = form_post(
        &fixture.router,
        "/reset",
        "198.51.100.10",
        &[("email", EMAIL), ("csrf", &csrf)],
        Some(&cookie),
    )
    .await;
    let confirmation = "If that address has an account, a reset code is on its way.";
    assert_eq!(known_status, StatusCode::OK);
    assert!(known_html.contains(confirmation), "{known_html}");
    let _ = next_email(&mut fixture).await;

    let (csrf, cookie) = reset_form_token(&fixture.router, "/reset").await;
    let (unknown_status, _, unknown_html) = form_post(
        &fixture.router,
        "/reset",
        "198.51.100.11",
        &[("email", "nobody@example.com"), ("csrf", &csrf)],
        Some(&cookie),
    )
    .await;
    assert_eq!(unknown_status, known_status);
    assert_eq!(unknown_html, known_html);
    assert!(
        tokio::time::timeout(Duration::from_millis(500), fixture.emails.recv())
            .await
            .is_err(),
        "the unknown address still produced an email"
    );
}

/// The code form goes through the same confirm body as the JSON endpoint and sets the password.
#[tokio::test]
async fn a_form_reset_confirm_changes_the_password() {
    let mut fixture = fixture("reset-form-confirm").await;
    register(&fixture).await;

    let (csrf, cookie) = reset_form_token(&fixture.router, "/reset").await;
    let (status, _, _) = form_post(
        &fixture.router,
        "/reset",
        "198.51.100.20",
        &[("email", EMAIL), ("csrf", &csrf)],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let code = code_from(&next_email(&mut fixture).await);

    let (csrf, cookie) = reset_form_token(&fixture.router, "/reset/code").await;
    let (status, _, html) = form_post(
        &fixture.router,
        "/reset/confirm",
        "198.51.100.21",
        &[
            ("email", EMAIL),
            ("code", &code),
            ("password", NEW_PASSWORD),
            ("csrf", &csrf),
        ],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Password updated."), "{html}");

    assert_eq!(
        login(&fixture, "198.51.100.22", NEW_PASSWORD).await,
        StatusCode::OK
    );
    assert_eq!(
        login(&fixture, "198.51.100.23", OLD_PASSWORD).await,
        StatusCode::UNAUTHORIZED
    );
}

/// A wrong and an expired code re-render the code form with the one generic message and a fresh
/// token, never saying which case it was.
#[tokio::test]
async fn a_bad_form_code_re_renders_the_generic_error_with_a_fresh_token() {
    let mut fixture = fixture("reset-form-wrong").await;
    register(&fixture).await;

    let (csrf, cookie) = reset_form_token(&fixture.router, "/reset").await;
    form_post(
        &fixture.router,
        "/reset",
        "198.51.100.30",
        &[("email", EMAIL), ("csrf", &csrf)],
        Some(&cookie),
    )
    .await;
    let code = code_from(&next_email(&mut fixture).await);
    let wrong = if code == "000000" { "111111" } else { "000000" };

    let (submitted, cookie) = reset_form_token(&fixture.router, "/reset/code").await;
    let (status, headers, html) = form_post(
        &fixture.router,
        "/reset/confirm",
        "198.51.100.31",
        &[
            ("email", EMAIL),
            ("code", wrong),
            ("password", NEW_PASSWORD),
            ("csrf", &submitted),
        ],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(html.contains("invalid or expired code"), "{html}");
    let fresh = set_cookie(&headers, "__Host-iwbtg_csrf").expect("a fresh csrf cookie");
    assert_ne!(fresh, submitted, "the error page reused the submitted token");
    assert_eq!(hidden_csrf(&html), fresh);

    sqlx::query("UPDATE password_resets SET expires_at = '2000-01-01T00:00:00.000Z'")
        .execute(&fixture.pool)
        .await
        .expect("expire the code");

    let (csrf, cookie) = reset_form_token(&fixture.router, "/reset/code").await;
    let (status, _, html) = form_post(
        &fixture.router,
        "/reset/confirm",
        "198.51.100.32",
        &[
            ("email", EMAIL),
            ("code", &code),
            ("password", NEW_PASSWORD),
            ("csrf", &csrf),
        ],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(html.contains("invalid or expired code"), "{html}");
    assert_eq!(
        login(&fixture, "198.51.100.33", NEW_PASSWORD).await,
        StatusCode::UNAUTHORIZED
    );
}

/// A form post whose hidden token and cookie disagree is `403` on both pages, before any code
/// is issued or checked.
#[tokio::test]
async fn reset_form_posts_without_the_csrf_pair_are_forbidden() {
    let mut fixture = fixture("reset-form-csrf").await;
    register(&fixture).await;

    let (csrf, cookie) = reset_form_token(&fixture.router, "/reset").await;

    // No cookie: no code is issued and no mail leaves.
    let (status, _, _) = form_post(
        &fixture.router,
        "/reset",
        "198.51.100.40",
        &[("email", EMAIL), ("csrf", &csrf)],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        count(&fixture.pool, "SELECT COUNT(*) FROM password_resets").await,
        0
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(500), fixture.emails.recv())
            .await
            .is_err(),
        "a failed CSRF check still sent mail"
    );

    // A cookie, but a different hidden token.
    let (status, _, html) = form_post(
        &fixture.router,
        "/reset",
        "198.51.100.41",
        &[("email", EMAIL), ("csrf", "00")],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(html.contains("form expired"), "{html}");

    // The confirm page: mismatched CSRF is refused before the code is checked, so the password
    // is untouched.
    let (_, cookie) = reset_form_token(&fixture.router, "/reset/code").await;
    let (status, _, html) = form_post(
        &fixture.router,
        "/reset/confirm",
        "198.51.100.42",
        &[
            ("email", EMAIL),
            ("code", "123456"),
            ("password", NEW_PASSWORD),
            ("csrf", "00"),
        ],
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(html.contains("form expired"), "{html}");
    assert_eq!(
        login(&fixture, "198.51.100.43", OLD_PASSWORD).await,
        StatusCode::OK
    );
}
