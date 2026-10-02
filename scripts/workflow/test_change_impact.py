"""Impact safety and real Git boundaries; no remote service or Xcode required."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import change_impact as ci


class ClassificationTest(unittest.TestCase):
    def test_required_eight_cases_and_semantic_exceptions(self):
        cases = [
            (['AGENTS.md'], {ci.KNOWLEDGE}, False),
            (['docs/design.md'], {ci.KNOWLEDGE}, False),
            (['Graffix-AR/kotlin/Service.kt'], {ci.RUNTIME}, True),
            (['Graffix-AR/res/layout/main.xml'], {ci.RUNTIME}, True),
            (['Graffix-AR.xcodeproj/project.pbxproj'], {ci.BUILD}, True),
            (['.github/workflows/pr-policy.yml'], {ci.TOOLING}, False),
            (['CLAUDE.md', 'Graffix-AR/Service.kt'], {ci.KNOWLEDGE, ci.RUNTIME}, True),
            (['new/input.unknown'], {ci.UNKNOWN}, True),
            (['Graffix-AR/resources/README.md'], {ci.RUNTIME}, True),
            (['Graffix-AR/App/Info.plist'], {ci.RUNTIME, ci.BUILD}, True),
            (['Graffix-ARTests/resources/input.json'], {ci.TEST}, True),
            (['docs/verification/changes/issue-198.json'], {ci.TOOLING}, False),
            (['.github/ISSUE_TEMPLATE/task.yml'], {ci.METADATA}, False),
            (['scripts/workflow/change_impact.py'], {ci.BUILD, ci.TOOLING}, True),
            (['scripts/workflow/ios_build.py'], {ci.BUILD, ci.TOOLING}, True),
            (['scripts/workflow/release_candidate.py'], {ci.BUILD, ci.TOOLING}, True),
            (['docs/codegen.yaml'], {ci.UNKNOWN}, True),
            (['scripts/new/code.kt'], {ci.UNKNOWN}, True),
            (['.github/new/input.bin'], {ci.UNKNOWN}, True),
        ]
        for paths, impacts, build in cases:
            with self.subTest(paths=paths):
                result = ci.classify([(p, ('100644',)) for p in paths])
                self.assertEqual(set(result['impacts']), impacts)
                self.assertEqual(result['ios_build'], build)
                self.assertTrue(result['tooling_test'])

    def test_executable_symlink_unknown_and_force_full_never_skip(self):
        for path, modes in [('docs/run.md', ('100644', '100755')), ('docs/link.md', ('120000',)),
                            ('../docs/note.md', ('100644',)), ('/docs/note.md', ('100644',))]:
            self.assertEqual(ci.path_impacts(path, modes), {ci.UNKNOWN})
        self.assertEqual(ci.path_impacts('AGENTS.md', ('120000',)), {ci.KNOWLEDGE})
        for result in (ci.classify([]), ci.classify([], reason='API/history failure'),
                       ci.classify([('README.md', ('100644',))], force_full=True)):
            self.assertTrue(all(result[x] for x in ('ios_build', 'tooling_test')))
            self.assertEqual(len(ci.test_commands(result)), 2)


class GitImpactTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = os.environ.copy()
        for key in subprocess.check_output(['git', 'rev-parse', '--local-env-vars'], text=True).splitlines():
            self.env.pop(key, None)
        self.env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1')
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Fixture'); self.git('config', 'user.email', 'fixture@example.invalid')
        self.write('Graffix-AR/resources/help.md', 'bundled runtime\n')
        self.write('docs/design.md', 'knowledge\n')
        self.base = self.commit()
        self.git('update-ref', 'refs/remotes/origin/develop', self.base)
        env_patch = patch.dict(os.environ, self.env, clear=True); env_patch.start(); self.addCleanup(env_patch.stop)

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, env=self.env, stderr=subprocess.PIPE).decode().strip()

    def write(self, name, content):
        file = self.root / name; file.parent.mkdir(parents=True, exist_ok=True); file.write_text(content)

    def commit(self):
        self.git('add', '.'); self.git('commit', '-m', 'fixture'); return self.git('rev-parse', 'HEAD')

    def test_doc_change_and_full_pr_range_after_later_doc_commit(self):
        self.write('docs/design.md', 'changed\n'); docs = self.commit()
        self.assertFalse(ci.git_impact(self.base, docs, cwd=self.root)['ios_build'])
        self.write('Graffix-AR/Service.kt', 'class Service\n'); self.commit()
        self.write('docs/design.md', 'later knowledge\n'); head = self.commit()
        self.assertTrue(ci.git_impact(None, head, cwd=self.root)['ios_build'])

    def test_rename_runtime_to_docs_and_deletion_keep_runtime_impact(self):
        self.git('mv', 'Graffix-AR/resources/help.md', 'docs/help.md'); head = self.commit()
        result = ci.git_impact(self.base, head, cwd=self.root)
        self.assertIn(ci.RUNTIME, result['impacts']); self.assertIn(ci.KNOWLEDGE, result['impacts'])
        self.assertEqual(len(result['files']), 2)
        self.git('reset', '--hard', self.base)  # Disposable fixture only.
        self.git('rm', 'Graffix-AR/resources/help.md'); head = self.commit()
        self.assertTrue(ci.git_impact(self.base, head, cwd=self.root)['ios_build'])

    def test_chmod_unicode_and_newline_paths_are_not_lost(self):
        self.write('docs/日本語\nnotes.md', 'text\n')
        (self.root / 'docs/design.md').chmod(0o755); head = self.commit()
        result = ci.git_impact(self.base, head, cwd=self.root)
        self.assertIn(ci.UNKNOWN, result['impacts']); self.assertTrue(result['ios_build'])
        self.assertEqual({x['path'] for x in result['files']}, {'docs/design.md', 'docs/日本語\nnotes.md'})

    def test_missing_history_nonancestor_and_git_failure_are_full(self):
        self.assertTrue(ci.git_impact('0' * 40, self.base, cwd=self.root)['ios_build'])
        self.write('docs/design.md', 'one\n'); first = self.commit()
        self.git('checkout', '-b', 'other', self.base)
        self.write('docs/design.md', 'two\n'); second = self.commit()
        result = ci.git_impact(first, second, cwd=self.root, merge_base=False)
        self.assertTrue(result['ios_build']); self.assertIn(ci.UNKNOWN, result['impacts'])
        with patch.object(ci, 'git', side_effect=OSError('unavailable')):
            self.assertTrue(ci.git_impact(self.base, second, cwd=self.root)['ios_build'])

    def test_gitlinks_are_visible_even_when_git_config_ignores_submodules(self):
        self.git('config', 'diff.ignoreSubmodules', 'all')
        previous = self.base
        for operation in ('add', 'update', 'delete'):
            if operation == 'delete':
                self.git('update-index', '--force-remove', 'vendor/module')
            else:
                self.git('update-index', '--add', '--cacheinfo', '160000,' + previous + ',vendor/module')
            self.write('docs/design.md', operation + '\n'); self.git('add', 'docs/design.md')
            self.git('commit', '-m', operation); head = self.git('rev-parse', 'HEAD')
            result = ci.git_impact(previous, head, cwd=self.root)
            with self.subTest(operation=operation):
                self.assertEqual({f['path'] for f in result['files']}, {'vendor/module', 'docs/design.md'})
                self.assertIn(ci.UNKNOWN, result['impacts'])
                self.assertTrue(result['ios_build']); self.assertTrue(result['ios_build'])
            previous = head


    def test_test_runner_rejects_dirty_or_different_checkout(self):
        self.write('docs/design.md', 'dirty\n')
        command = ['python3', str(Path(ci.__file__).resolve()), '--base', self.base, '--run-tests']
        dirty = subprocess.run(command, cwd=self.root, env=self.env, capture_output=True)
        self.assertNotEqual(dirty.returncode, 0)
        self.commit()
        other = subprocess.run(command + ['--head', self.base], cwd=self.root, env=self.env, capture_output=True)
        self.assertNotEqual(other.returncode, 0)

    def test_push_and_pr_events_emit_successful_skip_and_manual_full(self):
        self.write('docs/design.md', 'updated\n'); head = self.commit()
        script = str(Path(ci.__file__).resolve())
        for event_name, data, expected in [('pull_request', {'pull_request': {'base': {'sha': self.base}}}, False),
                                           ('push', {'before': self.base}, False), ('workflow_dispatch', {}, True)]:
            event = self.root / '.git/event.json'; event.write_text(json.dumps(data))
            output = self.root / '.git/output'; output.write_text('')
            summary = self.root / '.git/summary'; summary.write_text('')
            env = dict(self.env, GITHUB_EVENT_NAME=event_name, GITHUB_EVENT_PATH=str(event),
                       GITHUB_OUTPUT=str(output), GITHUB_STEP_SUMMARY=str(summary))
            subprocess.run(['python3', script, '--event', '--head', head], cwd=self.root,
                           env=env, check=True, capture_output=True)
            self.assertIn('ios_build=' + str(expected).lower(), output.read_text())
            self.assertIn('Change Impact', summary.read_text())


if __name__ == '__main__':
    unittest.main()
