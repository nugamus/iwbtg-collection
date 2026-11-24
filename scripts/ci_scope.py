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


def identity_digest(root, graph, kind, packages, changed, everything=False):
    """Stable identity for one step: its command kind, crate inputs and shared/environment inputs.

    A changed consumer of a crate does not invalidate that crate's tests; a changed dependency
    does. `everything` is for workspace-wide steps such as rustfmt.
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
    if everything:
        relevant = sorted(set(changed))
    else:
        dirs = {graph[name][0] for name in packages}
        for dep in set().union(*(upstream(name, graph) for name in packages)) if packages else set():
            dirs.add(graph[dep][0])
        relevant = {path for path in changed if any(path.startswith(d) for d in dirs)}
        if set(packages) & {'iwbtg-sim', 'iwbtg-format'}:
            relevant |= {path for path in changed if path.startswith(TEST_DOCS)}
        relevant = sorted(relevant)
    for path in relevant:
        digest.update(b'path\0')
        digest.update(path.encode())
        digest.update(b'\0')
        digest.update(digest_file(root, path))
    return digest.hexdigest()


def plan(paths, graph, full=False, root=ROOT):
    """Explain the plan for explicit paths. `full` forces every crate."""
    selected = set()
    inputs = []
    force_full = bool(full)
    for path in paths:
        owners = {name for name, (prefix, _) in graph.items() if path.startswith(prefix)}
        if path in ('Cargo.toml', 'crates/xtask/test-docs.txt') or path.endswith('/Cargo.toml'):
            force_full = True
            inputs.append({'path': path, 'selection': 'full',
                           'reason': 'workspace/crate manifest changes the dependency graph'})
        elif owners:
            selected.update(owners)
            inputs.append({'path': path, 'selection': ','.join(sorted(owners)),
                           'reason': 'changed crate source'})
        elif path.startswith(TEST_DOCS):
            selected.update({'iwbtg-sim', 'iwbtg-format'})
            inputs.append({'path': path, 'selection': 'iwbtg-sim,iwbtg-format',
                           'reason': 'documentation fixture read by sim/format tests'})
        elif path.startswith(('docs/', 'workspace/', '.claude/', 'progress/')) or path.endswith('.md'):
            inputs.append({'path': path, 'selection': 'none',
                           'reason': 'documentation or harness prose'})
        else:
            # Policy, workflows, scripts, lockfiles, toolchain, unknown/new/removed crates.
            force_full = True
            inputs.append({'path': path, 'selection': 'full',
                           'reason': 'unknown, policy, workflow, script, lock or toolchain input'})

    if force_full:
        test = set(graph)
        compile_ = set()
        selected = set(graph)
    else:
        test = selected
        compile_ = closure(selected, graph) - test
    selected_all = set(test) | set(compile_)
    code = bool(selected_all)
    wasip1 = bool(selected_all & {'iwbtg-sim', 'iwbtg-format'})
    browser = bool(selected_all & {'iwbtg-sim', 'iwbtg-format', 'iwbtg-client'})
    # The browser bundle is the client's own output: build it when the client (or its web page,
    # which lives under crates/client/) changed, not merely when a sim/format dependency did.
    web = bool(test & {'iwbtg-client'})
    determinism = bool(selected_all & {'iwbtg-sim', 'iwbtg-format', 'iwbtg-tools'})

    local_steps = [{'kind': 'rustfmt', 'reason': 'always checks the workspace formatting'}]
    if test:
        local_steps.extend([
            {'kind': 'clippy', 'packages': sorted(test), 'reason': 'changed crates'},
            {'kind': 'test', 'packages': sorted(test), 'reason': 'changed crates'},
        ])
    if compile_:
        local_steps.append({'kind': 'compile-check', 'packages': sorted(compile_),
                            'reason': 'transitive consumers of changed crates'})
    ci_steps = [
        {'kind': 'native', 'selected': code, 'reason': 'changed crates and consumers'},
        {'kind': 'clippy+test', 'packages': sorted(selected_all),
         'reason': 'CI tests changed crates and their consumers'},
        {'kind': 'wasm32-wasip1', 'selected': wasip1, 'reason': 'sim or format selected'},
        {'kind': 'wasm32-unknown-unknown', 'selected': browser, 'reason': 'sim, format or client selected'},
        {'kind': 'web', 'selected': web, 'reason': 'client or its web page selected'},
        {'kind': 'determinism', 'selected': determinism, 'reason': 'sim, format or tools selected'},
        {'kind': 'sim-deps', 'selected': code, 'reason': 'dependency ban applies to every code change'},
        {'kind': 'licenses', 'selected': code, 'reason': 'license check applies to every code change'},
    ]
    for step in local_steps:
        step['identity'] = identity_digest(
            root, graph, step['kind'], step.get('packages', []), paths,
            everything=step['kind'] == 'rustfmt')
    return {
        'full': force_full,
        'code': code,
        'packages': ' '.join('-p ' + name for name in sorted(selected_all)),
        'packages_list': sorted(selected_all),
        'test': sorted(test),
        'compile': sorted(compile_),
        'wasip1': wasip1,
        'browser': browser,
        'web': web,
        'determinism': determinism,
        'inputs': inputs,
        'local_steps': local_steps,
        'ci_steps': ci_steps,
        'identity': identity_digest(root, graph, 'plan', selected_all, paths, everything=True),
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


if __name__ == '__main__':
    main()
