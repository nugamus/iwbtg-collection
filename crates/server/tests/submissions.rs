//! `POST /submissions`: session auth, 1 MiB cap, canonical decode, storage and queueing (#162;
//! docs/server.md, Submissions).

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tower::ServiceExt;

use iwbtg_server::db;
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::replay::{Header, MAGIC, REPLAY_FORMAT, Replay, StartMode, Trailer};
use iwbtg_sim::stripped::PlayMode;

use crate::support::TempDir;

const PASSWORD: &str = "fifteen chars!!";
const GAMEPLAY_HASH: [u8; 32] = [7; 32];

/// A canonical, empty run: enough to exercise the endpoint without the sim.
fn replay_bytes() -> Vec<u8> {
    replay_with(0, 0)
}

/// The same run with a chosen seed and a different final hash, so two replays on one seed have
/// different bytes (as two real runs with different inputs would).
fn replay_with(rng_seed: u64, tag: u8) -> Vec<u8> {
    Replay {
        header: Header {
            magic: MAGIC,
            replay_format: REPLAY_FORMAT,
            sim_version: 1,
            package_id: "test.pkg".to_owned(),
            gameplay_hash: GAMEPLAY_HASH,
            engine_rev: 0,
            rng_seed,
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
            final_hash: [tag; 32],
        },
    }
    .encode()
    .expect("a valid replay")
}

async fn fixture(tag: &str) -> (TempDir, SqlitePool, Router) {
    let dir = TempDir::new(tag);
    let pool = db::connect(&dir.path().join("iwbtg.db"))
        .await
        .expect("open and migrate the database");
    let router = iwbtg_server::app(pool.clone(), None);
    (dir, pool, router)
}

async fn post_json(router: &Router, uri: &str, ip: &str, body: Value) -> (StatusCode, HeaderMap, Value) {
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
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body")
        .to_vec();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("a JSON body")
    };
    (status, headers, body)
}

/// Registers and signs in, returning the session token the login cookie carried.
async fn sign_in(router: &Router, ip: &str) -> String {
    sign_in_as(router, ip, "Submitter").await
}

