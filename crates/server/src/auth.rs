//! Accounts: registration, password sign-in, sessions and the auth forms (#156, #157;
//! docs/server.md, Accounts and sign-in).
//!
//! JSON (the native client, and the tests):
//!
//! - `POST /auth/register` `{username, email?, password}` -> `201 {id, username}`
//! - `POST /auth/login` `{login, password}` -> `200 {id, username}` and a session cookie
//!
//! Server-rendered forms (the web client; CSRF-protected):
//!
//! - `GET /register`, `GET /login`: HTML with a hidden token and a `__Host-iwbtg_csrf` cookie
//! - `POST /register`, `POST /login`: form-encoded, same rules, session cookie on sign-in
//!
//! Passwords are Argon2id at the documented OWASP parameters (m=19456 KiB, t=2, p=1) and only the
//! PHC string is stored. Hashing runs on a blocking worker: it is deliberately expensive, and the
//! async runtime must keep serving the other requests meanwhile.
//!
//! A sign-in creates a new session: 32 random bytes are the opaque token, only its SHA-256 is
//! stored, and the row expires after 30 days. The web receives it in a `__Host-iwbtg_session`
//! cookie (`HttpOnly; Secure; SameSite=Lax`), so script cannot read it and a cross-site form
//! cannot send it. CSRF is the double-submit pattern: the form page sets a separate
//! `__Host-iwbtg_csrf` cookie and embeds the same random token in a hidden field; a POST whose two
//! halves do not match never reaches the credential check.
//!
//! Session lookup for protected endpoints arrives with the first consumer (#162).
//!
//! Every hash holds [`ARGON2_M_COST`] KiB, so the endpoints are memory-bounded by [`HASH_PERMITS`]
//! rather than by the request rate (#1505): a request waits up to [`HASH_WAIT`] for a slot and is
//! then answered `429` with `Retry-After`. `compose.yaml` sizes the container from that number;
//! change one and change the other.
//!
//! Registration collides on a canonical username or a lowercased email. The two cases answer with
//! one message on purpose: a distinct "that email exists" would enumerate accounts. Login answers
//! the same 401 for an unknown account and a wrong password, and hashes against a fixed salt in
//! the unknown case so the response time does not reveal which happened.

use std::time::Duration;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::Form;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, SemaphorePermit};

use crate::AppState;

/// The documented OWASP parameters (docs/server.md, Accounts and sign-in).
const ARGON2_M_COST: u32 = 19_456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;

/// The documented minimum is 15 characters, or 8 plus a breached-password check. No breach list
/// exists yet, so the safe branch applies and 15 is the floor. The upper bound caps the work a
/// hostile request can ask for.
const MIN_PASSWORD_CHARS: usize = 15;
const MAX_PASSWORD_BYTES: usize = 1024;

/// A valid salt for the timing-equalizing hash of an unknown account. It is never stored.
const DUMMY_SALT: &str = "c29tZXNhbHRzb21lc2FsdA";

/// The web session cookie. The `__Host-` prefix requires `Secure`, `Path=/` and no `Domain`
/// (docs/server.md, Accounts and sign-in).
const SESSION_COOKIE: &str = "__Host-iwbtg_session";

/// The CSRF double-submit cookie; the form carries the same value in a hidden field.
const CSRF_COOKIE: &str = "__Host-iwbtg_csrf";

/// "Remember me for 30 days" is the default; there is no shorter session yet.
const SESSION_MAX_AGE_SECONDS: u64 = 30 * 24 * 60 * 60;

/// Shown when the hidden token and the cookie disagree, or the cookie is missing.
const CSRF_ERROR: &str = "Your form expired. Please try again.";

/// How many Argon2 hashes may run at once (#1505). Each holds [`ARGON2_M_COST`] KiB, so this is
/// what bounds the account endpoints' memory; `compose.yaml` sizes its limit from this number.
const HASH_PERMITS: usize = 4;

