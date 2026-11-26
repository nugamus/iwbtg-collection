import unittest
from prune_ci_cache import victims


def cache(i, key, size=10, ref='refs/heads/main'):
    return dict(id=i, key=key, size_in_bytes=size, ref=ref,
                created_at=f'2026-11-{i:02}T00:00:00Z', last_accessed_at=f'2026-11-{i:02}T00:00:00Z')


class CacheTests(unittest.TestCase):
    def test_only_superseded_rust_generations_are_removed(self):
        xs = [cache(1, 'v0-rust-native-env-old'), cache(2, 'v0-rust-native-env-new'),
              cache(3, 'v0-rust-wasm-env-current'), cache(4, 'unrelated-cache')]
        self.assertEqual(victims(xs, 30), ([1], 30))
        self.assertEqual(victims(xs, 0), ([1], 30))  # current target families stay protected
        self.assertEqual(victims(xs, 40), ([], 40))

    def test_refs_and_target_fingerprints_are_independent(self):
        xs = [cache(1, 'v0-rust-report-native-new'), cache(2, 'v0-rust-report-wasm-new'),
              cache(3, 'v0-rust-report-native-new', ref='refs/pull/1/merge')]
        self.assertEqual(victims(xs, 0), ([], 30))


if __name__ == '__main__':
    unittest.main()
