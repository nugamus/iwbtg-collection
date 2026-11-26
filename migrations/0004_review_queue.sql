-- The review queue and admin sessions (#58; docs/server.md, Admin access).
--
-- `run_reviews` records a moderator's decision for a run. The queue itself is derived from
-- `run_flags` and each board's leading verified run, so a new flag kind needs no schema change.
--
-- `admin_sessions` backs the `__Host-iwbtg_admin` cookie: a `__Host-` cookie is host-scoped, so
-- the admin listener has its own sign-in, and the TOTP unlock sits on top of that session.

CREATE TABLE run_reviews (
    run_id      INTEGER PRIMARY KEY REFERENCES runs (id) ON DELETE CASCADE,
    state       TEXT NOT NULL DEFAULT 'pending', -- pending | cleared | video_requested | quarantined
    note        TEXT,
    reviewer_id INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    opened_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    decided_at  TEXT
);

CREATE INDEX run_reviews_state ON run_reviews (state, opened_at);

CREATE TABLE admin_sessions (
    id         TEXT PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    token_hash BLOB NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX admin_sessions_expiry ON admin_sessions (expires_at);
