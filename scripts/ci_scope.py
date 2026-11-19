#!/usr/bin/env python3
"""Select CI consumers from local Cargo manifests. Unknown inputs fail closed to full CI."""
import argparse
import json
from pathlib import Path
import re
import subprocess
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


def scope(paths, graph, full=False):
    affected = set(graph) if full else set()
    for path in paths:
        owners = {name for name, (prefix, _) in graph.items() if path.startswith(prefix)}
        if path in ('Cargo.toml', 'crates/xtask/test-docs.txt') or path.endswith('/Cargo.toml'):
            affected.update(graph)
        elif owners:
            affected.update(owners)
        elif path.startswith(TEST_DOCS):
            affected.update({'iwbtg-sim', 'iwbtg-format'})
        elif path.startswith(('docs/', 'workspace/', '.claude/')) or path.endswith('.md'):
            continue
        else:
            # Includes workflows/actions/scripts, lockfiles, toolchain, policy, unknown/new crates.
            affected.update(graph)
    while True:
        consumers = {name for name, (_, deps) in graph.items() if deps & affected}
        if consumers <= affected:
            break
        affected.update(consumers)
    return {
        'code': bool(affected),
        'packages': ' '.join('-p ' + name for name in sorted(affected)),
        'wasip1': bool(affected & {'iwbtg-sim', 'iwbtg-format'}),
        'browser': bool(affected & {'iwbtg-sim', 'iwbtg-format', 'iwbtg-client'}),
        'determinism': bool(affected & {'iwbtg-sim', 'iwbtg-format', 'iwbtg-tools'}),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base')
    parser.add_argument('--full', action='store_true')
    parser.add_argument('--github-output', type=Path)
    args = parser.parse_args()
    paths, full = [], args.full
    if not full:
        try:
            if not args.base:
                raise ValueError('missing base')
            raw = subprocess.check_output(['git', 'diff', '--name-only', '--no-renames', '-z',
                                           args.base, 'HEAD', '--'], cwd=ROOT)
            paths = raw.decode().rstrip('\0').split('\0') if raw else []
        except (ValueError, subprocess.CalledProcessError, UnicodeError):
            full = True
    result = scope(paths, packages(ROOT), full)
    print(json.dumps(result, sort_keys=True))
    if args.github_output:
        with args.github_output.open('a') as out:
            for key, value in result.items():
                out.write(f'{key}={str(value).lower() if isinstance(value, bool) else value}\n')


if __name__ == '__main__':
    main()