/// How long a request may wait for a hashing slot before it is asked to retry. Waiting absorbs a
/// short burst without failing it; the timeout turns a sustained flood into `429`, not a queue.
const HASH_WAIT: Duration = Duration::from_secs(2);

/// The process-wide hashing bound; never closed, so acquiring only fails on the timeout.
static HASHING: Semaphore = Semaphore::const_new(HASH_PERMITS);

#[derive(Deserialize)]
pub struct RegisterRequest {
    username: String,
    /// Optional: OIDC-only accounts have no address, and the schema allows NULL.
    email: Option<String>,
    password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    /// Username or email (docs/server.md: "login by username or email").
    #[serde(alias = "username")]
    login: String,
    password: String,
}

/// The `POST /register` form body (docs/server.md: server-rendered forms).
#[derive(Deserialize)]
pub struct RegisterForm {
    username: String,
    email: Option<String>,
    password: String,
    csrf: String,
}

/// The `POST /login` form body.
#[derive(Deserialize)]
pub struct LoginForm {
    login: String,
    password: String,
    csrf: String,
}

#[derive(Serialize)]
pub struct AccountResponse {
    id: i64,
    username: String,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

/// The answers these handlers give. Every failure is a JSON `{"error": ...}`.
#[derive(Debug)]
pub(crate) enum AuthError {
    BadRequest(&'static str),
    Conflict,
    Unauthorized,
    /// Every hashing slot stayed busy for [`HASH_WAIT`]; the caller should retry.
    Busy,
    Internal,
}

impl AuthError {
    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Busy => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> &'static str {
        match self {
            Self::BadRequest(error) => error,
            Self::Conflict => "username or email is already registered",
            Self::Unauthorized => "invalid username or password",
            Self::Busy => "the sign-in queue is full; retry shortly",
            Self::Internal => "the account could not be processed",
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let busy = matches!(self, Self::Busy);
        let body = Json(ErrorBody {
            error: self.message(),
        });
        let mut response = (self.status(), body).into_response();
        if busy {
            // One second is longer than a queued hash takes in practice.
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        }
        response
    }
}

/// `POST /auth/register` (docs/server.md, Accounts and sign-in).
pub(crate) async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AccountResponse>), AuthError> {
    let account = create_account(
        &state.pool,
        request.username,
        request.email.as_deref(),
        request.password,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(account)))
}

/// `POST /auth/login`, JSON, setting the session cookie like the form does.
pub(crate) async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Response, AuthError> {
    let (account, cookie) = authenticate(&state.pool, request.login.trim(), request.password).await?;
    let mut response = Json(account).into_response();
    set_cookie(&mut response, &cookie);
    Ok(response)
}

/// Validates, hashes and stores a new account; the shared body of the JSON and form handlers.
async fn create_account(
    pool: &SqlitePool,
    username: String,
    email: Option<&str>,
    password: String,
) -> Result<AccountResponse, AuthError> {
    let (username, canonical) = validate_username(&username)?;
    let email = normalize_email(email)?;
    validate_password(&password)?;

    // Hash before opening the transaction: this is the slow part and holds no database lock. The
    // permit is scoped to the hash, so database work does not occupy a hashing slot.
    let password_hash = {
        let _permit = hash_permit().await?;
        hash_off_thread(password).await?
    };

    let mut transaction = pool
        .begin()
        .await
        .map_err(|err| database_error(err, "begin registration"))?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO accounts (username, username_canonical, email)
         VALUES (?1, ?2, ?3)
         RETURNING id",
    )
    .bind(&username)
    .bind(&canonical)
    .bind(&email)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|err| database_error(err, "insert account"))?;
    sqlx::query("INSERT INTO password_credentials (account_id, password_hash) VALUES (?1, ?2)")
        .bind(id)
        .bind(&password_hash)
        .execute(&mut *transaction)
        .await
        .map_err(|err| database_error(err, "insert password credential"))?;
    transaction
        .commit()
        .await
        .map_err(|err| database_error(err, "commit registration"))?;

    Ok(AccountResponse { id, username })
}

