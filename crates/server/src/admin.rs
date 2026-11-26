//! The review queue, on its own listener (docs/server.md, Admin access; #58).
//!
//! Moderation is deliberately a second HTTP surface: the public listener never mounts these
//! routes, and the operator binds this one to the tailnet (security rule 6). The operator also
//! configures the admin account ids and one TOTP secret in `server.env`; a signed-in admin
//! account must then pass the TOTP step before the queue answers.
//!
//! A `__Host-` cookie is host-scoped, so the admin surface serves its own sign-in: it mounts the
//! same `/login` page and handler as the public surface, and the session cookie it sets belongs to
//! the admin host. The unlock then adds [`ADMIN_COOKIE`] on top.
//!
//! The queue is derived, not stored: every run with a `run_flags` row (the kinds are open, so the
//! verifier and later flags appear without a query change) plus each board's leading verified run.
//! A `run_reviews` row records the moderator's decision. Nothing here changes a tick count, a board
//! key or verification; a quarantine is a review state that the boards (#57) will honour.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::Extension;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use hmac::{Hmac, KeyInit, Mac};
use rand_core::{OsRng, RngCore};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::AppState;
use crate::auth::{self, Session};

/// The TOTP-unlocked admin cookie; host-scoped, so it only exists on the admin listener's host.
const ADMIN_COOKIE: &str = "__Host-iwbtg_admin";

/// The double-submit cookie for the admin forms.
const CSRF_COOKIE: &str = "__Host-iwbtg_admin_csrf";

/// An unlock lasts one working day; re-entering the TOTP code is cheap.
const ADMIN_SESSION_HOURS: u64 = 8;
const ADMIN_MAX_AGE_SECONDS: u64 = ADMIN_SESSION_HOURS * 60 * 60;

/// RFC 6238 defaults, with SHA-256 as the HMAC and the otpauth URL says so.
const TOTP_STEP_SECONDS: u64 = 30;
const TOTP_DIGITS: u32 = 6;
/// Accept the previous and next step, so a slightly slow clock does not lock the operator out.
const TOTP_WINDOW: i64 = 1;

/// How many leading verified runs per board enter the queue. One is the board's record; a wider
/// top-N is a product choice that can follow #57.
const TOP_RUNS_PER_BOARD: i64 = 1;

/// The page shows the newest entries first and stops here.
const QUEUE_LIMIT: i64 = 200;

/// The admin surface's configuration, read from `server.env` by the binary.
#[derive(Clone)]
pub struct AdminConfig {
    accounts: Vec<i64>,
    secret: Vec<u8>,
}

impl AdminConfig {
    /// Builds the config, decoding the base32 TOTP secret.
    pub fn new(accounts: Vec<i64>, secret_base32: &str) -> Result<Self, String> {
        if accounts.is_empty() {
            return Err("no admin account ids".to_owned());
        }
        let secret = decode_base32(secret_base32)
            .map_err(|error| format!("the TOTP secret is not base32: {error}"))?;
        if secret.is_empty() {
            return Err("the TOTP secret is empty".to_owned());
        }
        Ok(Self { accounts, secret })
    }

    /// Whether `account_id` may use the admin surface.
    pub fn allows(&self, account_id: i64) -> bool {
        self.accounts.contains(&account_id)
    }

    /// The code for a Unix time, so a test can pin the RFC 6238 vectors.
    pub fn code_at(&self, unix_seconds: u64) -> String {
        totp(&self.secret, unix_seconds)
    }

    /// Whether `submitted` is the code for `unix_seconds` (or one step either side).
    pub fn verify_code(&self, submitted: &str, unix_seconds: u64) -> bool {
        let submitted = submitted.trim();
        if submitted.len() != TOTP_DIGITS as usize || !submitted.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        (-TOTP_WINDOW..=TOTP_WINDOW).any(|offset| {
            let time = unix_seconds as i64 + offset * TOTP_STEP_SECONDS as i64;
            time >= 0 && codes_match(&totp(&self.secret, time as u64), submitted)
        })
    }
}

/// A fresh base32 secret for an authenticator app.
pub fn new_secret() -> String {
    let mut bytes = [0u8; 20];
    OsRng.fill_bytes(&mut bytes);
    encode_base32(&bytes)
}

/// The `otpauth://` URL an authenticator app imports (SHA-256, six digits, thirty seconds).
pub fn otpauth_url(secret: &str) -> String {
    format!(
        "otpauth://totp/IWBTG%20Collection:admin?secret={secret}&issuer=IWBTG%20Collection\
         &algorithm=SHA256&digits={TOTP_DIGITS}&period={TOTP_STEP_SECONDS}"
    )
}

