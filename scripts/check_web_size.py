#!/usr/bin/env python3
"""Check the browser bundle's compressed wasm against the size budget (#1078).

`crates/client/web/pkg/iwbtg-client_bg.wasm` is what the page downloads; a server sends it
gzip-compressed, so that is the number the budget guards. CI runs `wasm-opt -Oz` on the
wasm-bindgen output before this check (#1131); the cargo profile stays the workspace release one,
so the native build is unchanged. The budget records today's optimized bundle with about 15%
headroom, so growth is a deliberate change with a measured reason (docs/dev-setup.md, CI).
"""
import argparse
import gzip
import pathlib
import sys

# 17 MiB compressed. Measured 2026-11-25 (#1131) at 20,168,221 bytes gzip from the workspace
# release profile and 15,456,322 after `wasm-opt -Oz`; the headroom covers a compiler or dependency
# bump between deliberate reviews.
BUDGET_BYTES = 17 * 1024 * 1024


def gzip_size(data: bytes) -> int:
    """The bytes a gzip server sends for `data` (level 9, fixed mtime so runs compare)."""
    return len(gzip.compress(data, 9, mtime=0))


def over_budget(size: int, budget: int = BUDGET_BYTES) -> bool:
    return size > budget


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('wasm', type=pathlib.Path, help='the bundle .wasm to measure')
    parser.add_argument('--budget', type=int, default=BUDGET_BYTES,
                        help=f'compressed budget in bytes (default {BUDGET_BYTES})')
    args = parser.parse_args()
    data = args.wasm.read_bytes()
    size = gzip_size(data)
    print(f'{args.wasm}: {len(data):,} bytes raw, {size:,} bytes gzip '
          f'(budget {args.budget:,})')
    if over_budget(size, args.budget):
        print(f'error: the compressed wasm is {size - args.budget:,} bytes over the budget; '
              'shrink the bundle or raise the budget in scripts/check_web_size.py with a '
              'measured reason (docs/dev-setup.md, CI)', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
