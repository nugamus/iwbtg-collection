//! Accounts: registration and password sign-in (#156; docs/server.md, Accounts and sign-in).
//!
//! Two JSON endpoints, used by the native client and by the server-rendered forms #157 adds:
//!
//! - `POST /auth/register` `{username, email?, password}` -> `201 {id, username}`
//! - `POST /auth/login` `{login, password}` -> `200 {id, username}`
//!
//! Passwords are Argon2id at the documented OWASP parameters (m=19456 KiB, t=2, p=1) and only the
//! PHC string is stored. Hashing runs on a blocking worker: it is deliberately expensive, and the
//! async runtime must keep serving the other requests meanwhile.
//!
//! Sessions are #157's job. Sign-in here proves the credentials and returns the account; the
//! caller has no session yet.
//!
//! Registration collides on a canonical username or a lowercased email. The two cases answer with
//! one message on purpose: a distinct "that email exists" would enumerate accounts. Login answers
//! the same 401 for an unknown account and a wrong password, and hashes against a fixed salt in
//! the unknown case so the response time does not reveal which happened.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

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

#[derive(Serialize)]
pub struct AccountResponse {
    id: i64,
    username: String,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

/// The four answers these handlers give. Every failure is a JSON `{"error": ...}`.
#[derive(Debug)]
pub(crate) enum AuthError {
    BadRequest(&'static str),
    Conflict,
    Unauthorized,
    Internal,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            Self::BadRequest(error) => (StatusCode::BAD_REQUEST, error),
            Self::Conflict => (StatusCode::CONFLICT, "username or email is already registered"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "invalid username or password"),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "the account could not be processed",
            ),
        };
        (status, Json(ErrorBody { error })).into_response()
    }
}

/// `POST /auth/register` (docs/server.md, Accounts and sign-in).
pub(crate) async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AccountResponse>), AuthError> {
    let (username, canonical) = validate_username(&request.username)?;
    let email = normalize_email(request.email.as_deref())?;
    validate_password(&request.password)?;

    // Hash before opening the transaction: this is the slow part and holds no database lock.
    let password_hash = hash_off_thread(request.password).await?;

    let mut transaction = state
        .pool
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

    Ok((StatusCode::CREATED, Json(AccountResponse { id, username })))
}

/// `POST /auth/login` (docs/server.md, Accounts and sign-in).
pub(crate) async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<AccountResponse>, AuthError> {
    let password = request.password;
    let stored = lookup(&state.pool, request.login.trim()).await?;

    let (password_hash, account) = match stored {
        Some((id, username, hash)) => (Some(hash), Some((id, username))),
        None => (None, None),
    };
    let verified = verify_off_thread(password, password_hash).await?;

    match (verified, account) {
        (true, Some((id, username))) => Ok(Json(AccountResponse { id, username })),
        _ => Err(AuthError::Unauthorized),
    }
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
    use super::{canonical_username, normalize_email, validate_password, validate_username};

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
}