/// The admin router. The caller serves it on its own listener and passes a state that shares the
/// database pool with the public router.
pub fn app(state: AppState, config: AdminConfig) -> Router {
    Router::new()
        // The public sign-in, served here so its `__Host-` cookie belongs to this host.
        .route("/login", get(auth::login_page).post(auth::login_form))
        .route("/", get(queue_page))
        .route("/unlock", post(unlock))
        .route("/review/{run_id}", post(review))
        .with_state(state)
        .layer(Extension(config))
}

/// `GET /`: the queue once the account is signed in, allowed and TOTP-unlocked; the unlock form
/// before that.
async fn queue_page(
    State(state): State<AppState>,
    Extension(config): Extension<AdminConfig>,
    headers: HeaderMap,
    session: Result<Session, auth::AuthError>,
) -> Response {
    let Ok(session) = session else {
        return Redirect::to("/login").into_response();
    };
    if !config.allows(session.account_id) {
        // Deliberately plain: a non-admin learns nothing about the queue.
        return (StatusCode::FORBIDDEN, "this account is not an administrator").into_response();
    }
    match admin_session_valid(&state.pool, &headers, session.account_id).await {
        Ok(true) => {}
        Ok(false) => return totp_form(StatusCode::OK, None),
        Err(err) => return internal(err, "check the admin session"),
    }
    match load_queue(&state.pool).await {
        Ok(rows) => {
            let token = random_hex(32);
            html_page(
                StatusCode::OK,
                render_queue(&rows, &token),
                Some(csrf_cookie(&token)),
            )
        }
        Err(err) => internal(err, "load the review queue"),
    }
}

/// `POST /unlock`: the TOTP code, on top of the signed-in admin account.
async fn unlock(
    State(state): State<AppState>,
    Extension(config): Extension<AdminConfig>,
    headers: HeaderMap,
    session: Result<Session, auth::AuthError>,
    Form(form): Form<UnlockForm>,
) -> Response {
    let Ok(session) = session else {
        return Redirect::to("/login").into_response();
    };
    if !config.allows(session.account_id) {
        return (StatusCode::FORBIDDEN, "this account is not an administrator").into_response();
    }
    if !csrf_matches(&headers, &form.csrf) {
        return totp_form(
            StatusCode::FORBIDDEN,
            Some("Your form expired. Please try again."),
        );
    }
    if !config.verify_code(&form.code, unix_now()) {
        return totp_form(StatusCode::FORBIDDEN, Some("The code did not match."));
    }
    match create_admin_session(&state.pool, session.account_id).await {
        Ok(cookie) => {
            let mut response = Redirect::to("/").into_response();
            set_cookie(&mut response, &cookie);
            response
        }
        Err(err) => internal(err, "create the admin session"),
    }
}

