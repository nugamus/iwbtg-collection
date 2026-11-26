//! The admin review queue on its own listener (#58; docs/server.md, Admin access).
//!
//! The admin router is driven in-process like the public one. Cookies are passed explicitly:
//! a `__Host-` cookie is host-scoped in a browser, which is why the admin listener serves its
//! own sign-in; the tests stand in for that browser.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

use iwbtg_server::admin::{self, AdminConfig};
use iwbtg_server::db;
use iwbtg_server::mail::Mailer;

use crate::support::TempDir;

const PASSWORD: &str = "fifteen chars!!";
/// The RFC 6238 SHA-256 secret, so the test computes the same codes the unit test pins.
const TOTP_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA====";
const SESSION: &str = "__Host-iwbtg_session";
const ADMIN_COOKIE: &str = "__Host-iwbtg_admin";
const ADMIN_CSRF: &str = "__Host-iwbtg_admin_csrf";

struct Fixture {
    _dir: TempDir,
    pool: SqlitePool,
    public: Router,
    admin: Router,
    admin_id: i64,
    player_id: i64,
}

async fn fixture(tag: &str) -> Fixture {
    let dir = TempDir::new(tag);
    let pool = db::connect(&dir.path().join("iwbtg.db"))
        .await
        .expect("open and migrate the database");
    let public = iwbtg_server::app(pool.clone(), None);
    let admin_id = register(&public, "203.0.113.90", "Admin").await;
    let player_id = register(&public, "203.0.113.91", "Player").await;

    let state = iwbtg_server::app_state(pool.clone(), None, Mailer::disabled());
    let config = AdminConfig::new(vec![admin_id], TOTP_SECRET).expect("the test admin config");
    Fixture {
        _dir: dir,
        pool,
        public,
        admin: admin::app(state, config),
        admin_id,
        player_id,
    }
}