/// Checks the credentials, then starts a new session; the shared body of both sign-ins.
async fn authenticate(
    pool: &SqlitePool,
    login: &str,
    password: String,
) -> Result<(AccountResponse, String), AuthError> {
    let stored = lookup(pool, login).await?;
    let (password_hash, account) = match stored {
        Some((id, username, hash)) => (Some(hash), Some((id, username))),
        None => (None, None),
    };
    let verified = {
        let _permit = hash_permit().await?;
        verify_off_thread(password, password_hash).await?
    };

    match (verified, account) {
        (true, Some((id, username))) => {
            let cookie = create_session(pool, id).await?;
            Ok((AccountResponse { id, username }, cookie))
        }
        _ => Err(AuthError::Unauthorized),
    }
}

/// A new opaque session: the token goes to the client, only its SHA-256 is stored.
///
/// The `expires_at` default is computed by SQLite in the schema's timestamp format, so the server
/// needs no date library.
async fn create_session(pool: &SqlitePool, account_id: i64) -> Result<String, AuthError> {
    let token = random_hex(32);
    let id = random_hex(16);
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, expires_at)
         VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+30 days'))",
    )
    .bind(&id)
    .bind(account_id)
    .bind(&token_hash)
    .execute(pool)
    .await
    .map_err(|err| database_error(err, "insert session"))?;

    Ok(format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Lax; \
         Max-Age={SESSION_MAX_AGE_SECONDS}"
    ))
}

/// 32 random bytes as 64 hex characters: opaque, URL- and header-safe, no base64 dependency.
fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buffer);
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn set_cookie(response: &mut Response, cookie: &str) {
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(cookie).expect("an ASCII cookie"),
    );
}

/// `GET /register`: the form with a fresh CSRF token in a hidden field and its cookie.
pub(crate) async fn register_page() -> Response {
    let token = random_hex(32);
    html_page(
        StatusCode::OK,
        render_register(None, &token),
        Some(csrf_cookie(&token)),
    )
}

/// `GET /login`: the same for signing in.
pub(crate) async fn login_page() -> Response {
    let token = random_hex(32);
    html_page(
        StatusCode::OK,
        render_login(None, &token),
        Some(csrf_cookie(&token)),
    )
}

/// `POST /register`: the form path through the same account rules as the JSON endpoint.
pub(crate) async fn register_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<RegisterForm>,
) -> Response {
    if !csrf_matches(&headers, &form.csrf) {
        return csrf_rejection(render_register);
    }
    match create_account(&state.pool, form.username, form.email.as_deref(), form.password).await {
        Ok(account) => html_page(
            StatusCode::OK,
            success_page(
                &format!("Account created, {}.", account.username),
                "/login",
                "Sign in",
            ),
            None,
        ),
        Err(error) => {
            let token = random_hex(32);
            html_page(
                error.status(),
                render_register(Some(error.message()), &token),
                Some(csrf_cookie(&token)),
            )
        }
    }
}

/// `POST /login`: credentials plus a new session cookie.
pub(crate) async fn login_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    if !csrf_matches(&headers, &form.csrf) {
        return csrf_rejection(render_login);
    }
    match authenticate(&state.pool, form.login.trim(), form.password).await {
        Ok((account, cookie)) => html_page(
            StatusCode::OK,
            notice_page(&format!("Signed in as {}.", account.username)),
            Some(cookie),
        ),
        Err(error) => {
            let token = random_hex(32);
            html_page(
                error.status(),
                render_login(Some(error.message()), &token),
                Some(csrf_cookie(&token)),
            )
        }
    }
}

/// A failed CSRF check gets a fresh form (and token), never a credential check.
fn csrf_rejection(render: fn(Option<&str>, &str) -> String) -> Response {
    let token = random_hex(32);
    html_page(
        StatusCode::FORBIDDEN,
        render(Some(CSRF_ERROR), &token),
        Some(csrf_cookie(&token)),
    )
}