/// `POST /review/{run_id}`: clear, ask for video, or quarantine.
async fn review(
    State(state): State<AppState>,
    Extension(config): Extension<AdminConfig>,
    headers: HeaderMap,
    UrlPath(run_id): UrlPath<i64>,
    session: Result<Session, auth::AuthError>,
    Form(form): Form<ReviewForm>,
) -> Response {
    let Ok(session) = session else {
        return Redirect::to("/login").into_response();
    };
    if !config.allows(session.account_id) {
        return (StatusCode::FORBIDDEN, "this account is not an administrator").into_response();
    }
    match admin_session_valid(&state.pool, &headers, session.account_id).await {
        Ok(true) => {}
        Ok(false) => return Redirect::to("/").into_response(),
        Err(err) => return internal(err, "check the admin session"),
    }
    if !csrf_matches(&headers, &form.csrf) {
        return (StatusCode::FORBIDDEN, "Your form expired. Please try again.").into_response();
    }
    let review_state = match form.action.as_str() {
        "clear" => "cleared",
        "request_video" => "video_requested",
        "quarantine" => "quarantined",
        other => return (StatusCode::BAD_REQUEST, format!("unknown action {other:?}")).into_response(),
    };
    let exists: Option<i64> = match sqlx::query_scalar("SELECT 1 FROM runs WHERE id = ?1")
        .bind(run_id)
        .fetch_optional(&state.pool)
        .await
    {
        Ok(row) => row,
        Err(err) => return internal(err, "look up the reviewed run"),
    };
    if exists.is_none() {
        return (StatusCode::NOT_FOUND, "no such run").into_response();
    }
    let decided = matches!(review_state, "cleared" | "quarantined");
    if let Err(err) = sqlx::query(
        "INSERT INTO run_reviews (run_id, state, note, reviewer_id, decided_at)
         VALUES (?1, ?2, ?3, ?4, CASE WHEN ?5 THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now') ELSE NULL END)
         ON CONFLICT (run_id) DO UPDATE SET
             state = excluded.state,
             note = excluded.note,
             reviewer_id = excluded.reviewer_id,
             decided_at = excluded.decided_at",
    )
    .bind(run_id)
    .bind(review_state)
    .bind(form.note.as_deref().filter(|note| !note.trim().is_empty()))
    .bind(session.account_id)
    .bind(decided)
    .execute(&state.pool)
    .await
    {
        return internal(err, "record the review");
    }
    Redirect::to("/").into_response()
}

#[derive(Deserialize)]
struct UnlockForm {
    code: String,
    csrf: String,
}

#[derive(Deserialize)]
struct ReviewForm {
    action: String,
    note: Option<String>,
    csrf: String,
}

/// One queue entry: a flagged run or a board's leading verified run.
#[derive(sqlx::FromRow)]
struct QueueRow {
    id: i64,
    username: String,
    package_id: String,
    map_id: String,
    mods: i64,
    ticks: Option<i64>,
    flags: Option<String>,
    state: String,
    note: Option<String>,
}

/// The queue: every run with a flag, plus each board's leading verified run.
async fn load_queue(pool: &SqlitePool) -> Result<Vec<QueueRow>, sqlx::Error> {
    sqlx::query_as(
        "WITH ranked AS (
             SELECT id,
                    ROW_NUMBER() OVER (
                        PARTITION BY package_id, map_id, mods, gameplay_hash
                        ORDER BY ticks ASC, submitted_at ASC, id ASC
                    ) AS place
             FROM runs
             WHERE ticks IS NOT NULL AND status = 'verified'
         )
         SELECT r.id, a.username, r.package_id, r.map_id, r.mods, r.ticks,
                (SELECT group_concat(f.kind, ',') FROM run_flags f WHERE f.run_id = r.id) AS flags,
                COALESCE(rv.state, 'pending') AS state, rv.note
         FROM runs r
         JOIN accounts a ON a.id = r.account_id
         LEFT JOIN run_reviews rv ON rv.run_id = r.id
         WHERE EXISTS (SELECT 1 FROM run_flags f WHERE f.run_id = r.id)
            OR r.id IN (SELECT id FROM ranked WHERE place <= ?1)
         ORDER BY CASE COALESCE(rv.state, 'pending')
                      WHEN 'pending' THEN 0
                      WHEN 'video_requested' THEN 1
                      WHEN 'quarantined' THEN 2
                      ELSE 3
                  END,
                  r.submitted_at DESC, r.id DESC
         LIMIT ?2",
    )
    .bind(TOP_RUNS_PER_BOARD)
    .bind(QUEUE_LIMIT)
    .fetch_all(pool)
    .await
}

/// A new admin session: the token goes into [`ADMIN_COOKIE`], only its SHA-256 is stored.
async fn create_admin_session(pool: &SqlitePool, account_id: i64) -> Result<String, sqlx::Error> {
    let token = random_hex(32);
    let id = random_hex(16);
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    sqlx::query(
        "INSERT INTO admin_sessions (id, account_id, token_hash, expires_at)
         VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?4))",
    )
    .bind(&id)
    .bind(account_id)
    .bind(&token_hash)
    .bind(format!("+{ADMIN_SESSION_HOURS} hours"))
    .execute(pool)
    .await?;
    Ok(format!(
        "{ADMIN_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Lax; \
         Max-Age={ADMIN_MAX_AGE_SECONDS}"
    ))
}

/// Whether the request carries an unexpired admin session for this account.
async fn admin_session_valid(
    pool: &SqlitePool,
    headers: &HeaderMap,
    account_id: i64,
) -> Result<bool, sqlx::Error> {
    let Some(token) = cookie_value(headers, ADMIN_COOKIE) else {
        return Ok(false);
    };
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM admin_sessions
         WHERE token_hash = ?1 AND account_id = ?2
           AND expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
    )
    .bind(token_hash)
    .bind(account_id)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

fn totp_form(status: StatusCode, error: Option<&str>) -> Response {
    let token = random_hex(32);
    html_page(status, render_unlock(error, &token), Some(csrf_cookie(&token)))
}

