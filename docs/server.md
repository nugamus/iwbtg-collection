# Server stack

The leaderboard, accounts and game downloads run on the maintainer's home server. Chosen for few moving parts, strong defaults and easy operation by agents. Versions were checked on 2026-10-23; check them again when Phase 8 starts.

Status: the skeleton exists: `crates/server` builds the `iwbtg-server` binary (axum 0.8 + tokio, `--bind`/`IWBTG_BIND`, default `127.0.0.1:8080`) and serves `GET /healthz` (#152). It opens the SQLite database (`--db`/`IWBTG_DB`, default `data/iwbtg.db`), sets WAL and runs the migrations at startup (#153, Schema below). A published folder is served as `GET /catalog.json` and `GET /files/<sha256>[.<ext>]` (`--catalog-dir`/`IWBTG_CATALOG_DIR`, Game downloads below); unset, both answer 404 (#807). Accounts, submissions and the verify worker are later issues.

## Components

| Part | Choice | Why |
|---|---|---|
| Server | One Rust binary, `crates/server`, on axum 0.8 + tokio | Links `crates/sim` directly, so verification runs the exact simulation the client runs. |
| Database | SQLite (WAL mode, one file) through sqlx 0.9, migrations in `migrations/` run at startup | Small data, no extra service, compile-time-checked SQL. Move to PostgreSQL only if one machine stops being enough. |
| Replays | BLOB column in `runs`, request body capped (1 MiB), SHA-256 stored for exact-copy detection | Replays are a few KB. One transaction, one backup unit. |
| Replay verification | `verify_jobs` table, worker spawns `iwbtg-server verify` as a child process per job | A hostile replay can only crash or stall its own child. See security rules. |
| Internet exposure | Cloudflare Tunnel (`cloudflared`) | No router port forwards, home IP hidden, DDoS protection in front, TLS handled by Cloudflare. |
| Admin access | Tailscale (not Funnel) for SSH and admin pages | No open ports. |
| Host | Debian 13, Docker Compose with two services: `server`, `cloudflared` | One `compose.yaml` in git describes production. |
| Game downloads | Content-addressed files `/files/<sha256>[.<ext>]` with a long immutable cache and single-range resume, plus `/catalog.json` with a short one | Resumable downloads, end-to-end hash check, caches never go stale. |
| Backups | restic to remote storage (Backblaze B2 or a Hetzner Storage Box) | Encrypted, deduplicated, retention built in. Replaces the earlier "push backups to a Git server" idea: Git would keep password hashes and emails forever, unencrypted, and could never forget a deleted account. |
| Logs and monitoring | `tracing` JSON logs to stdout; the check's healthchecks.io URL pinged every 5 minutes (`HEALTHCHECKS_URL`) | Alerts arrive even when the house loses power or internet. |
| Dependency updates | Dependabot PRs for cargo and compose, `cargo audit` in CI | Updates arrive as reviewable PRs, never silently. |

## Schema

One SQLite file, opened once at startup by `crates/server/src/db.rs`. `IWBTG_DB` (or `--db`) names it; the folder is created if missing. Every connection sets `journal_mode = wal`, `foreign_keys = on`, `busy_timeout = 5000` and `synchronous = normal`. WAL is a property of the file, so a crash or a second process cannot leave it half-switched.

Migrations live in `migrations/` at the workspace root (not in the crate), are compiled into the binary with `sqlx::migrate!`, and run before the listener binds. A failed migration is a startup error: the server logs it and exits 1 rather than serve a half-migrated file. sqlx records what ran in `_sqlx_migrations`; the startup log line `database ready (migrations applied)` carries the count. The list is append-only: sqlx stores a checksum per migration, so editing one that already ran also stops startup (version mismatch) instead of letting the file and the binary disagree.

| Table | Keys and indexes | Holds |
|---|---|---|
| `accounts` | `username_canonical` unique; `email` unique where not null | Username as typed plus its canonical form, lowercased email and whether it is verified, `deleted_at` for a soft delete. |
| `identities` | `(provider, subject)` unique; index on `account_id` | One row per sign-in method (Google, Discord, ...); a new provider is a new value, no schema change. |
| `password_credentials` | `account_id` primary key | Argon2id PHC string. OIDC-only accounts have no row. |
| `sessions` | `token_hash` unique; indexes on `account_id` and `expires_at` | Opaque session ID, SHA-256 of the bearer token, expiry. |
| `runs` | `(package_id, map_id, mods, gameplay_hash, ticks, submitted_at)`; indexes on `replay_sha256` and `(account_id, ticks)` | The replay BLOB (at most 1 MiB), its SHA-256 for exact-copy detection, the board identity, and the verified tick count (`NULL` until the verifier has reproduced it). |
| `verify_jobs` | index on `(state, id)` and `run_id` | Queue state per submitted run: `queued`, `running`, `done` or `failed`, with attempts, timings and the verifier's error. |

Timestamps are UTC ISO-8601 text and hashes are raw 32-byte BLOBs. Boards sort on `ticks` with ties to the earlier `submitted_at`, keyed on the replay header's `(package_id, map_id, mods, gameplay_hash)` ([versioning](architecture/versioning.md#what-a-board-keys-on)); `gameplay_hash` therefore covers `engine_rev`, and no product columns beyond the replay header are stored.

Queries that need compile-time checking use `sqlx::query!`; the `.sqlx` offline data is committed once the first such query lands (#162). Until then the only SQL in `crates/server` is the PRAGMA and schema probes in the tests, which use runtime `query_scalar`, so the build needs no live database and no `DATABASE_URL`.

## Game downloads

`crates/server/src/catalog.rs` serves a folder chosen at startup with `--catalog-dir` (`IWBTG_CATALOG_DIR`), so the binary never needs the catalog baked in (#807). Unset, `/catalog.json` and `/files/...` answer 404 and the rest of the server runs normally. The folder layout is:

```text
<catalog-dir>/
  catalog.json
  files/<sha256>[.<ext>]
```

- `/catalog.json` is read per request and sent `Cache-Control: public, max-age=60`: it changes when a game is published, so caches pick a new one up within a minute.
- `/files/...` is sent `Cache-Control: public, max-age=31536000, immutable`, because the file name is the hash: those bytes can never change under that URL.
- A file name is accepted only as 64 lowercase hex characters plus an optional alphanumeric extension (at most 16). Everything else — `..`, encoded or literal separators, a non-hash name — is 404 before the filesystem is touched, and a missing catalog or file is also 404. There is no directory listing.
- `Content-Type` follows the extension (`.json`, `.png`, `.ogg`, `.wav`, `.zip`/`.iwbz`), otherwise `application/octet-stream`.
- Every file response carries `Accept-Ranges: bytes`. A single `Range: bytes=...` (`start-end`, `start-` or `-suffix`) gets `206` with the sliced body and `Content-Range`; the long cache header stays on both `200` and `206`. A valid range past the end is `416` with `Content-Range: bytes */<len>`; a malformed, other-unit or multi-range header is ignored and answered with the full `200`, never a multipart body (#1397).

## Logs and monitoring

The binary writes `tracing` JSON lines to stdout (`tracing_subscriber::fmt().json()`): one object per event with `timestamp`, `level`, `fields` and `target`. Startup lines — database ready, serving catalog, listening — are events like any other, so a collector parses them without a regex.

Set `HEALTHCHECKS_URL` (`--healthchecks-url`) to the healthchecks.io check's ping URL. The server GETs it once at startup and then every 5 minutes (`crates/server/src/health.rs`, #155); with the variable unset nothing is spawned. The URL is never logged, because anyone holding it can silence the check. The client uses the OS trust store (`native-tls`), so the runtime image needs `ca-certificates` (which pulls `libssl3` on Debian) rather than a bundled CA list.

A failed ping — DNS, connect, a 10-second timeout, or a non-2xx status — is logged at `WARN` and the loop continues after its period, so monitoring can never take the server down. healthchecks.io alerts on the absence of pings, which covers what the server cannot report about itself: power loss, a crashed container or a dead uplink.

## Security rules

These are requirements, not suggestions.

1. **Rate limiting** keys on the `CF-Connecting-IP` header with a custom extractor. Never use tower_governor's `SmartIpKeyExtractor` (spoofable through `X-Forwarded-For`) or `PeerIpKeyExtractor` (sees only cloudflared). Test that a forged `X-Forwarded-For` does not change the key. Limits apply per IP and per account on login, register, password reset, token and submit endpoints.
2. **Verification child processes** start with `env_clear()` (no secrets), a memory and CPU limit (`rlimit`), a wall-clock timeout, `kill_on_drop`, and at most 2 at a time. Replays longer than the game's maximum tick count are rejected before simulating.
3. **Secrets are split.** `cloudflared.env` holds only the tunnel token. `server.env` holds only app secrets (OAuth, SMTP). Backup credentials live in a root-only file read by the host backup timer, never mounted into a container. The B2 key is limited to one bucket without delete rights (or object lock on).
4. **Containers** publish no ports, run as non-root, `read_only: true` with `tmpfs: [/tmp]`, `cap_drop: [ALL]`, `no-new-privileges`, memory/CPU/PID limits, `restart: unless-stopped`. The database lives in a bind mount (`/srv/iwbtg/data`) writable by the container user.
5. **Network isolation.** The server sits on its own VLAN or guest network, or a host firewall blocks it from reaching the home LAN. The research archive of original games never lives on this machine.
6. **Admin pages** (review queue, takedowns) are reachable only over Tailscale, and admin accounts require a second factor.
7. **Host hardening.** SSH key-only, root login off, unattended security upgrades on. Docker engine updates are applied by hand or through an explicitly added apt origin.
8. **Same site for the web client.** The WASM build is served from the same registrable domain as the API (for example `play.<domain>` and `api.<domain>`), so session cookies work.

## Deployment

Production is one `compose.yaml` with two services, `server` and `cloudflared` (Components). CI parses it with `docker compose config` in the `changes` job on every PR (#1419), so a broken deployment file fails before it reaches the host. The image comes from the multi-stage `Dockerfile` in the repository root: the builder compiles `iwbtg-server` on the pinned Rust toolchain with the Cargo registry and target directory held in BuildKit cache mounts, and the runtime is `debian:trixie-slim` plus the binary, the CA roots and `curl` (the healthcheck client). Nothing in the running image is writable except the database mount.

### Host layout

Only `/srv/iwbtg` is created by hand. Secrets stay outside the checkout and are never mounted into the server container (rule 3):

| Path | Holds | Owner and mode |
|---|---|---|
| `/srv/iwbtg/cloudflared.env` | `TUNNEL_TOKEN` only | `root:root`, `600` |
| `/srv/iwbtg/server.env` | app secrets (OAuth, SMTP); optional until they exist | `root:root`, `600` |
| named volume `iwbtg-data` | the SQLite file and its WAL siblings, at `/srv/iwbtg/data` | uid/gid `10001` |

Rule 4 names `/srv/iwbtg/data` as the writable database path. Compose backs it with the named volume `iwbtg-data` rather than a host bind mount: Docker seeds a fresh volume with the image directory's ownership, so the non-root server (uid/gid `10001`) can write without a `chown` on the host. A bind mount at the same path also works; create it and `chown 10001:10001` it first.

### Cloudflare Tunnel

Create the tunnel in the Cloudflare dashboard (Zero Trust > Networks > Tunnels > Create a tunnel > Cloudflared), copy its token into `/srv/iwbtg/cloudflared.env`, and add a public hostname whose service is `http://server:8080`; the compose network resolves `server`. Ingress rules live in the dashboard, so `cloudflared` needs only its token and there is no second config file in git. Neither service publishes a host port (rule 4), so the tunnel is the only way in.

### Install, start and update

```sh
sudo install -d -m 700 /srv/iwbtg
sudo install -m 600 deploy/cloudflared.env.example /srv/iwbtg/cloudflared.env
sudoedit /srv/iwbtg/cloudflared.env            # paste the tunnel token

docker compose config                          # validates the file, touches nothing
docker compose up -d --build
docker compose ps                              # server healthy, cloudflared up
curl -fsS https://<public-hostname>/healthz    # -> ok
```

Create the token file before the first `up`: `cloudflared` exits without it and restarts until it exists. `docker compose logs -f server` follows the JSON logs. Updates are `git pull && docker compose build && docker compose up -d`; migrations run at startup and a failed one exits 1 instead of serving (Schema). The host restic timer reads the volume through a throwaway container (`docker run --rm -v iwbtg-data:/data:ro ...`), so backup credentials never enter a container (rule 3).

## Accounts and sign-in

Reference flow: https://typebeat.sh/register (observed 2026-10-23). One centred card, square corners, one accent colour, "Continue with Google" as a ghost button, an "OR" divider, short forms with hints under each field, login by username or email, "Remember me for 30 days", password reset by emailed 6-digit code.

Design:

- Tables: `accounts`, `identities(account_id, provider, subject)`, `password_credentials`, `sessions`. A new sign-in method is a new `provider` value and one handler; nothing after sign-in changes.
- Sessions are opaque random tokens stored hashed, with expiry; a new session ID at every sign-in. Web uses an `HttpOnly; Secure; SameSite=Lax` cookie (`__Host-` prefix where possible); the native client sends `Authorization: Bearer`.
- Auth pages are server-rendered HTML forms with CSRF protection, visible `:focus-visible` styles, and correct `autocomplete` values.
- Passwords: Argon2id at OWASP parameters (m=19456 KiB, t=2, p=1). Minimum 15 characters, or 8 plus a breached-password check. Password reset and registration answer identically whether or not an account exists.
- Usernames: 3 to 15 characters of letters, digits, spaces, `_ - [ ]`, trimmed, no doubled spaces. A canonical form (lowercase, spaces as `_`) is unique, so "Foo Bar" and "foo_bar" cannot both exist.
- Email: unique after lowercasing. A Google identity links to an existing account only when both emails are verified; otherwise the user links it from settings.
- Google and Discord use OpenID Connect (`openidconnect` crate) with PKCE, state and nonce. Discord publishes OIDC discovery; confirm it returns an `id_token` before relying on it.
- The native game client never asks for a password. It opens the system browser to our sign-in page and receives a code on a loopback redirect (`http://127.0.0.1:<port>`, PKCE, no client secret in the exe), then stores a rotating refresh token in the OS keyring. Reused refresh tokens revoke the whole token family. A device-code fallback covers machines where loopback fails.
- Email verification needs SMTP. Until SMTP exists, leaderboard submissions are gated by the review queue instead of email verification.
- Accounts can be deleted and exported. A privacy page exists before accounts go public.

### Password endpoints

`crates/server/src/auth.rs` serves the first two steps (#156):

- `POST /auth/register` takes `{username, email?, password}` and answers `201 {id, username}`. The username is stored as typed plus its canonical form; the email is optional, lowercased and unique when present. A duplicate username and a duplicate email get the same `409` message, so registration does not say which one exists.
- `POST /auth/login` takes `{login, password}`, where `login` is a username (any casing) or an email, and answers `200 {id, username}`. An unknown account and a wrong password get the same `401`; the unknown path still runs an Argon2id hash, so the timing does not tell them apart either.
- Only the Argon2id PHC string (m=19456 KiB, t=2, p=1) is stored, never the password. Hashing runs on a blocking worker. Passwords must be 15 characters until a breached-password list exists (the documented 8 + breach branch).
- These endpoints are JSON only and create no session; #157 adds sessions, cookies, CSRF and the server-rendered forms.

## Risks

- Cloudflare's terms limit serving many large files (audio) through its CDN on free plans. When downloads grow, move `/files/` to Cloudflare R2 (no egress fees) by changing catalog URLs.
- Cloudflare terminates TLS, so it sees requests in plaintext. Accepted in exchange for hiding the home IP.
- The server verifies on Linux x86_64. Determinism tests must include that target, not only Windows and WASM.
- Losing the restic password makes every backup useless. Keep it in a password manager and test a restore monthly.
- One home box means outages during power or ISP failures. Backups cover data, not uptime.