/// The double-submit check: the hidden field must equal the cookie the form page set.
fn csrf_matches(headers: &HeaderMap, submitted: &str) -> bool {
    !submitted.is_empty() && cookie_value(headers, CSRF_COOKIE).is_some_and(|cookie| cookie == submitted)
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

fn csrf_cookie(token: &str) -> String {
    format!("{CSRF_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Lax")
}

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

fn page(title: &str, content: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{title}</title><style>{PAGE_STYLE}</style></head>\
         <body><main>{content}</main></body></html>"
    )
}

const PAGE_STYLE: &str = "body{font-family:system-ui,sans-serif;max-width:22rem;margin:3rem auto;\
padding:0 1rem;line-height:1.5}label{display:block;margin-top:.75rem}input{width:100%;box-sizing:\
border-box;padding:.45rem;font:inherit}button{margin-top:1.25rem;padding:.5rem 1rem;font:inherit}\
input:focus-visible,button:focus-visible,a:focus-visible{outline:2px solid #0b5fff;outline-offset:\
2px}.error{color:#b00020}.hint{color:#555;font-size:.9em}";

fn error_paragraph(error: Option<&str>) -> String {
    match error {
        Some(error) => format!("<p class=\"error\" role=\"alert\">{error}</p>"),
        None => String::new(),
    }
}

fn render_register(error: Option<&str>, csrf: &str) -> String {
    let form = format!(
        "<form method=\"post\" action=\"/register\">\
         <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
         <label for=\"username\">Username</label>\
         <input id=\"username\" name=\"username\" autocomplete=\"username\" required minlength=\"3\" \
         maxlength=\"15\">\
         <label for=\"email\">Email <span class=\"hint\">(optional)</span></label>\
         <input id=\"email\" name=\"email\" type=\"email\" autocomplete=\"email\">\
         <label for=\"password\">Password</label>\
         <input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"new-password\" \
         required minlength=\"15\"><p class=\"hint\">At least 15 characters.</p>\
         <button type=\"submit\">Create account</button></form>\
         <p>Already registered? <a href=\"/login\">Sign in</a>.</p>"
    );
    page(
        "Create an account",
        &format!("<h1>Create an account</h1>{}{form}", error_paragraph(error)),
    )
}

fn render_login(error: Option<&str>, csrf: &str) -> String {
    let form = format!(
        "<form method=\"post\" action=\"/login\">\
         <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
         <label for=\"login\">Username or email</label>\
         <input id=\"login\" name=\"login\" autocomplete=\"username\" required>\
         <label for=\"password\">Password</label>\
         <input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"current-password\" \
         required>\
         <button type=\"submit\">Sign in</button></form>\
         <p>New here? <a href=\"/register\">Create an account</a>.</p>"
    );
    page(
        "Sign in",
        &format!("<h1>Sign in</h1>{}{form}", error_paragraph(error)),
    )
}

fn success_page(message: &str, href: &str, label: &str) -> String {
    page(
        "IWBTG Collection",
        &format!("<h1>{message}</h1><p><a href=\"{href}\">{label}</a></p>"),
    )
}

fn notice_page(message: &str) -> String {
    page("IWBTG Collection", &format!("<h1>{message}</h1>"))
}

