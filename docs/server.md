# Server stack

The leaderboard, accounts and game downloads run on the maintainer's home server. Chosen for few moving parts, strong defaults and easy operation by agents. Versions were checked on 2026-10-23; check them again when Phase 8 starts.

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
| Game downloads | Content-addressed files `/files/<sha256>.<ext>` served with Range support, plus `/catalog.json` | Resumable downloads, end-to-end hash check, caches never go stale. |
| Backups | restic to remote storage (Backblaze B2 or a Hetzner Storage Box) | Encrypted, deduplicated, retention built in. Replaces the earlier "push backups to a Git server" idea: Git would keep password hashes and emails forever, unencrypted, and could never forget a deleted account. |
| Logs and monitoring | `tracing` JSON logs to stdout; `/healthz` pinged to healthchecks.io every 5 minutes | Alerts arrive even when the house loses power or internet. |
| Dependency updates | Dependabot PRs for cargo and compose, `cargo audit` in CI | Updates arrive as reviewable PRs, never silently. |

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

## Risks

- Cloudflare's terms limit serving many large files (audio) through its CDN on free plans. When downloads grow, move `/files/` to Cloudflare R2 (no egress fees) by changing catalog URLs.
- Cloudflare terminates TLS, so it sees requests in plaintext. Accepted in exchange for hiding the home IP.
- The server verifies on Linux x86_64. Determinism tests must include that target, not only Windows and WASM.
- Losing the restic password makes every backup useless. Keep it in a password manager and test a restore monthly.
- One home box means outages during power or ISP failures. Backups cover data, not uptime.
