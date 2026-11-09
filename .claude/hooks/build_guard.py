"""PreToolUse guard for cargo commands (Bash and PowerShell), from client/docs/dev-setup.md, Build speed.

Blocks the two habits that cost the most build time:
  1. Changing the build fingerprint (RUSTFLAGS, CARGO_PROFILE_*, CARGO_INCREMENTAL, CARGO_TARGET_DIR,
     CARGO_BUILD_* except CARGO_BUILD_JOBS, cargo --config): each change rebuilds every dependency (about 5 minutes) and again on the way back.
  2. Starting cargo in the background: a teammate is not woken when it finishes and sits idle until
     the lead pings it (30 to 60 minutes per wait in the 2026-11-04 run).

A command that really needs either (a one-off experiment, a lead watching a long run) adds the
comment `# build-guard: ok (<why>)` to the command.

  python -I .claude/hooks/build_guard.py          hook mode: reads the PreToolUse JSON on stdin
  python -I .claude/hooks/build_guard.py --test   self-test
"""
import json
import re
import sys

OVERRIDE = re.compile(r"build-guard:\s*ok")
CARGO = re.compile(r"(^|[\s;&|(])cargo(\.exe)?\s+(\+\S+\s+)?(build|check|clippy|test|run|nextest|doc|bench)\b")
FINGERPRINT = re.compile(
    r"\b(RUSTFLAGS|RUSTDOCFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_PROFILE_\w+|CARGO_INCREMENTAL|CARGO_TARGET_DIR"
    r"|CARGO_BUILD_(?!JOBS)\w+)\s*=|\$env:(RUSTFLAGS|CARGO_PROFILE_\w+|CARGO_INCREMENTAL|CARGO_TARGET_DIR|CARGO_BUILD_(?!JOBS)\w+)"
    r"|\s--config[\s=]"
)


def check(tool_input):
    """The reason to block this command, or None."""
    cmd = tool_input.get("command", "") or ""
    if not CARGO.search(cmd) or OVERRIDE.search(cmd):
        return None
    m = FINGERPRINT.search(cmd)
    if m:
        return (f"Blocked by build_guard: `{m.group(0).strip()}` changes the cargo build fingerprint, which "
                "rebuilds every dependency (about 5 minutes) and again when you switch back. Run cargo with the "
                "workspace defaults (client/CLAUDE.md, Builds). If this is really needed, add the comment "
                "`# build-guard: ok (<why>)` to the command.")
    if tool_input.get("run_in_background"):
        return ("Blocked by build_guard: run cargo in the foreground. A teammate is not woken when a background "
                "command finishes, so it sits idle until the lead pings it. The shell limit is 45 minutes "
                "(client/CLAUDE.md, Builds). A lead that really wants it in the background adds "
                "`# build-guard: ok (<why>)`.")
    return None


def main():
    try:
        data = json.load(sys.stdin)
    except ValueError:
        return 0
    if data.get("tool_name") not in ("Bash", "PowerShell"):
        return 0
    reason = check(data.get("tool_input") or {})
    if reason:
        print(json.dumps({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }}))
    return 0


def self_test():
    blocked = [
        {"command": "RUSTFLAGS='-D warnings' cargo test --workspace"},
        {"command": 'cd x && CARGO_PROFILE_DEV_DEBUG=0 cargo build -p iwbtg-client'},
        {"command": "$env:RUSTFLAGS='-D warnings'; cargo clippy --workspace"},
        {"command": "cargo build --config 'profile.dev.debug=0'"},
        {"command": "CARGO_TARGET_DIR=/tmp/t cargo test -p iwbtg-sim"},
        {"command": "cargo test --workspace 2>&1 | tail", "run_in_background": True},
        {"command": "cd a; cargo.exe run -p iwbtg-client -- x.pkg", "run_in_background": True},
    ]
    allowed = [
        {"command": "cargo test -p iwbtg-sim --test it wp1_determinism::"},
        {"command": "cargo test -p iwbtg-sim --target wasm32-wasip1"},
        {"command": "cargo clippy --workspace --tests -- -D warnings"},
        {"command": "echo RUSTFLAGS=x"},
        {"command": "git log --oneline", "run_in_background": True},
        {"command": "cargo test --workspace # build-guard: ok (lead watching CI parity)", "run_in_background": True},
        {"command": "RUSTFLAGS=x cargo build # build-guard: ok (one-off timing)"},
        {"command": "cargo install wasm-bindgen-cli --version 0.2.100 --locked"},
        {"command": "CARGO_BUILD_JOBS=6 cargo test --workspace"},  # job count is not in the fingerprint
    ]
    bad = [c for c in blocked if not check(c)] + [c for c in allowed if check(c)]
    for c in bad:
        print("self-test failed:", c)
    if bad:
        return 1
    print("build_guard self-test passed")
    return 0


if __name__ == "__main__":
    sys.exit(self_test() if "--test" in sys.argv[1:] else main())
