"""Regression boundaries for CI and local selection: losing a consumer drops required checks.

The real manifest graph is read once; every expectation below is derived from the same
`packages()`/`closure()` data the selector uses, so a legitimate dependency edge cannot break a
literal list here (client #1562). The only pinned set is the documentation-fixture reader policy
(`READERS`), which is not a manifest edge. `test_a_fake_dependency_edge_changes_the_plan` is the
regression that proves a new edge reaches the plan without editing an expectation.
"""
import unittest
from ci_scope import ROOT, closure, identity_digest, packages, plan, render_lines, scope


class ScopeTests(unittest.TestCase):
    # The crates whose tests read a documentation fixture (`scripts/ci_scope.py` TEST_DOCS). This is
    # selector policy, not the manifest graph, so it stays pinned.
    READERS = ['iwbtg-format', 'iwbtg-sim']

    @classmethod
    def setUpClass(cls):
        cls.graph = packages(ROOT)

    def owners(self, path, graph=None):
        """The crates a path belongs to, from the same manifest graph the selector uses."""
        graph = graph or self.graph
        return {name for name, (prefix, _) in graph.items() if path.startswith(prefix)}

    def selected(self, *paths, graph=None):
        """The (test, compile) a change to `paths` selects, derived from the manifest graph."""
        graph = graph or self.graph
        owners = set().union(*(self.owners(path, graph) for path in paths)) if paths else set()
        return sorted(owners), sorted(closure(owners, graph) - owners)

    @staticmethod
    def packages_argv(names):
        return ' '.join('-p ' + name for name in sorted(names))

    def test_client_skips_sim_reexecution(self):
        path = 'crates/client/src/view.rs'
        test, compile_ = self.selected(path)
        result = scope([path], self.graph)
        self.assertEqual(result['packages'], self.packages_argv(set(test) | set(compile_)))
        self.assertNotIn('iwbtg-sim', test + compile_, 'a client change does not rerun the sim')
        self.assertTrue(result['browser'])
        self.assertTrue(result['web'], "a client change rebuilds the browser bundle")
        self.assertFalse(result['wasip1'])
        self.assertFalse(result['determinism'])

    def test_sim_includes_all_transitive_consumers(self):
        path = 'crates/sim/src/world.rs'
        test, compile_ = self.selected(path)
        result = scope([path], self.graph)
        self.assertEqual(result['packages'], self.packages_argv(set(test) | set(compile_)))
        self.assertEqual(set(test) | set(compile_), closure(test, self.graph),
                         'the plan selects exactly the changed crates and their consumers')
        self.assertTrue(result['wasip1'] and result['browser'] and result['determinism'])
        self.assertFalse(result['web'], "a sim change does not rebuild the browser bundle")

    def test_web_assets_select_the_bundle(self):
        path = 'crates/client/web/index.html'
        test, compile_ = self.selected(path)
        result = scope([path], self.graph)
        self.assertTrue(result['web'], 'the page is part of the client bundle')
        self.assertEqual(result['packages'], self.packages_argv(set(test) | set(compile_)))

    def test_server_does_not_build_bevy_or_wasm(self):
        path = 'crates/server/src/main.rs'
        test, compile_ = self.selected(path)
        result = scope([path], self.graph)
        self.assertEqual(result['packages'], self.packages_argv(set(test) | set(compile_)))
        # The server's consumers are compiled; its own dependencies are not selected. The
        # determinism report follows whether a determinism crate is selected (it does not when no
        # consumer is one). Still no Bevy and no wasm.
        self.assertNotIn('iwbtg-sim', test + compile_)
        determinism_crates = {'iwbtg-sim', 'iwbtg-format', 'iwbtg-tools'}
        self.assertEqual(result['determinism'],
                         bool((set(test) | set(compile_)) & determinism_crates))
        self.assertFalse(result['browser'] or result['wasip1'] or result['web'])

    def test_shared_unknown_deleted_and_ci_inputs_are_full(self):
        for path in ('Cargo.lock', 'Cargo.toml', 'crates/server/Cargo.toml', 'rust-toolchain.toml', '.cargo/config.toml',
                     '.github/workflows/ci.yml', 'scripts/ci_scope.py', 'crates/xtask/test-docs.txt', 'crates/removed/src/lib.rs',
                     'some-new-input'):
            with self.subTest(path=path):
                self.assertEqual(scope([path], self.graph)['packages'], scope([], self.graph, True)['packages'])

    def test_native_only_fixtures_keep_the_wasm_and_determinism_matrix_off(self):
        for path in ('docs/format/v0.md', 'docs/format/rooms.md',
                     'docs/format/schema/v0/game.schema.json',
                     'docs/format/examples/rely-physics.toml', 'docs/format/game.md'):
            with self.subTest(path=path):
                result = plan([path], self.graph)
                self.assertTrue(result['code'], 'the native readers still run locally')
                self.assertFalse(result['code_input'], 'a fixture is not a code input')
                self.assertEqual(result['test'], self.READERS)
                self.assertFalse(result['wasip1'])
                self.assertFalse(result['browser'])
                self.assertFalse(result['web'])
                self.assertFalse(result['determinism'])
        self.assertFalse(scope(['README.md', 'docs/dev-setup.md', 'docs/architecture/client/shell.md', '.claude/hooks/issues.py'], self.graph)['code'])

    def test_fixtures_keep_only_the_wasm_targets_that_read_them(self):
        # `iws.d.rhai` is include_str!'d by the golden corpus, which both wasm jobs run.
        both = plan(['docs/format/script/iws.d.rhai'], self.graph)
        self.assertTrue(both['wasip1'] and both['browser'])
        self.assertFalse(both['web'] or both['determinism'])
        # The script pages are read by the sim test binary, which wasm32-wasip1 runs; the
        # wasm32-unknown-unknown job runs only the determinism and golden-corpus binaries.
        for path in ('docs/format/scripts.md', 'docs/architecture/sim/scripts.md',
                     'docs/decisions/0001-script-language.md'):
            with self.subTest(path=path):
                result = plan([path], self.graph)
                self.assertTrue(result['wasip1'], path)
                self.assertFalse(result['browser'] or result['determinism'], path)

    def test_fixture_plus_code_keeps_the_full_matrix(self):
        result = plan(['docs/format/game.md', 'crates/sim/src/world.rs'], self.graph)
        self.assertTrue(result['code_input'])
        self.assertTrue(result['wasip1'] and result['browser'] and result['determinism'])
        self.assertFalse(result['web'])

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
        paths = ['crates/server/src/main.rs', 'crates/client/src/view.rs']
        test, compile_ = self.selected(*paths)
        result = scope(paths, self.graph)
        self.assertEqual(result['packages'], self.packages_argv(set(test) | set(compile_)))
        graph = {'a': ('crates/a/', set()), 'b': ('crates/b/', {'a'}), 'c': ('crates/c/', {'b'})}
        self.assertEqual(scope(['crates/a/deleted.rs'], graph)['packages'], '-p a -p b -p c')

    def test_plan_splits_changed_crates_from_transitive_consumers(self):
        # The split derives from the manifest graph: a changed crate is tested, its transitive
        # consumers are compile-checked. Historical edges (#1068 sim dev-depends on format, #1479
        # tools dev-depends on server) arrive through `closure` without being named here.
        for path in ('crates/sim/src/world.rs', 'crates/format/src/lib.rs', 'crates/client/src/view.rs',
                     'crates/tools/src/main.rs', 'crates/removal/src/lib.rs', 'crates/server/src/main.rs',
                     'crates/xtask/src/main.rs'):
            with self.subTest(path=path):
                test, compile_ = self.selected(path)
                result = plan([path], self.graph)
                self.assertEqual(result['test'], test)
                self.assertEqual(result['compile'], compile_)
                self.assertEqual(set(test) | set(compile_), closure(test, self.graph))
                self.assertEqual(set(result['test']) | set(result['compile']),
                                 set(result['packages_list']))
                self.assertFalse(result['full'])
                for step in result['local_steps'] + result['ci_steps']:
                    self.assertTrue(step['reason'], step)

    def test_a_fake_dependency_edge_changes_the_plan_without_editing_a_literal(self):
        # A synthetic graph: adding a consumer of `a` must change the plan with no expectation edit.
        graph = {'a': ('crates/a/', set()), 'b': ('crates/b/', {'a'})}
        before = plan(['crates/a/src/lib.rs'], graph)
        self.assertEqual(before['test'], ['a'])
        self.assertEqual(before['compile'], ['b'])
        grown = dict(graph) | {'c': ('crates/c/', {'a'})}
        after = plan(['crates/a/src/lib.rs'], grown)
        self.assertEqual(after['test'], ['a'])
        self.assertEqual(after['compile'], ['b', 'c'])
        # A consumer of that new consumer arrives transitively.
        deeper = dict(grown) | {'d': ('crates/d/', {'c'})}
        self.assertEqual(plan(['crates/a/src/lib.rs'], deeper)['compile'], ['b', 'c', 'd'])
        # The same holds on the real graph: a fake consumer of the sim joins the compile set.
        extended = dict(self.graph) | {'fake-consumer': ('crates/fake/', {'iwbtg-sim'})}
        self.assertIn('fake-consumer', plan(['crates/sim/src/lib.rs'], extended)['compile'])

    def test_fixture_only_native_job_tests_only_the_readers(self):
        fixture = plan(['docs/format/v0.md'], self.graph)
        # The readers are the TEST_DOCS policy; the consumers are derived from the manifest graph.
        consumers = sorted(closure(set(self.READERS), self.graph) - set(self.READERS))
        self.assertEqual(fixture['native_packages'], self.READERS)
        self.assertEqual(fixture['native_packages_argv'], self.packages_argv(self.READERS))
        self.assertEqual(fixture['compile'], consumers)
        self.assertEqual(set(fixture['test']) | set(fixture['compile']),
                         set(fixture['packages_list']))
        native = next(step for step in fixture['ci_steps'] if step['kind'] == 'native')
        self.assertEqual(native['packages'], fixture['native_packages'])
        # A code change still tests the consumers.
        sim = plan(['crates/sim/src/world.rs'], self.graph)
        self.assertEqual(sim['native_packages'], sim['packages_list'])

    def test_feel_step_follows_the_wasm_unknown_unknown_test_step(self):
        sim = plan(['crates/sim/src/world.rs'], self.graph)
        self.assertTrue(sim['feel'])
        feel = next(step for step in sim['ci_steps'] if step['kind'] == 'feel')
        self.assertTrue(feel['selected'] and feel['reason'])
        self.assertIn('feel=true', render_lines(sim))
        # A docs-only change selects no wasm job, so the feel step is off.
        native_docs = plan(['docs/format/v0.md'], self.graph)
        self.assertFalse(native_docs['feel'])
        skipped = next(step for step in native_docs['ci_steps'] if step['kind'] == 'feel')
        self.assertFalse(skipped['selected'])
        self.assertIn('feel=false', render_lines(native_docs))
        # A script-page fixture keeps only wasip1; the unknown-unknown job (and feel) stays off.
        self.assertFalse(plan(['docs/architecture/sim/scripts.md'], self.graph)['feel'])

    def test_plan_docs_and_full_selection(self):
        docs = plan(['docs/architecture/client/shell.md', 'README.md'], self.graph)
        self.assertFalse(docs['code'])
        self.assertEqual(docs['test'], [])
        self.assertEqual(docs['compile'], [])
        fixture = plan(['docs/format/game.md'], self.graph)
        self.assertEqual(fixture['test'], self.READERS)
        self.assertEqual(fixture['compile'],
                         sorted(closure(set(self.READERS), self.graph) - set(self.READERS)))
        self.assertTrue(fixture['code'])
        self.assertFalse(fixture['code_input'])
        self.assertFalse(fixture['wasip1'] or fixture['browser'] or fixture['web'] or fixture['determinism'])
        full = plan(['some-new-input'], self.graph)
        self.assertTrue(full['full'])
        self.assertEqual(set(full['test']), set(self.graph))
        self.assertEqual(full['compile'], [])
        self.assertTrue(full['wasip1'] and full['browser'] and full['web'] and full['determinism'])
        for item in full['inputs']:
            self.assertTrue(item['reason'])

    def test_plan_render_lines_is_a_stable_interface(self):
        result = plan(['crates/sim/src/world.rs'], self.graph)
        text = render_lines(result)
        # The field names and order are the interface; the values come from the plan.
        for field, value in (
            ('full', 'false'),
            ('code', 'true'),
            ('test', ','.join(result['test'])),
            ('compile', ','.join(result['compile'])),
            ('wasip1', 'true'),
            ('web', 'false'),
            ('packages', self.packages_argv(result['packages_list'])),
        ):
            self.assertIn(f'{field}={value}', text)
        self.assertIn('identity=', text)
        self.assertIn('step-identity=test:iwbtg-sim=', text)

    def test_step_identity_tracks_dependencies_not_consumers(self):
        import tempfile
        from pathlib import Path
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'Cargo.toml').write_text('[workspace]\nmembers = ["a", "b"]\n')
            (root / 'a/src').mkdir(parents=True)
            (root / 'a/Cargo.toml').write_text('[package]\nname = "a"\n')
            (root / 'a/src/lib.rs').write_text('a1')
            (root / 'b/src').mkdir(parents=True)
            (root / 'b/Cargo.toml').write_text('[package]\nname = "b"\n[dependencies]\na = { path = "../a" }\n')
            (root / 'b/src/lib.rs').write_text('b1')
            graph = packages(root)
            changed_a = ['a/src/lib.rs']
            ia = identity_digest(root, graph, 'test', ['a'], changed_a)
            ib = identity_digest(root, graph, 'test', ['b'], changed_a)
            # A consumer change does not invalidate the dependency's tests.
            (root / 'b/src/lib.rs').write_text('b2')
            both = ['a/src/lib.rs', 'b/src/lib.rs']
            self.assertEqual(identity_digest(root, graph, 'test', ['a'], both), ia)
            # A dependency change invalidates both the dependency and its consumer.
            (root / 'a/src/lib.rs').write_text('a2')
            self.assertNotEqual(identity_digest(root, graph, 'test', ['a'], both), ia)
            self.assertNotEqual(identity_digest(root, graph, 'test', ['b'], both), ib)
            # Shared lock/config/toolchain inputs invalidate every step.
            (root / 'Cargo.lock').write_text('lock1')
            before = identity_digest(root, graph, 'test', ['a'], both)
            (root / 'Cargo.lock').write_text('lock2')
            self.assertNotEqual(identity_digest(root, graph, 'test', ['a'], both), before)
            # Fingerprint environment variables are part of the identity.
            with patch.dict('os.environ', {'RUSTFLAGS': '-D warnings'}):
                flags_a = identity_digest(root, graph, 'test', ['a'], both)
            with patch.dict('os.environ', {'RUSTFLAGS': '--cfg changed'}):
                flags_b = identity_digest(root, graph, 'test', ['a'], both)
            self.assertNotEqual(flags_a, flags_b)

    def test_step_identity_sees_crate_sources_a_rebase_brings(self):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'Cargo.toml').write_text('[workspace]\nmembers = ["crates/format", "crates/app"]\n')
            (root / 'crates/format/src').mkdir(parents=True)
            (root / 'crates/format/Cargo.toml').write_text('[package]\nname = "iwbtg-format"\n')
            (root / 'crates/format/src/lib.rs').write_text('format v1')
            (root / 'crates/app/src').mkdir(parents=True)
            (root / 'crates/app/Cargo.toml').write_text(
                '[package]\nname = "iwbtg-app"\n[dependencies]\niwbtg-format = { path = "../format" }\n')
            (root / 'crates/app/src/lib.rs').write_text('app v1')
            graph = packages(root)
            changed = ['crates/app/src/lib.rs']  # equal in every call: the fixture-style list
            before = identity_digest(root, graph, 'test', ['iwbtg-format'], changed)
            # A rebase onto a new base replaces crates/format sources without naming them in
            # `changed`; the identity must still differ.
            (root / 'crates/format/src/lib.rs').write_text('format v2')
            after = identity_digest(root, graph, 'test', ['iwbtg-format'], changed)
            self.assertNotEqual(before, after,
                                'a base-brought dependency source must change the identity')
            # An unchanged-consumer edit does not enter the dependency's identity.
            format_identity = identity_digest(root, graph, 'test', ['iwbtg-format'], changed)
            (root / 'crates/app/src/lib.rs').write_text('app v2')
            self.assertEqual(identity_digest(root, graph, 'test', ['iwbtg-format'], changed),
                             format_identity)
            # An unchanged tree reproduces the same identity, so receipts stay reusable.
            self.assertEqual(identity_digest(root, graph, 'test', ['iwbtg-format'], changed),
                             format_identity)

    def test_local_and_ci_union_never_drifts(self):
        for paths in ([], ['README.md'], ['crates/sim/src/world.rs'], ['crates/client/src/view.rs'],
                      ['crates/format/src/lib.rs'], ['crates/removal/src/lib.rs'], ['Cargo.lock'],
                      ['docs/format/game.md']):
            with self.subTest(paths=paths):
                result = plan(paths, self.graph)
                self.assertEqual(set(result['test']) | set(result['compile']),
                                 set(result['packages_list']))
                self.assertEqual(result['code'], bool(result['packages_list']))


if __name__ == '__main__':
    unittest.main()