/// The account's id, typed username and password hash, found by username or email.
async fn lookup(pool: &SqlitePool, login: &str) -> Result<Option<(i64, String, String)>, AuthError> {
    // A username can never contain `@`, so the two lookups cannot overlap. A malformed address
    // at login is simply an unknown account (401 after the same work), not a validation error:
    // registration is where the rules are explained.
    let row = if login.contains('@') {
        let Ok(Some(email)) = normalize_email(Some(login)) else {
            return Ok(None);
        };
        sqlx::query_as(
            "SELECT accounts.id, accounts.username, password_credentials.password_hash
             FROM accounts
             JOIN password_credentials ON password_credentials.account_id = accounts.id
             WHERE accounts.deleted_at IS NULL AND accounts.email = ?1",
        )
        .bind(email)
        .fetch_optional(pool)
        .await
    } else {
        let canonical = canonical_username(login);
        sqlx::query_as(
            "SELECT accounts.id, accounts.username, password_credentials.password_hash
             FROM accounts
             JOIN password_credentials ON password_credentials.account_id = accounts.id
             WHERE accounts.deleted_at IS NULL AND accounts.username_canonical = ?1",
        )
        .bind(canonical)
        .fetch_optional(pool)
        .await
    };
    row.map_err(|err| database_error(err, "look up account"))
}

/// Validates the typed name and returns it with its canonical form (docs/server.md).
///
/// The canonical form (lowercase, spaces as `_`) is what the unique index holds, so "Foo Bar" and
/// "foo_bar" cannot both exist while the player keeps the name they typed.
fn validate_username(raw: &str) -> Result<(String, String), AuthError> {
    let username = raw.trim();
    let length = username.chars().count();
    if !(3..=15).contains(&length) {
        return Err(AuthError::BadRequest("username must be 3 to 15 characters"));
    }
    if !username.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, ' ' | '_' | '-' | '[' | ']')
    }) {
        return Err(AuthError::BadRequest(
            "username may only use letters, digits, spaces, _ - [ ]",
        ));
    }
    if username.contains("  ") {
        return Err(AuthError::BadRequest("username has doubled spaces"));
    }
    Ok((username.to_owned(), canonical_username(username)))
}

/// Lowercase and spaces as `_`; the unique form of a typed username.
fn canonical_username(username: &str) -> String {
    username
        .chars()
        .map(|character| match character {
            ' ' => '_',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

/// Lowercases and validates an address; `None` stays `None`, and an empty string means none.
fn normalize_email(raw: Option<&str>) -> Result<Option<String>, AuthError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let email = raw.trim().to_ascii_lowercase();
    if email.is_empty() {
        return Ok(None);
    }
    let invalid = || AuthError::BadRequest("email is not valid");
    if email.len() > 254 || email.chars().any(char::is_whitespace) {
        return Err(invalid());
    }
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(invalid());
    };
    if local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
        || !domain.contains('.')
    {
        return Err(invalid());
    }
    Ok(Some(email))
}

/// The documented minimum, with a cap so a huge body cannot turn into a long Argon2 run.
fn validate_password(password: &str) -> Result<(), AuthError> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(AuthError::BadRequest("password must be at least 15 characters"));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(AuthError::BadRequest("password is too long"));
    }
    Ok(())
}

/// Argon2id at the documented parameters.
fn argon2() -> Argon2<'static> {
    let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, None)
        .expect("the documented Argon2id parameters are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| err.to_string())
}

fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else {
        tracing::error!("stored password hash is not a valid PHC string");
        return false;
    };
    argon2().verify_password(password.as_bytes(), &parsed).is_ok()
}

/// Spend the same work as a real verification when the account does not exist, so login timing
/// does not say whether a username or email is registered.
fn burn_password_time(password: &str) {
    if let Ok(salt) = SaltString::from_b64(DUMMY_SALT) {
        let _ = argon2().hash_password(password.as_bytes(), &salt);
    }
}

/// One hashing slot, waiting at most [`HASH_WAIT`] before the request is told to retry.
async fn hash_permit() -> Result<SemaphorePermit<'static>, AuthError> {
    hash_permit_for(HASH_WAIT).await
}

