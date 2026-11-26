-- Review flags for submissions (#164; docs/server.md, Submissions).
--
-- `rng_seed` is the replay header's u64 kept as its eight big-endian bytes: an INTEGER cast could
-- lose the high bit, and seeds are compared bit for bit.

ALTER TABLE runs ADD COLUMN rng_seed BLOB;

-- Seed reuse is scoped to one account and one board (#790): board-wide reuse flags fire on
-- honest runs, because 2^32 seeds collide by the birthday bound.
CREATE INDEX runs_seed ON runs (account_id, package_id, map_id, mods, gameplay_hash, rng_seed);

-- One row per (run, kind). `detail` names the earlier run that explains the flag.
CREATE TABLE run_flags (
    run_id INTEGER NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
    kind   TEXT NOT NULL, -- 'exact_copy' | 'seed_reuse'
    detail INTEGER REFERENCES runs (id) ON DELETE SET NULL,
    PRIMARY KEY (run_id, kind)
);

-- The review queue reads flags by kind.
CREATE INDEX run_flags_kind ON run_flags (kind, run_id);
