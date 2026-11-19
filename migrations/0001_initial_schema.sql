-- Initial schema for the leaderboard server (#153; docs/server.md, Schema).
--
-- Timestamps are UTC ISO-8601 text (`2026-11-19T12:34:56.789Z`): SQLite has no date type, and the
-- text sorts correctly. Hashes are raw BLOBs (32 bytes) so they compare byte for byte.

-- One row per person. `username` keeps what the player typed; `username_canonical` (lowercase,
-- spaces as `_`) is the unique one, so "Foo Bar" and "foo_bar" cannot both exist
-- (docs/server.md, Accounts and sign-in).
CREATE TABLE accounts (
    id                 INTEGER PRIMARY KEY,
    username           TEXT NOT NULL,
    username_canonical TEXT NOT NULL UNIQUE,
    email              TEXT, -- lowercased; NULL when no address is attached
    email_verified     INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    deleted_at         TEXT -- soft delete: accounts can be deleted and exported (docs/server.md)
);

-- Email is unique after lowercasing; accounts without one do not collide.
CREATE UNIQUE INDEX accounts_email ON accounts (email) WHERE email IS NOT NULL;

-- A sign-in method: an OIDC provider's stable subject for this account. A new provider is a new
-- row here plus one handler; nothing after sign-in changes (docs/server.md).
CREATE TABLE identities (
    id         INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    provider   TEXT NOT NULL, -- 'google', 'discord', ...
    subject    TEXT NOT NULL, -- the provider's stable user id
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (provider, subject)
);

CREATE INDEX identities_account ON identities (account_id);

-- Argon2id PHC string (m=19456 KiB, t=2, p=1). One password per account; Google/Discord-only
-- accounts have no row here.
CREATE TABLE password_credentials (
    account_id    INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Opaque session tokens, stored hashed (SHA-256 of the token the client sends), with expiry
-- (docs/server.md, Accounts and sign-in). A new session ID at every sign-in.
CREATE TABLE sessions (
    id         TEXT PRIMARY KEY, -- random, opaque; the client only ever sends the token
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    token_hash BLOB NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX sessions_account ON sessions (account_id);
CREATE INDEX sessions_expires ON sessions (expires_at);

-- Submitted runs. The replay file is the source of truth (a few KB, at most 1 MiB); the columns
-- next to it are the replay header/trailer fields the boards and the review queue query without
-- decoding the blob again (docs/architecture/replay.md).
--
-- A board keys on (package_id, map_id, mods, gameplay_hash) and sorts by ticks with ties going to
-- the earlier submission (docs/architecture/versioning.md, docs/product.md Leaderboards).
CREATE TABLE runs (
    id            INTEGER PRIMARY KEY,
    account_id    INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    package_id    TEXT NOT NULL,
    map_id        TEXT NOT NULL,
    mods          INTEGER NOT NULL DEFAULT 0, -- u32 bit set (docs/architecture/replay.md, Mods)
    gameplay_hash BLOB NOT NULL, -- 32 bytes
    -- The rankable time in ticks. NULL until the verifier has reproduced the run; a pending
    -- run's claimed time stays in its replay until then.
    ticks         INTEGER,
    replay        BLOB NOT NULL,
    replay_sha256 BLOB NOT NULL, -- exact-copy detection across accounts (#164)
    status        TEXT NOT NULL DEFAULT 'pending', -- pending | verified | rejected
    submitted_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    verified_at   TEXT
);

-- The board read: one game's ranked list, best time first, ties to the earlier submission.
CREATE INDEX runs_board ON runs (package_id, map_id, mods, gameplay_hash, ticks, submitted_at);
-- Exact-copy lookup (#164) and one player's runs (Local scope, #484).
CREATE INDEX runs_replay_sha256 ON runs (replay_sha256);
CREATE INDEX runs_account ON runs (account_id, ticks);

-- One verification per submitted run, worked off in order by child processes
-- (docs/server.md, Replay verification; #163).
CREATE TABLE verify_jobs (
    id          INTEGER PRIMARY KEY,
    run_id      INTEGER NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
    state       TEXT NOT NULL DEFAULT 'queued', -- queued | running | done | failed
    attempts    INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    started_at  TEXT,
    finished_at TEXT,
    error       TEXT
);

CREATE INDEX verify_jobs_queue ON verify_jobs (state, id);
CREATE INDEX verify_jobs_run ON verify_jobs (run_id);