fn render_unlock(error: Option<&str>, csrf: &str) -> String {
    let error = match error {
        Some(error) => format!("<p class=\"error\" role=\"alert\">{}</p>", escape_html(error)),
        None => String::new(),
    };
    page(
        "Review queue",
        &format!(
            "<h1>Unlock the review queue</h1>{error}\
             <p>Enter the current code from your authenticator app.</p>\
             <form method=\"post\" action=\"/unlock\">\
             <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
             <label>Authenticator code\
             <input name=\"code\" inputmode=\"numeric\" autocomplete=\"one-time-code\" \
             required autofocus></label>\
             <button type=\"submit\">Unlock</button></form>"
        ),
    )
}

fn render_queue(rows: &[QueueRow], csrf: &str) -> String {
    let mut body = String::from("<h1>Review queue</h1>");
    if rows.is_empty() {
        body.push_str("<p>Nothing is waiting for review.</p>");
    } else {
        body.push_str(
            "<table><thead><tr><th>Run</th><th>Player</th><th>Board</th>\
                       <th>Time</th><th>Flags</th><th>State</th><th>Action</th></tr></thead><tbody>",
        );
        for row in rows {
            let ticks = match row.ticks {
                Some(ticks) => format!("{ticks} ticks"),
                None => "unverified".to_owned(),
            };
            let flags = match row.flags.as_deref() {
                Some(flags) if !flags.is_empty() => escape_html(&flags.replace(',', ", ")),
                _ => "top run".to_owned(),
            };
            let note = row
                .note
                .as_deref()
                .map(|note| format!("<div class=\"hint\">{}</div>", escape_html(note)))
                .unwrap_or_default();
            body.push_str(&format!(
                "<tr><td>{id}</td><td>{player}</td><td>{package} · {map} · mods {mods}</td>\
                 <td>{ticks}</td><td>{flags}</td><td>{state}{note}</td><td>\
                 <form method=\"post\" action=\"/review/{id}\">\
                 <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
                 <select name=\"action\">\
                 <option value=\"clear\">Clear</option>\
                 <option value=\"request_video\">Request video</option>\
                 <option value=\"quarantine\">Quarantine</option></select>\
                 <input name=\"note\" placeholder=\"note\" size=\"12\">\
                 <button type=\"submit\">Apply</button></form></td></tr>",
                id = row.id,
                player = escape_html(&row.username),
                package = escape_html(&row.package_id),
                map = escape_html(&row.map_id),
                mods = row.mods,
                ticks = ticks,
                flags = flags,
                state = escape_html(&row.state),
                note = note,
                csrf = csrf,
            ));
        }
        body.push_str("</tbody></table>");
    }
    page("Review queue", &body)
}

fn page(title: &str, content: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{title}</title><style>{STYLE}</style></head>\
         <body><main>{content}</main></body></html>"
    )
}

const STYLE: &str = "body{font-family:system-ui,sans-serif;max-width:60rem;margin:2rem auto;\
padding:0 1rem;line-height:1.5}table{border-collapse:collapse;width:100%}th,td{text-align:left;\
vertical-align:top;padding:.4rem .5rem;border-bottom:1px solid #ddd}th{font-size:.85em;\
text-transform:uppercase;letter-spacing:.03em;color:#555}form{display:flex;gap:.3rem;\
flex-wrap:wrap}label{display:block;margin-top:1rem}input,select,button{font:inherit;\
padding:.3rem}input:focus-visible,select:focus-visible,button:focus-visible,a:focus-visible\
{outline:2px solid #0b5fff;outline-offset:2px}.error{color:#b00020}.hint{color:#555;\
font-size:.9em}";

fn html_page(status: StatusCode, html: String, cookie: Option<String>) -> Response {
    let mut response = html.into_response();
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    if let Some(cookie) = cookie {
        set_cookie(&mut response, &cookie);
    }
    response
}

fn set_cookie(response: &mut Response, cookie: &str) {
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(cookie).expect("an ASCII cookie"),
    );
}

/// The double-submit check for the admin forms.
fn csrf_matches(headers: &HeaderMap, submitted: &str) -> bool {
    !submitted.is_empty() && cookie_value(headers, CSRF_COOKIE).is_some_and(|cookie| cookie == submitted)
}

fn csrf_cookie(token: &str) -> String {
    format!("{CSRF_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Lax")
}

/// One cookie's value from a `Cookie` request header, if present.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
}

