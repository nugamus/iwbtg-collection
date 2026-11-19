"""Regression boundaries for CI selection: losing a consumer silently drops required checks."""
import unittest
from ci_scope import ROOT, packages, scope


class ScopeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.graph = packages(ROOT)

    def test_client_skips_sim_reexecution(self):
        result = scope(['crates/client/src/view.rs'], self.graph)
        self.assertEqual(result['packages'], '-p iwbtg-client')
        self.assertTrue(result['browser'])
        self.assertFalse(result['wasip1'])
        self.assertFalse(result['determinism'])

    def test_sim_includes_all_transitive_consumers(self):
        result = scope(['crates/sim/src/world.rs'], self.graph)
        for name in ('iwbtg-sim', 'iwbtg-format', 'iwbtg-tools', 'iwbtg-client'):
            self.assertIn('-p ' + name, result['packages'])
        self.assertTrue(result['wasip1'] and result['browser'] and result['determinism'])

    def test_server_does_not_build_bevy_or_wasm(self):
        result = scope(['crates/server/src/main.rs'], self.graph)
        self.assertEqual(result['packages'], '-p iwbtg-server')
        self.assertFalse(result['browser'] or result['wasip1'] or result['determinism'])

    def test_shared_unknown_deleted_and_ci_inputs_are_full(self):
        for path in ('Cargo.lock', 'Cargo.toml', 'crates/server/Cargo.toml', 'rust-toolchain.toml', '.cargo/config.toml',
                     '.github/workflows/ci.yml', 'scripts/ci_scope.py', 'crates/xtask/test-docs.txt', 'crates/removed/src/lib.rs',
                     'some-new-input'):
            with self.subTest(path=path):
                self.assertEqual(scope([path], self.graph)['packages'], scope([], self.graph, True)['packages'])

    def test_docs_read_by_tests_run_consumers(self):
        for path in ('docs/format/script/iws.d.rhai', 'docs/architecture/sim/scripts.md', 'docs/decisions/0001-script-language.md'):
            self.assertTrue(scope([path], self.graph)['wasip1'])
        self.assertFalse(scope(['README.md', 'docs/dev-setup.md', 'docs/architecture/client/shell.md', '.claude/hooks/issues.py'], self.graph)['code'])

    def test_manifest_aliases_and_target_dev_dependencies(self):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'Cargo.toml').write_text('[workspace]\nmembers = ["a", "b"]\n[workspace.dependencies]\nalias = { package = "a", path = "a" }\n')
            (root / 'a').mkdir()
            (root / 'b').mkdir()
            (root / 'a/Cargo.toml').write_text('[package]\nname = "a"\n')
            (root / 'b/Cargo.toml').write_text('[package]\nname = "b"\n[target.wasm32.dev-dependencies]\nalias = { workspace = true }\n')
            graph = packages(root)
            self.assertEqual(scope(['a/deleted.rs'], graph)['packages'], '-p a -p b')

    def test_mixed_changes_and_dependency_aliases(self):
        result = scope(['crates/server/src/main.rs', 'crates/client/src/view.rs'], self.graph)
        self.assertEqual(result['packages'], '-p iwbtg-client -p iwbtg-server')
        graph = {'a': ('crates/a/', set()), 'b': ('crates/b/', {'a'}), 'c': ('crates/c/', {'b'})}
        self.assertEqual(scope(['crates/a/deleted.rs'], graph)['packages'], '-p a -p b -p c')


if __name__ == '__main__':
    unittest.main()
