"""PreToolUse guard (Bash and PowerShell): refuse a git commit or push that carries merge conflict markers (#698).

On 2026-11-12 a scripted merge resolution missed a second conflict block and the merge commit went in with
`<<<<<<<` lines. This hook looks at the command before it runs:

  git commit (and git merge --continue)   the staged changes (with -a/--all also the unstaged ones)
  git push                                the commits not yet on the upstream (or origin/main), as one diff

and denies the command when an added line starts with git's markers `<<<<<<< ` or `>>>>>>> ` (seven
characters and a space, as git writes them before the side's name). `=======` alone is not matched: it is
also a Markdown setext heading underline. A resolve-and-commit in one command (`python fix.py; git commit -am x`)
is checked before the script runs, so its commit passes; the push check catches what the script missed.

The repository is the last `cd`/`Set-Location`/`pushd` before the git call (or `git -C <dir>`), else the
session's cwd. Anything unreadable (not a repo, git missing, no upstream) lets the command through.
A command that really needs it adds `# conflict-guard: ok (<why>)`.

  python -I .claude/hooks/conflict_guard.py          hook mode: reads the PreToolUse JSON on stdin
  python -I .claude/hooks/conflict_guard.py --test   self-test (makes throwaway repos in the temp folder)
"""
import json
import os
import re
import subprocess
import sys
import tempfile

OVERRIDE = re.compile(r"conflict-guard:\s*ok")
MARKER = re.compile(r"^\+(<{7}|>{7}) ")
SEGMENT_SPLIT = re.compile(r"&&|\|\||[;|\n]|(?<![<>0-9])&(?=\s)")
TOKEN = re.compile(r"\"[^\"]*\"|'[^']*'|\S+")
CHDIR = {"cd", "set-location", "sl", "chdir", "pushd", "push-location"}
GIT_OPTS_WITH_VALUE = {"-C", "-c", "--git-dir", "--work-tree", "--namespace", "--exec-path"}


def native_path(path, base):
    """A path from a Bash or PowerShell command, as a directory Python can use."""
    path = path.strip("\"'")
    m = re.match(r"^/([a-zA-Z])(/|$)", path)  # Git Bash /c/Users/... -> C:/Users/...
    if m and os.name == "nt":
        path = f"{m.group(1).upper()}:/{path[3:]}"
    path = os.path.expanduser(path)
    return os.path.normpath(os.path.join(base or os.getcwd(), path))


def git_calls(command, cwd):
    """(directory, subcommand, args) for each git commit/push in the command, in order."""
    found = []
    here = cwd
    for segment in SEGMENT_SPLIT.split(command):
        tokens = TOKEN.findall(segment.strip())
        while tokens and tokens[0] in ("&", "."):
            tokens.pop(0)
        if not tokens:
            continue
        name = re.split(r"[\\/]", tokens[0].strip("\"'"))[-1].lower()
        if name in CHDIR:
            args = [t for t in tokens[1:] if not t.startswith("-")]
            if args:
                here = native_path(args[0], here)
            continue
        if name not in ("git", "git.exe"):
            continue
        where, i = here, 1
        while i < len(tokens) and tokens[i].startswith("-"):
            opt = tokens[i].split("=", 1)[0]
            if opt == "-C" and i + 1 < len(tokens):
                where = native_path(tokens[i + 1], where)
            i += 2 if opt in GIT_OPTS_WITH_VALUE and "=" not in tokens[i] else 1
        if i >= len(tokens):
            continue
        sub, rest = tokens[i], [t.strip("\"'") for t in tokens[i + 1:]]
        if sub == "merge" and "--continue" in rest:
            sub = "commit"
        if sub in ("commit", "push"):
            found.append((where, sub, rest))
    return found


def git(where, *args):
    """Output of a git command in `where`, or None when it fails."""
    try:
        out = subprocess.run(["git", "-C", where, *args], capture_output=True, timeout=8)
    except (OSError, subprocess.SubprocessError):
        return None
    return out.stdout.decode("utf-8", "replace") if out.returncode == 0 else None


def markers_in_diff(diff):
    """[(file, line)] of added conflict-marker lines in a unified diff."""
    hits, path = [], "?"
    for line in diff.splitlines():
        if line.startswith("+++ "):
            path = line[4:].removeprefix("b/")
        elif MARKER.match(line):
            hits.append((path, line[1:].rstrip()))
    return hits


DIFF = ("diff", "--no-color", "--no-ext-diff", "--no-renames", "-U0")


def check_commit(where, args):
    diff = git(where, *DIFF, "--cached")
    if diff is None:
        return []
    if "-a" in args or "--all" in args or any(re.fullmatch(r"-[a-zA-Z]*a[a-zA-Z]*", a) for a in args if not a.startswith("--")):
        diff += git(where, *DIFF) or ""
    return markers_in_diff(diff)


