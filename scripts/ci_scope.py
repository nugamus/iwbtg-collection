#!/usr/bin/env python3
"""One manifest-derived selection plan for CI and local `cargo xtask`.

Reads workspace members and their normal, dev, build and target dependencies. A changed crate
selects itself for tests; its transitive consumers are selected for CI tests and for a local
compile check. Unknown, policy, workflow, script, lockfile, toolchain or manifest inputs fail
closed to full coverage, so an unknown crate can never select zero tests.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TEST_DOCS = tuple(line.strip() for line in (ROOT / 'crates/xtask/test-docs.txt').read_text().splitlines()
                  if line.strip() and not line.lstrip().startswith('#'))


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


def plan(paths, graph, full=False):
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
        elif path.startswith(('docs/', 'workspace/', '.claude/')) or path.endswith('.md'):
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
        {'kind': 'determinism', 'selected': determinism, 'reason': 'sim, format or tools selected'},
        {'kind': 'sim-deps', 'selected': code, 'reason': 'dependency ban applies to every code change'},
        {'kind': 'licenses', 'selected': code, 'reason': 'license check applies to every code change'},
    ]
    return {
        'full': force_full,
        'code': code,
        'packages': ' '.join('-p ' + name for name in sorted(selected_all)),
        'packages_list': sorted(selected_all),
        'test': sorted(test),
        'compile': sorted(compile_),
        'wasip1': wasip1,
        'browser': browser,
        'determinism': determinism,
        'inputs': inputs,
        'local_steps': local_steps,
        'ci_steps': ci_steps,
    }


def scope(paths, graph, full=False):
    """Backward-compatible subset used by CI output and older callers."""
    result = plan(paths, graph, full)
    return {key: result[key] for key in
            ('code', 'packages', 'wasip1', 'browser', 'determinism')}


def render_lines(result):
    def names(key):
        return ','.join(result[key])
    return '\n'.join([
        f"full={str(result['full']).lower()}",
        f"code={str(result['code']).lower()}",
        f"test={names('test')}",
        f"compile={names('compile')}",
        f"packages={result['packages']}",
        f"wasip1={str(result['wasip1']).lower()}",
        f"browser={str(result['browser']).lower()}",
        f"determinism={str(result['determinism']).lower()}",
    ]) + '\n'


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
            for key in ('code', 'packages', 'wasip1', 'browser', 'determinism'):
                value = result[key]
                out.write(f'{key}={str(value).lower() if isinstance(value, bool) else value}\n')


if __name__ == '__main__':
    main()
