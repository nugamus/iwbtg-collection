//! Replay submissions (#162; docs/server.md, Submissions).
//!
//! `POST /submissions` takes the replay file as the raw request body, at most
//! [`MAX_REPLAY_BYTES`] (enforced by a per-route body limit in [`crate::app`]). It needs a
//! session, decodes the canonical bytes through `iwbtg_sim::replay`, and stores the file with its
//! SHA-256 and the board key from the header. The claimed tick count is not stored: `runs.ticks`
//! stays NULL until the verifier reproduces the run (#163), which is also why this endpoint only
//! queues the `verify_jobs` row.
//!
//! Each accepted run records review flags in `run_flags` (#164): `exact_copy` when the same bytes
//! were stored before, and `seed_reuse` when the same account used the same seed on the same
//! board. A flag never rejects a run. Seed reuse is deliberately same-player and same-board,
//! because board-wide reuse flags fire on honest runs at scale (#790).

use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::auth::{AuthError, Session};
use crate::rate_limit::ClientIp;

/// The replay cap (docs/server.md, Replays: "request body capped (1 MiB)").
pub(crate) const MAX_REPLAY_BYTES: usize = 1024 * 1024;

/// Submissions per client address, and per account, in the window (Security rule 1).
const SUBMIT_LIMIT: u32 = 10;
const SUBMIT_WINDOW: Duration = Duration::from_secs(60);

#[derive(Serialize)]
pub struct Submitted {
    run_id: i64,
    /// Lowercase hex of the SHA-256 over the exact bytes received.
    replay_sha256: String,
}

/// `POST /submissions` (docs/server.md, Submissions; #162).
pub(crate) async fn submit(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Session { account_id }: Session,
    body: Bytes,
) -> Result<(StatusCode, Json<Submitted>), AuthError> {
    limit_submit(&state, &ip, account_id)?;

    let replay = iwbtg_sim::replay::Replay::decode(&body).map_err(|error| {
        // The detail is useful to a bug report, not to the caller; the answer stays generic.
        tracing::debug!(%error, "rejected replay");
        AuthError::BadReplay
    })?;
    let header = &replay.header;
    let replay_sha256 = Sha256::digest(&body).to_vec();
    // The header's seed is compared bit for bit, so keep all eight bytes; an INTEGER cast could
    // lose the high bit of the u64.
    let rng_seed = header.rng_seed.to_be_bytes();

    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|err| database_error(err, "begin submission"))?;
    // Flags are recorded, never a rejection (#164). Looking before the insert keeps the new run
    // out of its own matches; `detail` names the earliest run that explains the flag.
    let exact_copy: Option<i64> =
        sqlx::query_scalar("SELECT id FROM runs WHERE replay_sha256 = ?1 ORDER BY id LIMIT 1")
            .bind(&replay_sha256)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|err| database_error(err, "look for an exact copy"))?;
    let seed_reuse: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM runs
         WHERE account_id = ?1 AND package_id = ?2 AND map_id = ?3 AND mods = ?4
           AND gameplay_hash = ?5 AND rng_seed = ?6
         ORDER BY id LIMIT 1",
    )
    .bind(account_id)
    .bind(&header.package_id)
    .bind(&header.map)
    .bind(i64::from(header.mods.bits()))
    .bind(&header.gameplay_hash[..])
    .bind(rng_seed.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|err| database_error(err, "look for a reused seed"))?;

    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO runs
             (account_id, package_id, map_id, mods, gameplay_hash, rng_seed, replay, replay_sha256)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         RETURNING id",
    )
    .bind(account_id)
    .bind(&header.package_id)
    .bind(&header.map)
    .bind(i64::from(header.mods.bits()))
    .bind(&header.gameplay_hash[..])
    .bind(rng_seed.as_slice())
    .bind(body.as_ref())
    .bind(&replay_sha256)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|err| database_error(err, "insert run"))?;
    for (kind, detail) in [("exact_copy", exact_copy), ("seed_reuse", seed_reuse)] {
        if let Some(detail) = detail {
            sqlx::query("INSERT INTO run_flags (run_id, kind, detail) VALUES (?1, ?2, ?3)")
                .bind(run_id)
                .bind(kind)
                .bind(detail)
                .execute(&mut *transaction)
                .await
                .map_err(|err| database_error(err, "record a submission flag"))?;
        }
    }
    // The verifier (#163) works this queue; the run's ticks stay NULL until it has reproduced
    // them, so a claimed time is never on a board.
    sqlx::query("INSERT INTO verify_jobs (run_id) VALUES (?1)")
        .bind(run_id)
        .execute(&mut *transaction)
        .await
        .map_err(|err| database_error(err, "queue verification"))?;
    transaction
        .commit()
        .await
        .map_err(|err| database_error(err, "commit submission"))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(Submitted {
            run_id,
            replay_sha256: hex(&replay_sha256),
        }),
    ))
}

fn limit_submit(state: &AppState, ip: &str, account_id: i64) -> Result<(), AuthError> {
    state
        .limiter
        .check(&format!("submit-ip:{ip}"), SUBMIT_LIMIT, SUBMIT_WINDOW)
        .and_then(|()| {
            state.limiter.check(
                &format!("submit-account:{account_id}"),
                SUBMIT_LIMIT,
                SUBMIT_WINDOW,
            )
        })
        .map_err(AuthError::RateLimited)
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn database_error(error: sqlx::Error, context: &'static str) -> AuthError {
    tracing::error!(%error, context = context, "submission database error");
    AuthError::Internal
}
