#!/usr/bin/env python3
"""One manifest-derived selection plan for CI and local `cargo xtask`.

Reads workspace members and their normal, dev, build and target dependencies. A changed crate
selects itself for tests; its transitive consumers are selected for CI tests and for a local
compile check. Unknown, policy, workflow, script, lockfile, toolchain or manifest inputs fail
closed to full coverage, so an unknown crate can never select zero tests.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TEST_DOCS = tuple(line.strip() for line in (ROOT / 'crates/xtask/test-docs.txt').read_text().splitlines()
                  if line.strip() and not line.lstrip().startswith('#'))
# Documentation fixtures are not code inputs: on their own they run the native tests that read
# them and leave the wasm/determinism matrix off. A fixture keeps a wasm target only when a test on
# that target reads it. `docs/format/script/iws.d.rhai` is `include_str!`'d by the sim's
# golden-corpus test, which the wasm32-wasip1 job and the wasm32-unknown-unknown job both run. The
# three script pages are read by the sim's script doc-example test, part of the sim test binary that
# the wasm32-wasip1 job runs (`cargo test -p iwbtg-sim`); wasm32-unknown-unknown runs only the
# determinism and golden-corpus binaries, so it does not read them. Everything else under
# `test-docs.txt` is read by native-only format/sim tests.
WASM_BOTH_FIXTURES = ('docs/format/script/iws.d.rhai',)
WASIP1_FIXTURES = ('docs/format/scripts.md', 'docs/architecture/sim/scripts.md',
                   'docs/decisions/0001-script-language.md')
# Inputs that change every crate's result or the build fingerprint. Never include secrets here.
SHARED_INPUTS = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo/config.toml',
                 'crates/xtask/test-docs.txt')
FINGERPRINT_ENV = ('RUSTFLAGS', 'CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG',
                   'CARGO_PROFILE_TEST_DEBUG', 'CARGO_TARGET_DIR', 'RUSTC_WRAPPER')


def packages(root):
    result = {}
    workspace = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']
    for member in workspace['members']:
        data = tomllib.loads((root / member / 'Cargo.toml').read_text())
        name = data['package']['name']
        if not re.fullmatch(r'[a-zA-Z0-9_-]+', name):
            raise ValueError('invalid package name')
        sections = [data, *data.get('target', {}).values()]
        deps = set()
        for section in sections:
            for kind in ('dependencies', 'dev-dependencies', 'build-dependencies'):
                for alias, value in section.get(kind, {}).items():
                    if isinstance(value, dict) and value.get('workspace'):
                        value = workspace.get('dependencies', {}).get(alias, value)
                    deps.add(value.get('package', alias) if isinstance(value, dict) else alias)
        result[name] = (member.rstrip('/') + '/', deps)
    return result


def closure(seed, graph):
    """Transitive consumers of `seed` (including the seed itself)."""
    affected = set(seed)
    while True:
        consumers = {name for name, (_, deps) in graph.items() if deps & affected}
        if consumers <= affected:
            return affected
        affected.update(consumers)


def upstream(name, graph):
    """Transitive dependencies of `name`, the inputs a test or lint of it must see."""
    seen = set()
    stack = list(graph[name][1])
    while stack:
        dep = stack.pop()
        if dep in graph and dep not in seen:
            seen.add(dep)
            stack.extend(graph[dep][1])
    return seen


def digest_file(root, path):
    digest = hashlib.sha256()
    file = root / path
    if file.is_file():
        digest.update(b'file\0')
        digest.update(file.read_bytes())
    else:
        digest.update(b'absent\0')
    return digest.digest()


# Path -> (size, mtime_ns, ctime_ns, sha256). A file's content digest is reused while all three
# change markers match, so repeated plan() calls stat every source but re-read only what changed.
_FILE_DIGESTS = {}


def file_digest(path):
    stat = path.stat()
    entry = _FILE_DIGESTS.get(path)
    if entry and entry[:3] == (stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns):
        return entry[3]
    digest = hashlib.sha256(path.read_bytes()).digest()
    _FILE_DIGESTS[path] = (stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns, digest)
    return digest


def source_digest(root, directory):
    """Content manifest of one crate directory: every file, path-sorted and content-hashed.

    The identity must see sources that arrive with a new base commit, which are not in `changed`
    (#1422). Hashing the crate's own files (not the whole workspace) keeps the cost per step small;
    `plan()` memoizes one digest per directory across its steps.
    """
    digest = hashlib.sha256()
    base = root / directory
    files = sorted(path for path in base.rglob('*') if path.is_file()) if base.is_dir() else []
    for path in files:
        digest.update(b'file\0')
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(b'\0')
        digest.update(file_digest(path))
    return digest.digest()


def identity_digest(root, graph, kind, packages, changed, everything=False, sources=None):
    """Stable identity for one step: its command kind, crate inputs and shared/environment inputs.

    A changed consumer of a crate does not invalidate that crate's tests; a changed dependency
    does. `everything` is for workspace-wide steps such as rustfmt. The step's crate and in-repo
    dependency source contents are part of the identity, so a rebase that replaces sources
    invalidates the receipt even when `changed` does not name them (#1422). Pass `sources` (a dict)
    to memoize one directory digest per plan.
    """
    digest = hashlib.sha256()
    digest.update(b'receipt-v1\0')
    digest.update(kind.encode())
    digest.update(b'\0')
    digest.update(','.join(sorted(packages)).encode())
    digest.update(b'\0')
    for path in SHARED_INPUTS:
        digest.update(path.encode())
        digest.update(b'\0')
        digest.update(digest_file(root, path))
    for key in FINGERPRINT_ENV:
        digest.update(key.encode())
        digest.update(b'\0')
        digest.update(os.environ.get(key, '').encode())
        digest.update(b'\0')

    def source(directory):
        if sources is None:
            return source_digest(root, directory)
        if directory not in sources:
            sources[directory] = source_digest(root, directory)
        return sources[directory]

    if everything:
        dirs = {prefix for prefix, _ in graph.values()}
        relevant = sorted(set(changed))
    else:
        dirs = {graph[name][0] for name in packages}
        for dep in set().union(*(upstream(name, graph) for name in packages)) if packages else set():
            dirs.add(graph[dep][0])
        relevant = {path for path in changed if any(path.startswith(d) for d in dirs)}
        if set(packages) & {'iwbtg-sim', 'iwbtg-format'}:
            relevant |= {path for path in changed if path.startswith(TEST_DOCS)}
        relevant = sorted(relevant)
    for directory in sorted(dirs):
        digest.update(b'source\0')
        digest.update(directory.encode())
        digest.update(b'\0')
        digest.update(source(directory))
    for path in relevant:
        digest.update(b'path\0')
        digest.update(path.encode())
        digest.update(b'\0')
        digest.update(digest_file(root, path))
    return digest.hexdigest()


def fixture_reason(path):
    """Why a documentation fixture selects tests, and on which targets its readers run."""
    if path in WASM_BOTH_FIXTURES:
        return ('documentation fixture read by the golden corpus: native and both wasm targets')
    if path in WASIP1_FIXTURES:
        return ('documentation fixture read by the script doc-example test: native and wasm32-wasip1')
    return 'documentation fixture read by native sim/format tests'


def plan(paths, graph, full=False, root=ROOT):
    """Explain the plan for explicit paths. `full` forces every crate.

    A documentation fixture (a `test-docs.txt` path) selects the crates whose tests read it, but it
    is not a code input: on its own it keeps the native tests and leaves the wasm/determinism matrix
    off, except for the targets where a test genuinely reads it (`WASM_BOTH_FIXTURES`,
    `WASIP1_FIXTURES`). `code` stays true for a fixture so local `cargo xtask check` still runs the
    native readers; `code_input` distinguishes an actual crate/policy change.
    """
    selected = set()
    code_crates = set()
    fixtures = []
    inputs = []
    force_full = bool(full)
    for path in paths:
        owners = {name for name, (prefix, _) in graph.items() if path.startswith(prefix)}
        if path in ('Cargo.toml', 'crates/xtask/test-docs.txt') or path.endswith('/Cargo.toml'):
            force_full = True
            code_crates.update(graph)
            inputs.append({'path': path, 'selection': 'full',
                           'reason': 'workspace/crate manifest changes the dependency graph'})
        elif owners:
            selected.update(owners)
            code_crates.update(owners)
            inputs.append({'path': path, 'selection': ','.join(sorted(owners)),
                           'reason': 'changed crate source'})
        elif path.startswith(TEST_DOCS):
            selected.update({'iwbtg-sim', 'iwbtg-format'})
            fixtures.append(path)
            inputs.append({'path': path, 'selection': 'iwbtg-sim,iwbtg-format',
                           'reason': fixture_reason(path)})
        elif path.startswith(('docs/', 'workspace/', '.claude/', 'progress/')) or path.endswith('.md'):
            inputs.append({'path': path, 'selection': 'none',
                           'reason': 'documentation or harness prose'})
        else:
            # Policy, workflows, scripts, lockfiles, toolchain, unknown/new/removed crates.
            force_full = True
            code_crates.update(graph)
            inputs.append({'path': path, 'selection': 'full',
                           'reason': 'unknown, policy, workflow, script, lock or toolchain input'})

    if force_full:
        test = set(graph)
        compile_ = set()
        selected = set(graph)
        code_crates = set(graph)
    else:
        test = selected
        compile_ = closure(selected, graph) - test
    selected_all = set(test) | set(compile_)
    # There is native work whenever a crate was selected, including for a fixture's native readers.
    code = bool(selected_all)
    # Only a crate, manifest, policy or unknown input makes this a code change. A fixture alone does
    # not, so the wasm and determinism jobs stay off unless one of their tests reads that fixture.
    code_input = bool(code_crates)
    wasm_fixture = any(path in WASM_BOTH_FIXTURES for path in fixtures)
    wasip1_fixture = wasm_fixture or any(path in WASIP1_FIXTURES for path in fixtures)
    wasip1 = (code_input and bool(selected_all & {'iwbtg-sim', 'iwbtg-format'})) or wasip1_fixture
    browser = (code_input and bool(selected_all & {'iwbtg-sim', 'iwbtg-format', 'iwbtg-client'})) or wasm_fixture
    # The browser bundle is the client's own output: build it when the client (or its web page,
    # which lives under crates/client/) changed, not merely when a sim/format dependency did.
    web = bool(code_input and test & {'iwbtg-client'})
    determinism = code_input and bool(selected_all & {'iwbtg-sim', 'iwbtg-format', 'iwbtg-tools'})
    # The public feel contracts are a sim test target that the wasm32-unknown-unknown job runs in
    # the same step as the golden corpus. That job needs `browser` and its test step is gated on
    # `wasip1` (#1169), so the feel step mirrors that pair and stays off for a docs-only change.
    feel = browser and wasip1
    # CI's native job tests the changed crates and their consumers for a code change. A
    # documentation fixture cannot change a consumer's tests, so a fixture-only plan tests only the
    # crates whose tests read it (#1405). `packages_list` keeps the full set, so `test | compile`
    # is still the plan's package set and the local check still compile-checks the consumers.
    native_packages = sorted(selected_all if code_input else test)

    local_steps = [{'kind': 'rustfmt', 'reason': 'always checks the workspace formatting'}]
    if test:
        local_steps.extend([
            {'kind': 'clippy', 'packages': sorted(test), 'reason': 'changed crates'},
            {'kind': 'test', 'packages': sorted(test), 'reason': 'changed crates'},
        ])
    if compile_:
        local_steps.append({'kind': 'compile-check', 'packages': sorted(compile_),
                            'reason': 'transitive consumers of changed crates'})
    native_reason = ('changed crates and consumers' if code_input
                     else 'native tests that read the documentation fixture' if code
                     else 'no code input')
    wasip1_reason = ('sim or format code selected' if code_input and
                     selected_all & {'iwbtg-sim', 'iwbtg-format'}
                     else 'documentation fixture read by wasm32-wasip1 tests' if wasip1_fixture
                     else 'no code input and no wasm-read fixture')
    browser_reason = ('sim, format or client code selected' if code_input and
                      selected_all & {'iwbtg-sim', 'iwbtg-format', 'iwbtg-client'}
                      else 'documentation fixture read by wasm32-unknown-unknown tests' if wasm_fixture
                      else 'no code input and no wasm-read fixture')
    determinism_reason = ('sim, format or tools code selected' if determinism
                          else 'no sim, format or tools code selected' if code_input
                          else 'no code input')
    ci_steps = [
        {'kind': 'native', 'selected': code, 'packages': native_packages, 'reason': native_reason},
        {'kind': 'clippy+test', 'packages': sorted(selected_all),
         'reason': 'CI tests changed crates and their consumers'},
        {'kind': 'wasm32-wasip1', 'selected': wasip1, 'reason': wasip1_reason},
        {'kind': 'wasm32-unknown-unknown', 'selected': browser, 'reason': browser_reason},
        {'kind': 'feel', 'selected': feel,
         'reason': 'public feel contracts run in the wasm32-unknown-unknown test step' if feel
         else 'no code input' if not code_input
         else 'the sim/format test step is not selected on wasm32-unknown-unknown'},
        {'kind': 'web', 'selected': web, 'reason': 'client or its web page selected'},
        {'kind': 'determinism', 'selected': determinism, 'reason': determinism_reason},
        {'kind': 'sim-deps', 'selected': code, 'reason': 'dependency ban applies to every code change'},
        {'kind': 'licenses', 'selected': code, 'reason': 'license check applies to every code change'},
    ]
    # One source-directory digest per plan: every step shares it, and a later check with unchanged
    # sources reproduces the same values.
    sources = {}
    for step in local_steps:
        step['identity'] = identity_digest(
            root, graph, step['kind'], step.get('packages', []), paths,
            everything=step['kind'] == 'rustfmt', sources=sources)
    return {
        'full': force_full,
        'code': code,
        'code_input': code_input,
        'fixtures': bool(fixtures),
        'packages': ' '.join('-p ' + name for name in sorted(selected_all)),
        'packages_list': sorted(selected_all),
        'native_packages': native_packages,
        'native_packages_argv': ' '.join('-p ' + name for name in native_packages),
        'test': sorted(test),
        'compile': sorted(compile_),
        'wasip1': wasip1,
        'browser': browser,
        'feel': feel,
        'web': web,
        'determinism': determinism,
        'inputs': inputs,
        'local_steps': local_steps,
        'ci_steps': ci_steps,
        'identity': identity_digest(root, graph, 'plan', selected_all, paths,
                                    everything=True, sources=sources),
    }


def scope(paths, graph, full=False):
    """Backward-compatible subset used by CI output and older callers."""
    result = plan(paths, graph, full)
    return {key: result[key] for key in
            ('code', 'packages', 'wasip1', 'browser', 'web', 'determinism')}


def render_lines(result):
    def names(key):
        return ','.join(result[key])
    lines = [
        f"full={str(result['full']).lower()}",
        f"code={str(result['code']).lower()}",
        f"test={names('test')}",
        f"compile={names('compile')}",
        f"packages={result['packages']}",
        f"wasip1={str(result['wasip1']).lower()}",
        f"browser={str(result['browser']).lower()}",
        f"feel={str(result['feel']).lower()}",
        f"web={str(result['web']).lower()}",
        f"determinism={str(result['determinism']).lower()}",
        f"identity={result['identity']}",
    ]
    for step in result['local_steps']:
        key = step['kind'] + ':' + ','.join(step.get('packages', []))
        lines.append(f"step-identity={key}={step['identity']}")
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', help='git revision to diff against (CI)')
    parser.add_argument('--paths-stdin', action='store_true',
                        help='read affected paths from stdin, one per line (local xtask)')
    parser.add_argument('--full', action='store_true')
    parser.add_argument('--format', choices=('json', 'lines'), default='json')
    parser.add_argument('--github-output', type=Path)
    args = parser.parse_args()
    paths, full = [], args.full
    if args.paths_stdin:
        paths = [line.strip() for line in sys.stdin.read().splitlines() if line.strip()]
    elif not full:
        try:
            if not args.base:
                raise ValueError('missing base')
            raw = subprocess.check_output(['git', 'diff', '--name-only', '--no-renames', '-z',
                                           args.base, 'HEAD', '--'], cwd=ROOT)
            paths = raw.decode().rstrip('\0').split('\0') if raw else []
        except (ValueError, subprocess.CalledProcessError, UnicodeError):
            full = True
    result = plan(paths, packages(ROOT), full)
    print(render_lines(result) if args.format == 'lines' else json.dumps(result, sort_keys=True))
    if args.github_output:
        with args.github_output.open('a') as out:
            for key in ('code', 'packages', 'wasip1', 'browser', 'web', 'determinism'):
                value = result[key]
                out.write(f'{key}={str(value).lower() if isinstance(value, bool) else value}\n')
            # CI's native job tests this set: consumers for a code change, only the fixture readers
            # for a fixture-only change (#1405).
            out.write(f"native_packages={result['native_packages_argv']}\n")


if __name__ == '__main__':
    main()
