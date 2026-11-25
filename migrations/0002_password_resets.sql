-- Password reset codes (#160; docs/server.md, Accounts and sign-in).
--
-- One active code per account: a new request replaces the row, so an old code stops working the
-- moment a new one is asked for. Only the SHA-256 of the 6-digit code is stored; the expiry and
-- the attempt counter are what make guessing a million possibilities impractical online.
CREATE TABLE password_resets (
    account_id INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    code_hash  BLOB NOT NULL, -- SHA-256 of the six ASCII digits
    expires_at TEXT NOT NULL,
    attempts   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