def check_push(where):
    for base in ("@{upstream}", "origin/HEAD", "origin/main"):
        if git(where, "rev-parse", "--verify", "--quiet", base) is not None:
            return markers_in_diff(git(where, *DIFF, f"{base}...HEAD") or "")
    return []


def check(tool_input, cwd=""):
    """The reason to block this command, or None."""
    cmd = tool_input.get("command", "") or ""
    if "git" not in cmd or OVERRIDE.search(cmd):
        return None
    for where, sub, args in git_calls(cmd, cwd):
        if not os.path.isdir(where):
            continue
        hits = check_commit(where, args) if sub == "commit" else check_push(where)
        if hits:
            shown = "\n".join(f"  {path}: {line}" for path, line in hits[:8])
            more = f"\n  ... {len(hits) - 8} more" if len(hits) > 8 else ""
            what = "staged changes" if sub == "commit" else "commits not yet pushed"
            return (f"Blocked by conflict_guard: the {what} in {where} still have merge conflict markers:\n"
                    f"{shown}{more}\nResolve those blocks (search the files for the marker lines), `git add` "
                    "them and commit the fix, then retry. A false alarm: add `# conflict-guard: ok (<why>)`.")
    return None


def main():
    try:
        data = json.load(sys.stdin)
    except ValueError:
        return 0
    if data.get("tool_name") not in ("Bash", "PowerShell"):
        return 0
    reason = check(data.get("tool_input") or {}, data.get("cwd") or "")
    if reason:
        print(json.dumps({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }}))
    return 0


def self_test():
    lt, gt = "<" * 7, ">" * 7  # built here so this file never holds a marker line itself
    env = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@t", GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
    failures = []

    def run(where, *args):
        subprocess.run(["git", "-C", where, *args], check=True, capture_output=True, env=env)

    def expect(label, blocked, command, cwd):
        got = check({"command": command}, cwd)
        if bool(got) != blocked:
            failures.append(f"{label}: expected {'block' if blocked else 'pass'}, got {got!r}")

    def write(path, text):
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)

    with tempfile.TemporaryDirectory() as tmp:
        origin, repo = os.path.join(tmp, "origin.git"), os.path.join(tmp, "repo")
        run(tmp, "init", "-q", "--bare", "-b", "main", origin)
        run(tmp, "clone", "-q", origin, repo)
        run(repo, "commit", "-q", "--allow-empty", "-m", "root")
        run(repo, "push", "-q", "origin", "HEAD:main")
        run(repo, "branch", "-q", "--set-upstream-to=origin/main")
        doc = os.path.join(repo, "doc.md")

        # Clean files and look-alikes pass: setext underline, inline mention, 8 '<', marker without a space.
        write(doc, f"Title\n=======\n\nGit writes `{lt} HEAD`.\n{lt}<\n{gt}\n")
        run(repo, "add", "doc.md")
        expect("clean staged commit", False, "git commit -m x", repo)
        expect("clean push", False, "git push", repo)
        run(repo, "commit", "-q", "-m", "clean")

        # A staged marker blocks the commit, however the repo is named in the command.
        write(doc, f"Title\n{lt} HEAD\nours\n=======\ntheirs\n{gt} origin/main\n")
        run(repo, "add", "doc.md")
        expect("staged marker", True, "git commit -m 'merge'", repo)
        expect("cd then commit", True, f'cd "{repo}"; git add -A; git commit -m x', tmp)
        expect("git -C", True, f'git -C "{repo}" commit -m x', tmp)
        expect("Set-Location", True, f"Set-Location '{repo}' && git commit --no-edit", tmp)
        expect("merge --continue", True, "git merge --continue", repo)
        expect("override", False, "git commit -m x # conflict-guard: ok (fixture)", repo)
        expect("other repo", False, f'git -C "{tmp}" commit -m x', repo)
        expect("not a commit", False, "git status; git log -1", repo)
        if os.name == "nt":
            bash_path = "/" + repo[0].lower() + repo[2:].replace("\\", "/")
            expect("Git Bash path", True, f"cd {bash_path} && git commit -m x", tmp)

        # Unstaged markers only count with -a.
        run(repo, "reset", "-q")
        expect("unstaged, plain commit", False, "git commit -m x", repo)
        expect("unstaged, commit -am", True, "git commit -am x", repo)

        # A committed marker blocks the push until a later commit removes it.
        run(repo, "commit", "-q", "-am", "oops")
        expect("unpushed marker", True, "git push -u origin HEAD", repo)
        write(doc, "Title\n=======\nours\n")
        run(repo, "commit", "-q", "-am", "fix")
        expect("fixed before push", False, "git push", repo)

    expect("not a repo", False, "git commit -m x", tempfile.gettempdir())
    for f in failures:
        print("self-test failed:", f)
    if failures:
        return 1
    print("conflict_guard self-test passed")
    return 0


if __name__ == "__main__":
    sys.exit(self_test() if "--test" in sys.argv[1:] else main())