/// The same for a named account, so a test can hold two.
async fn sign_in_as(router: &Router, ip: &str, username: &str) -> String {
    let (status, _, _) = post_json(
        router,
        "/auth/register",
        ip,
        json!({ "username": username, "password": PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, headers, _) = post_json(
        router,
        "/auth/login",
        ip,
        json!({ "login": username, "password": PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            value
                .strip_prefix("__Host-iwbtg_session=")
                .and_then(|rest| rest.split(';').next())
                .map(str::to_owned)
        })
        .expect("a session cookie")
}

async fn post_replay(
    router: &Router,
    replay: Vec<u8>,
    cookie: Option<&str>,
    bearer: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::post("/submissions").header(header::CONTENT_TYPE, "application/octet-stream");
    if let Some(token) = cookie {
        request = request.header(header::COOKIE, format!("__Host-iwbtg_session={token}"));
    }
    if let Some(token) = bearer {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(replay)).expect("a request"))
        .await
        .expect("router must answer");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("read the body")
        .to_vec();
    // The body-limit rejection is plain text, so not every answer is JSON.
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn count(pool: &SqlitePool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.expect("count rows")
}

/// A run's flags, each with the earlier run that explains it.
async fn flags(pool: &SqlitePool, run_id: i64) -> Vec<(String, Option<i64>)> {
    sqlx::query_as("SELECT kind, detail FROM run_flags WHERE run_id = ?1 ORDER BY kind")
        .bind(run_id)
        .fetch_all(pool)
        .await
        .expect("read the run's flags")
}

#[tokio::test]
async fn a_valid_replay_is_stored_with_its_hash_and_board_key_and_queued() {
    let (_dir, pool, router) = fixture("submit-store").await;
    let token = sign_in(&router, "203.0.113.70").await;
    let replay = replay_bytes();

    let (status, body) = post_replay(&router, replay.clone(), Some(&token), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let run_id = body["run_id"].as_i64().expect("a run id");
    assert_eq!(
        body["replay_sha256"].as_str().expect("a hash"),
        hex(&Sha256::digest(&replay))
    );

    let row: (String, String, i64, Vec<u8>, Vec<u8>, Option<i64>, String) = sqlx::query_as(
        "SELECT package_id, map_id, mods, gameplay_hash, replay, ticks, status
         FROM runs WHERE id = ?1",
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("read the run");
    assert_eq!(row.0, "test.pkg");
    assert_eq!(row.1, "main");
    assert_eq!(row.2, 0);
    assert_eq!(row.3, GAMEPLAY_HASH);
    assert_eq!(row.4, replay);
    assert_eq!(row.5, None, "a claimed tick count was stored before verification");
    assert_eq!(row.6, "pending");

    let state: String = sqlx::query_scalar("SELECT state FROM verify_jobs WHERE run_id = ?1")
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .expect("read the verify job");
    assert_eq!(state, "queued");
}

#[tokio::test]
async fn a_bearer_token_also_authenticates() {
    let (_dir, _pool, router) = fixture("submit-bearer").await;
    let token = sign_in(&router, "203.0.113.71").await;

    let (status, body) = post_replay(&router, replay_bytes(), None, Some(&token)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(body["run_id"].as_i64().is_some());
}

#[tokio::test]
async fn a_submission_without_a_session_is_rejected_before_the_body() {
    let (_dir, pool, router) = fixture("submit-anon").await;

    let (status, body) = post_replay(&router, replay_bytes(), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "sign in required");
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM runs").await, 0);
}

#[tokio::test]
async fn a_body_over_one_mib_is_refused() {
    let (_dir, pool, router) = fixture("submit-cap").await;
    let token = sign_in(&router, "203.0.113.72").await;

    let oversized = vec![0u8; 1024 * 1024 + 1];
    let (status, _) = post_replay(&router, oversized, Some(&token), None).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM runs").await, 0);
}

#[tokio::test]
async fn a_non_replay_body_is_rejected() {
    let (_dir, pool, router) = fixture("submit-garbage").await;
    let token = sign_in(&router, "203.0.113.73").await;

    let (status, body) = post_replay(&router, b"not a replay".to_vec(), Some(&token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "the replay is not valid");
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM runs").await, 0);
}

#[tokio::test]
async fn submissions_are_rate_limited_per_account() {
    let (_dir, _pool, router) = fixture("submit-limit").await;
    let token = sign_in(&router, "203.0.113.74").await;

    for attempt in 0..10 {
        let (status, _) = post_replay(&router, replay_bytes(), Some(&token), None).await;
        assert_eq!(status, StatusCode::ACCEPTED, "submission {attempt}");
    }
    let (status, body) = post_replay(&router, replay_bytes(), Some(&token), None).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "too many requests; retry shortly");
}

#[tokio::test]
async fn an_exact_copy_is_flagged_with_the_earlier_run() {
    let (_dir, pool, router) = fixture("flags-copy").await;
    let first_token = sign_in_as(&router, "203.0.113.80", "First").await;
    let second_token = sign_in_as(&router, "203.0.113.81", "Second").await;
    let replay = replay_bytes();

    let (status, first) = post_replay(&router, replay.clone(), Some(&first_token), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let first_id = first["run_id"].as_i64().expect("a run id");
    assert!(
        flags(&pool, first_id).await.is_empty(),
        "the first copy is not a copy"
    );

    let (status, second) = post_replay(&router, replay, Some(&second_token), None).await;
    assert_eq!(status, StatusCode::ACCEPTED, "a flag never rejects the run");
    let second_id = second["run_id"].as_i64().expect("a run id");
    assert_eq!(
        flags(&pool, second_id).await,
        vec![("exact_copy".to_string(), Some(first_id))]
    );
}

#[tokio::test]
async fn the_same_players_reused_seed_is_flagged_with_the_earlier_run() {
    let (_dir, pool, router) = fixture("flags-seed").await;
    let token = sign_in(&router, "203.0.113.82").await;

    let (_, first) = post_replay(&router, replay_with(7, 1), Some(&token), None).await;
    let first_id = first["run_id"].as_i64().expect("a run id");
    let (status, second) = post_replay(&router, replay_with(7, 2), Some(&token), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let second_id = second["run_id"].as_i64().expect("a run id");
    assert_eq!(
        flags(&pool, second_id).await,
        vec![("seed_reuse".to_string(), Some(first_id))]
    );

    // The seed is stored as its eight big-endian bytes, not a lossy integer.
    let stored: Vec<u8> = sqlx::query_scalar("SELECT rng_seed FROM runs WHERE id = ?1")
        .bind(second_id)
        .fetch_one(&pool)
        .await
        .expect("read the seed");
    assert_eq!(stored, 7u64.to_be_bytes());
}

#[tokio::test]
async fn another_players_seed_is_not_a_reuse_flag() {
    let (_dir, pool, router) = fixture("flags-seed-other").await;
    let first_token = sign_in_as(&router, "203.0.113.83", "First").await;
    let second_token = sign_in_as(&router, "203.0.113.84", "Second").await;

    let (_, _) = post_replay(&router, replay_with(7, 1), Some(&first_token), None).await;
    let (_, other) = post_replay(&router, replay_with(7, 2), Some(&second_token), None).await;
    let other_id = other["run_id"].as_i64().expect("a run id");
    assert!(
        flags(&pool, other_id).await.is_empty(),
        "seed reuse is scoped to one player (#790)"
    );
}

#[tokio::test]
async fn a_new_seed_is_not_a_reuse_flag() {
    let (_dir, pool, router) = fixture("flags-seed-new").await;
    let token = sign_in(&router, "203.0.113.85").await;

    let (_, _) = post_replay(&router, replay_with(7, 1), Some(&token), None).await;
    let (_, next) = post_replay(&router, replay_with(8, 2), Some(&token), None).await;
    let next_id = next["run_id"].as_i64().expect("a run id");
    assert!(flags(&pool, next_id).await.is_empty());
}