/// 32 random bytes as 64 hex characters: opaque and header-safe.
fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buffer);
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn internal(error: sqlx::Error, context: &'static str) -> Response {
    tracing::error!(%error, context = context, "admin request failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "the admin request failed").into_response()
}

/// HTML-escape attacker-controlled text (replay header fields, usernames, notes).
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// RFC 6238: HMAC-SHA-256 over the time step, then dynamic truncation.
fn totp(secret: &[u8], unix_seconds: u64) -> String {
    let counter = unix_seconds / TOTP_STEP_SECONDS;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let binary = u32::from_be_bytes([
        digest[offset],
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]) & 0x7fff_ffff;
    format!(
        "{:0width$}",
        binary % 10u32.pow(TOTP_DIGITS),
        width = TOTP_DIGITS as usize
    )
}

/// Compare two codes without an early exit.
fn codes_match(expected: &str, submitted: &str) -> bool {
    if expected.len() != submitted.len() {
        return false;
    }
    expected
        .bytes()
        .zip(submitted.bytes())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

/// RFC 4648 base32; padding and spaces are accepted, case-insensitive.
fn decode_base32(text: &str) -> Result<Vec<u8>, String> {
    let mut bits = 0u32;
    let mut value = 0u32;
    let mut out = Vec::new();
    for character in text.chars() {
        if character == '=' || character.is_whitespace() {
            continue;
        }
        let digit = match character.to_ascii_uppercase() {
            upper @ 'A'..='Z' => upper as u32 - 'A' as u32,
            digit @ '2'..='7' => digit as u32 - '2' as u32 + 26,
            other => return Err(format!("{other:?} is not a base32 character")),
        };
        value = (value << 5) | digit;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((value >> bits) as u8);
            value &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

fn encode_base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut bits = 0u32;
    let mut value = 0u32;
    for byte in bytes {
        value = (value << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((value >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((value << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238's SHA-256 secret: the ASCII digits 1..=0 repeated.
    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA====";

    fn config() -> AdminConfig {
        AdminConfig::new(vec![1], RFC_SECRET).expect("the RFC secret")
    }

    #[test]
    fn totp_matches_the_rfc_6238_sha256_vectors() {
        // RFC 6238 prints eight digits; authenticator apps use six, which is the same truncated
        // value modulo 10^6 (e.g. 46119246 -> 119246).
        let config = config();
        for (time, code) in [
            (59u64, "119246"),
            (1_111_111_109, "084774"),
            (1_111_111_111, "062674"),
            (1_234_567_890, "819424"),
            (2_000_000_000, "698825"),
            (20_000_000_000, "737706"),
        ] {
            assert_eq!(config.code_at(time), code, "at {time}");
        }
    }

    #[test]
    fn the_verifier_accepts_the_neighbouring_steps_only() {
        let config = config();
        let now = 1_234_567_890u64;
        assert!(config.verify_code(&config.code_at(now), now));
        assert!(config.verify_code(&config.code_at(now - TOTP_STEP_SECONDS), now));
        assert!(config.verify_code(&config.code_at(now + TOTP_STEP_SECONDS), now));
        assert!(!config.verify_code(&config.code_at(now + 2 * TOTP_STEP_SECONDS), now));
        assert!(!config.verify_code("", now));
        assert!(!config.verify_code("12345", now));
        assert!(!config.verify_code("abcdef", now));
    }

    #[test]
    fn base32_round_trips_and_ignores_padding() {
        let bytes = b"12345678901234567890123456789012";
        let encoded = encode_base32(bytes);
        assert_eq!(decode_base32(&encoded).expect("decode"), bytes);
        assert_eq!(decode_base32(RFC_SECRET).expect("the RFC secret"), bytes);
        assert!(decode_base32("not-base32!").is_err());
    }

    #[test]
    fn the_otpauth_url_names_the_algorithm_and_digits() {
        let url = otpauth_url("ABC234");
        assert!(url.starts_with("otpauth://totp/"));
        assert!(url.contains("secret=ABC234"));
        assert!(url.contains("algorithm=SHA256"));
        assert!(url.contains("digits=6"));
        assert!(url.contains("period=30"));
    }

    #[test]
    fn html_is_escaped() {
        assert_eq!(
            escape_html("<script>\"x\" & 'y'</script>"),
            "&lt;script&gt;&quot;x&quot; &amp; &#39;y&#39;&lt;/script&gt;"
        );
    }

    #[test]
    fn an_empty_allow_list_or_secret_is_refused() {
        assert!(AdminConfig::new(Vec::new(), RFC_SECRET).is_err());
        assert!(AdminConfig::new(vec![1], "").is_err());
        assert!(AdminConfig::new(vec![1], "!!!").is_err());
    }
}