async fn register(public: &Router, ip: &str, username: &str) -> i64 {
    let response = public
        .clone()
        .oneshot(
            Request::post("/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .header("cf-connecting-ip", ip)
                .body(Body::from(
                    json!({ "username": username, "password": PASSWORD }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("router must answer");
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body");
    let body: Value = serde_json::from_slice(&bytes).expect("a JSON body");
    body["id"].as_i64().expect("an account id")
}

/// Signs in through the public JSON endpoint and returns the session token.
async fn sign_in(public: &Router, ip: &str, username: &str) -> String {
    let response = public
        .clone()
        .oneshot(
            Request::post("/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header("cf-connecting-ip", ip)
                .body(Body::from(
                    json!({ "login": username, "password": PASSWORD }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("router must answer");
    assert_eq!(response.status(), StatusCode::OK);
    cookie(response.headers(), SESSION).expect("the session cookie")
}

async fn get(router: &Router, uri: &str, cookies: &[(&str, &str)]) -> (StatusCode, HeaderMap, String) {
    let request = with_cookies(Request::get(uri), cookies);
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).expect("a request"))
        .await
        .expect("router must answer");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body");
    (status, headers, String::from_utf8_lossy(&bytes).into_owned())
}

async fn post_form(
    router: &Router,
    uri: &str,
    cookies: &[(&str, &str)],
    form: &[(&str, &str)],
) -> (StatusCode, HeaderMap, String) {
    let body = form
        .iter()
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let request = with_cookies(Request::post(uri), cookies)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    let response = router
        .clone()
        .oneshot(request.body(Body::from(body)).expect("a request"))
        .await
        .expect("router must answer");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body");
    (status, headers, String::from_utf8_lossy(&bytes).into_owned())
}

fn with_cookies(
    builder: axum::http::request::Builder,
    cookies: &[(&str, &str)],
) -> axum::http::request::Builder {
    if cookies.is_empty() {
        return builder;
    }
    let joined = cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    builder.header(header::COOKIE, joined)
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// One `Set-Cookie` value by name.
fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            value
                .strip_prefix(&format!("{name}="))
                .and_then(|rest| rest.split(';').next())
                .map(str::to_owned)
        })
}

/// The hidden CSRF token from a rendered form.
fn csrf(html: &str) -> String {
    let marker = "name=\"csrf\" value=\"";
    let start = html.find(marker).expect("a CSRF field") + marker.len();
    html[start..].split('"').next().expect("a CSRF value").to_owned()
}

fn code_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a modern clock")
        .as_secs();
    AdminConfig::new(vec![1], TOTP_SECRET)
        .expect("the test config")
        .code_at(now)
}

/// Signs in as the admin and unlocks the queue, returning the two cookies.
async fn unlocked(fixture: &Fixture, ip: &str) -> (String, String) {
    let session = sign_in(&fixture.public, ip, "Admin").await;
    let (status, headers, html) = get(&fixture.admin, "/", &[(SESSION, &session)]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Unlock the review queue"), "not the unlock form");
    let token = csrf(&html);
    let csrf_cookie = cookie(&headers, ADMIN_CSRF).expect("the admin CSRF cookie");

    let (status, headers, _) = post_form(
        &fixture.admin,
        "/unlock",
        &[(SESSION, &session), (ADMIN_CSRF, &csrf_cookie)],
        &[("code", &code_now()), ("csrf", &token)],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let admin_cookie = cookie(&headers, ADMIN_COOKIE).expect("the admin cookie");
    (session, admin_cookie)
}

/// Inserts a run directly: the queue only reads rows, and the flag producer is tested in #164.
async fn insert_run(pool: &SqlitePool, account_id: i64, ticks: Option<i64>, tag: u8) -> i64 {
    let status = if ticks.is_some() { "verified" } else { "pending" };
    let gameplay = [7u8; 32];
    let replay_sha256 = [tag; 32];
    sqlx::query_scalar(
        "INSERT INTO runs
             (account_id, package_id, map_id, mods, gameplay_hash, replay, replay_sha256, ticks, status)
         VALUES (?1, 'test.pkg', 'main', 0, ?2, ?3, ?4, ?5, ?6)
         RETURNING id",
    )
    .bind(account_id)
    .bind(gameplay.as_slice())
    .bind(b"replay".as_slice())
    .bind(replay_sha256.as_slice())
    .bind(ticks)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("insert a run")
}

async fn flag(pool: &SqlitePool, run_id: i64, kind: &str) {
    sqlx::query("INSERT INTO run_flags (run_id, kind, detail) VALUES (?1, ?2, NULL)")
        .bind(run_id)
        .bind(kind)
        .execute(pool)
        .await
        .expect("insert a flag");
}

/// A canonical empty replay, enough for the submission endpoint.
fn replay_bytes() -> Vec<u8> {
    use iwbtg_sim::mods::ModSet;
    use iwbtg_sim::replay::{Header, MAGIC, REPLAY_FORMAT, Replay, StartMode, Trailer};
    use iwbtg_sim::stripped::PlayMode;

    Replay {
        header: Header {
            magic: MAGIC,
            replay_format: REPLAY_FORMAT,
            sim_version: 1,
            package_id: "test.pkg".to_owned(),
            gameplay_hash: [7; 32],
            engine_rev: 0,
            rng_seed: 0,
            start_mode: StartMode::NewGame,
            play_mode: PlayMode::Stripped,
            map: "main".to_owned(),
            mods: ModSet::NONE,
            client_build_id: "test".to_owned(),
        },
        inputs: Vec::new(),
        trailer: Trailer {
            total_ticks: 0,
            splits: Vec::new(),
            ranked_ticks: 0,
            pauses: Vec::new(),
            checkpoints: Vec::new(),
            final_hash: [0; 32],
        },
    }
    .encode()
    .expect("a valid replay")
}

/// Submits through the real endpoint and returns the answer.
async fn submit(public: &Router, session: &str, replay: Vec<u8>) -> Value {
    let response = public
        .clone()
        .oneshot(
            Request::post("/submissions")
                .header(header::COOKIE, format!("{SESSION}={session}"))
                .body(Body::from(replay))
                .expect("a request"),
        )
        .await
        .expect("router must answer");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body");
    serde_json::from_slice(&bytes).expect("a JSON body")
}

#[tokio::test]
async fn an_unsigned_request_is_sent_to_the_admin_sign_in() {
    let fixture = fixture("admin-anon").await;
    let (status, headers, _) = get(&fixture.admin, "/", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers
            .get(header::LOCATION)
            .expect("a location")
            .to_str()
            .expect("ASCII"),
        "/login"
    );
}

#[tokio::test]
async fn a_non_admin_account_is_refused() {
    let fixture = fixture("admin-not-admin").await;
    let session = sign_in(&fixture.public, "203.0.113.91", "Player").await;
    let (status, _, body) = get(&fixture.admin, "/", &[(SESSION, &session)]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!body.contains("Review queue"), "the queue leaked to a non-admin");
}

#[tokio::test]
async fn the_admin_must_unlock_with_a_totp_code() {
    let fixture = fixture("admin-totp").await;
    let session = sign_in(&fixture.public, "203.0.113.90", "Admin").await;

    let (status, headers, html) = get(&fixture.admin, "/", &[(SESSION, &session)]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Unlock the review queue"));
    let token = csrf(&html);
    let csrf_cookie = cookie(&headers, ADMIN_CSRF).expect("the CSRF cookie");

    // A code from far outside the window is refused and sets no admin cookie.
    let far = AdminConfig::new(vec![1], TOTP_SECRET).expect("config").code_at(0);
    let (status, headers, _) = post_form(
        &fixture.admin,
        "/unlock",
        &[(SESSION, &session), (ADMIN_CSRF, &csrf_cookie)],
        &[("code", &far), ("csrf", &token)],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(cookie(&headers, ADMIN_COOKIE).is_none());

    // The current code unlocks it.
    let (status, headers, _) = post_form(
        &fixture.admin,
        "/unlock",
        &[(SESSION, &session), (ADMIN_CSRF, &csrf_cookie)],
        &[("code", &code_now()), ("csrf", &token)],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let admin_cookie = cookie(&headers, ADMIN_COOKIE).expect("the admin cookie");

    let (status, _, html) = get(
        &fixture.admin,
        "/",
        &[(SESSION, &session), (ADMIN_COOKIE, &admin_cookie)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Review queue"));
}

#[tokio::test]
async fn the_queue_lists_a_flagged_run_and_records_an_action() {
    let fixture = fixture("admin-queue").await;
    let run_id = insert_run(&fixture.pool, fixture.player_id, Some(120), 1).await;
    flag(&fixture.pool, run_id, "exact_copy").await;
    let (session, admin_cookie) = unlocked(&fixture, "203.0.113.90").await;

    let (status, headers, html) = get(
        &fixture.admin,
        "/",
        &[(SESSION, &session), (ADMIN_COOKIE, &admin_cookie)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(&format!("<td>{run_id}</td>")), "the run is missing");
    assert!(html.contains("exact_copy"));
    assert!(html.contains("Player"));
    let token = csrf(&html);
    let csrf_cookie = cookie(&headers, ADMIN_CSRF).expect("the CSRF cookie");

    let (status, _, _) = post_form(
        &fixture.admin,
        &format!("/review/{run_id}"),
        &[
            (SESSION, &session),
            (ADMIN_COOKIE, &admin_cookie),
            (ADMIN_CSRF, &csrf_cookie),
        ],
        &[
            ("action", "clear"),
            ("note", "checked manually"),
            ("csrf", &token),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let row: (String, Option<String>, Option<i64>, Option<String>) =
        sqlx::query_as("SELECT state, note, reviewer_id, decided_at FROM run_reviews WHERE run_id = ?1")
            .bind(run_id)
            .fetch_one(&fixture.pool)
            .await
            .expect("the review row");
    assert_eq!(row.0, "cleared");
    assert_eq!(row.1.as_deref(), Some("checked manually"));
    assert_eq!(row.2, Some(fixture.admin_id));
    assert!(row.3.is_some(), "a decision carries its timestamp");
}

/// The issue's own user check: submit a copied replay and find it in the queue.
#[tokio::test]
async fn a_copied_replay_reaches_the_queue() {
    let fixture = fixture("admin-copied").await;
    let player = sign_in(&fixture.public, "203.0.113.91", "Player").await;
    let replay = replay_bytes();

    let _first = submit(&fixture.public, &player, replay.clone()).await;
    let copied = submit(&fixture.public, &player, replay).await;
    let copied_id = copied["run_id"].as_i64().expect("a run id");

    let (session, admin_cookie) = unlocked(&fixture, "203.0.113.90").await;
    let (status, _, html) = get(
        &fixture.admin,
        "/",
        &[(SESSION, &session), (ADMIN_COOKIE, &admin_cookie)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(&format!("<td>{copied_id}</td>")),
        "the copied replay is not in the queue"
    );
    // Identical bytes also reused the seed on the same board, so both flags are listed.
    assert!(html.contains("exact_copy"));
    assert!(html.contains("seed_reuse"));
}

#[tokio::test]
async fn each_board_leader_enters_the_queue_without_a_flag() {
    let fixture = fixture("admin-top").await;
    let leader = insert_run(&fixture.pool, fixture.player_id, Some(100), 2).await;
    let slower = insert_run(&fixture.pool, fixture.player_id, Some(200), 3).await;
    let (session, admin_cookie) = unlocked(&fixture, "203.0.113.90").await;

    let (status, _, html) = get(
        &fixture.admin,
        "/",
        &[(SESSION, &session), (ADMIN_COOKIE, &admin_cookie)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(&format!("<td>{leader}</td>")),
        "the leader is missing"
    );
    assert!(
        !html.contains(&format!("<td>{slower}</td>")),
        "a slower run is not the record"
    );
    assert!(html.contains("top run"));
}

#[tokio::test]
async fn a_csrf_mismatch_and_an_unknown_action_change_nothing() {
    let fixture = fixture("admin-csrf").await;
    let run_id = insert_run(&fixture.pool, fixture.player_id, Some(120), 4).await;
    flag(&fixture.pool, run_id, "seed_reuse").await;
    let (session, admin_cookie) = unlocked(&fixture, "203.0.113.90").await;

    let (status, _, _) = post_form(
        &fixture.admin,
        &format!("/review/{run_id}"),
        &[(SESSION, &session), (ADMIN_COOKIE, &admin_cookie)],
        &[("action", "clear"), ("csrf", "not-the-cookie")],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_reviews")
        .fetch_one(&fixture.pool)
        .await
        .expect("count reviews");
    assert_eq!(count, 0, "a CSRF mismatch recorded a review");

    // A fresh form token, but an unknown action.
    let (_, headers, html) = get(
        &fixture.admin,
        "/",
        &[(SESSION, &session), (ADMIN_COOKIE, &admin_cookie)],
    )
    .await;
    let token = csrf(&html);
    let csrf_cookie = cookie(&headers, ADMIN_CSRF).expect("the CSRF cookie");
    let (status, _, _) = post_form(
        &fixture.admin,
        &format!("/review/{run_id}"),
        &[
            (SESSION, &session),
            (ADMIN_COOKIE, &admin_cookie),
            (ADMIN_CSRF, &csrf_cookie),
        ],
        &[("action", "ban"), ("csrf", &token)],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_reviews")
        .fetch_one(&fixture.pool)
        .await
        .expect("count reviews");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn the_admin_listener_serves_its_own_sign_in() {
    let fixture = fixture("admin-login").await;
    let (status, headers, html) = get(&fixture.admin, "/login", &[]).await;
    assert_eq!(status, StatusCode::OK);
    let token = csrf(&html);
    let csrf_cookie = cookie(&headers, "__Host-iwbtg_csrf").expect("the login CSRF cookie");

    let (status, headers, _) = post_form(
        &fixture.admin,
        "/login",
        &[("__Host-iwbtg_csrf", &csrf_cookie)],
        &[("login", "Admin"), ("password", PASSWORD), ("csrf", &token)],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the admin host signs in with the same form"
    );
    assert!(
        cookie(&headers, SESSION).is_some(),
        "no session on the admin host"
    );
}