/// [`hash_permit`] with an explicit wait, so a test can prove the refusal without sleeping for
/// the production timeout.
async fn hash_permit_for(wait: Duration) -> Result<SemaphorePermit<'static>, AuthError> {
    match tokio::time::timeout(wait, HASHING.acquire()).await {
        Ok(Ok(permit)) => Ok(permit),
        // Nothing closes the semaphore; a closed one is a bug, not a burst.
        Ok(Err(_)) => Err(AuthError::Internal),
        Err(_) => Err(AuthError::Busy),
    }
}

async fn hash_off_thread(password: String) -> Result<String, AuthError> {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|err| {
            tracing::error!(%err, "password hashing task failed");
            AuthError::Internal
        })?
        .map_err(|err| {
            tracing::error!(%err, "password hashing failed");
            AuthError::Internal
        })
}

async fn verify_off_thread(password: String, password_hash: Option<String>) -> Result<bool, AuthError> {
    tokio::task::spawn_blocking(move || match password_hash {
        Some(password_hash) => verify_password(&password, &password_hash),
        None => {
            burn_password_time(&password);
            false
        }
    })
    .await
    .map_err(|err| {
        tracing::error!(%err, "password verification task failed");
        AuthError::Internal
    })
}

/// A unique violation is a registration collision; anything else is ours to investigate.
fn database_error(error: sqlx::Error, context: &'static str) -> AuthError {
    if let sqlx::Error::Database(ref database_error) = error
        && database_error.is_unique_violation()
    {
        return AuthError::Conflict;
    }
    tracing::error!(%error, context = context, "auth database error");
    AuthError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_username_lowercases_and_joins_spaces() {
        assert_eq!(canonical_username("Foo Bar"), "foo_bar");
        assert_eq!(canonical_username("foo_bar"), "foo_bar");
        assert_eq!(canonical_username("[Clan] Kid-1"), "[clan]_kid-1");
    }

    #[test]
    fn the_username_rules_match_the_documented_set() {
        assert!(validate_username("abc").is_ok());
        assert!(validate_username("  Foo Bar  ").is_ok());
        assert!(validate_username("[clan] Foo_1").is_ok());
        for invalid in [
            "ab",
            "this name is way too long",
            "bad!",
            "a  b",
            "tab\tname",
            "José",
        ] {
            assert!(validate_username(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn emails_are_lowercased_and_empty_is_none() {
        assert_eq!(
            normalize_email(Some("  Foo@Example.COM ")).unwrap(),
            Some("foo@example.com".to_owned())
        );
        assert_eq!(normalize_email(None).unwrap(), None);
        assert_eq!(normalize_email(Some("   ")).unwrap(), None);
    }

    #[test]
    fn invalid_emails_are_rejected() {
        for invalid in [
            "no-at-sign",
            "@example.com",
            "foo@",
            "foo@example",
            "foo@@example.com",
            "foo@.example.com",
            "foo@example..com",
            "foo bar@example.com",
        ] {
            assert!(normalize_email(Some(invalid)).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn passwords_need_fifteen_characters_and_are_capped() {
        assert!(validate_password("fourteen chars").is_err());
        assert!(validate_password("fifteen chars!!").is_ok());
        assert!(validate_password(&"x".repeat(1025)).is_err());
    }

    #[tokio::test]
    async fn a_burst_beyond_the_permits_is_refused_after_the_wait() {
        // Hold every slot, then ask with a short wait: the request is told to retry instead of
        // queueing without a memory bound.
        let held: Vec<_> = (0..HASH_PERMITS)
            .map(|_| HASHING.try_acquire().expect("a free hashing slot"))
            .collect();
        let refused = match hash_permit_for(Duration::from_millis(10)).await {
            Ok(_) => panic!("a hashing slot was free while all were held"),
            Err(error) => error,
        };
        assert!(matches!(refused, AuthError::Busy));

        drop(held);
        assert!(
            hash_permit_for(Duration::from_millis(10)).await.is_ok(),
            "the slot is free again after the burst"
        );
    }

    #[test]
    fn a_busy_answer_is_429_with_retry_after() {
        let response = AuthError::Busy.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("1")
        );
    }
}
