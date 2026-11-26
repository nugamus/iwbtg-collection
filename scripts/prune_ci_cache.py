#!/usr/bin/env python3
"""Remove superseded Rust cache generations, oldest access first, to an 8 GiB target.

The newest entry in each ref/key family is protected. Dry-run unless --apply is supplied.
"""
import argparse
import json
import subprocess


def victims(caches, budget):
    total = sum(c['size_in_bytes'] for c in caches)
    newest = {}
    for cache in caches:
        if not cache['key'].startswith('v0-rust-'):
            continue
        family = (cache['ref'], cache['key'].rsplit('-', 1)[0])
        if family not in newest or cache['created_at'] > newest[family]['created_at']:
            newest[family] = cache
    keep = {c['id'] for c in newest.values()}
    candidates = sorted((c for c in caches if c['key'].startswith('v0-rust-') and c['id'] not in keep),
                        key=lambda c: (c['last_accessed_at'], c['created_at'], c['id']))
    selected = []
    for cache in candidates:
        if total <= budget:
            break
        selected.append(cache['id'])
        total -= cache['size_in_bytes']
    return selected, total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', required=True)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    pages = json.loads(subprocess.check_output(['gh', 'api', '--paginate', '--slurp',
                       f'repos/{args.repo}/actions/caches?per_page=100']))
    selected, remaining = victims([c for p in pages for c in p['actions_caches']], 8 * 2**30)
    print(f'{len(selected)} superseded caches selected; projected retained size {remaining / 2**30:.2f} GiB')
    for cache_id in selected:
        print(('Delete' if args.apply else 'Would delete'), cache_id)
        if args.apply:
            subprocess.run(['gh', 'api', '--method', 'DELETE',
                            f'repos/{args.repo}/actions/caches/{cache_id}'], check=True)
    if remaining > 8 * 2**30:
        print('Protected current caches exceed the target; inspect cache sizes before reducing retention further.')


if __name__ == '__main__':
    main()
