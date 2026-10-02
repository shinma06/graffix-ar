import copy
import json
import io
from contextlib import redirect_stdout, redirect_stderr
from unittest.mock import patch
import pr_policy
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from git_guard import check_branch, check_push
from gui_lease import locked, transition
from pr_policy import validate

ROOT = Path(__file__).resolve().parents[2]
SHA = 'a' * 40
ZERO = '0' * 40


class GuardTest(unittest.TestCase):
    def setUp(self):
        self.git_env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}

    def test_commit_rejects_main_detached_and_no_issue(self):
        for branch in ['main', 'master', 'develop', '', 'codex/topic']:
            with self.assertRaises(ValueError):
                check_branch(branch)
        check_branch('codex/31-workflow')

    def test_push_checks_destination_not_current_branch(self):
        for dest in ['main', 'master', 'develop']:
            for sha in [SHA, ZERO]:
                with self.assertRaises(ValueError):
                    check_push([f'HEAD {sha} refs/heads/{dest} {SHA}'], 'codex/31-test', SHA, False)

    def test_tested_head_and_clean_tree_are_required(self):
        line = f'HEAD {SHA} refs/heads/codex/31-test {ZERO}'
        self.assertTrue(check_push([line], 'codex/31-test', SHA, False))
        for head, dirty in [('b' * 40, False), (SHA, True)]:
            with self.assertRaises(ValueError):
                check_push([line], 'codex/31-test', head, dirty)

    def test_branch_cleanup_cannot_delete_another_task(self):
        with self.assertRaises(ValueError):
            check_push([f'(delete) {ZERO} refs/heads/claude/99-other {SHA}'], 'codex/31-test', SHA, False)
        self.assertFalse(check_push([f'(delete) {ZERO} refs/heads/codex/31-test {SHA}'], 'codex/31-test', SHA, False))

    def test_real_commit_hook(self):
        with tempfile.TemporaryDirectory() as tmp:
            def git(*args):
                return subprocess.run(['git', *args], cwd=tmp, text=True, capture_output=True, env=self.git_env)
            self.assertEqual(git('init', '-b', 'main').returncode, 0)
            git('config', 'user.name', 'Test')
            git('config', 'user.email', 'test@example.invalid')
            # Install an actual hook pointing at the production guard in a disposable repo.
            hook = Path(tmp) / '.git/hooks/pre-commit'
            hook.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{ROOT}/scripts/workflow/git_guard.py" commit\n')
            hook.chmod(0o755)
            self.assertNotEqual(git('commit', '--allow-empty', '-m', 'blocked').returncode, 0)
            git('checkout', '-b', 'codex/31-test')
            self.assertEqual(git('commit', '--allow-empty', '-m', 'allowed').returncode, 0)

    def test_guard_rejects_history_rewrite_even_with_current_head(self):
        with tempfile.TemporaryDirectory() as tmp:
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=tmp, text=True, env=self.git_env).strip()
            git('init', '-b', 'codex/31-test')
            git('config', 'user.name', 'Test')
            git('config', 'user.email', 'test@example.invalid')
            git('commit', '--allow-empty', '-m', 'base')
            base = git('rev-parse', 'HEAD')
            git('commit', '--allow-empty', '-m', 'remote tip')
            remote = git('rev-parse', 'HEAD')
            git('checkout', '-b', 'codex/31-other', base)
            git('commit', '--allow-empty', '-m', 'diverged')
            head = git('rev-parse', 'HEAD')
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/workflow/git_guard.py'), 'push'],
                                    cwd=tmp, input=f'HEAD {head} refs/heads/codex/31-other {remote}\n',
                                    text=True, capture_output=True, env=self.git_env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Non-fast-forward', result.stderr)

    def test_foreign_repo_tests_ignore_hook_git_environment(self):
        with tempfile.TemporaryDirectory() as tmp:
            subprocess.run(['git', 'init', '-q', '-b', 'main', tmp], env=self.git_env, check=True)
            git_dir = Path(tmp) / '.git'
            before = (git_dir / 'config').read_bytes()
            contaminated = dict(self.git_env, GIT_DIR=str(git_dir), GIT_WORK_TREE=tmp)
            result = subprocess.run([sys.executable, '-m', 'unittest',
                                     'test_workflow.GuardTest.test_real_commit_hook',
                                     'test_workflow.GuardTest.test_guard_rejects_history_rewrite_even_with_current_head'],
                                    cwd=ROOT / 'scripts/workflow', env=contaminated,
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((git_dir / 'config').read_bytes(), before)


class LeaseTest(unittest.TestCase):
    def test_expiry_does_not_steal_and_stale_release_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'lease.json'
            first = transition(path, 'acquire', owner='a', issue=31, run='a', head=SHA)
            data = json.loads(path.read_text()); data['expires_at'] = 0
            path.write_text(json.dumps(data))
            self.assertTrue(transition(path, 'status')['expired'])
            with self.assertRaises(ValueError):
                transition(path, 'acquire', owner='b', issue=32, run='b', head=SHA)
            transition(path, 'release', token=first['token'])
            second = transition(path, 'acquire', owner='b', issue=32, run='b', head=SHA)
            for action in ['release', 'renew']:
                with self.assertRaises(ValueError):
                    transition(path, action, token=first['token'])
            self.assertEqual(transition(path, 'status')['token'], second['token'])

    def test_corruption_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'lease.json'
            for value in ['null', '{}', '{', '[]', '{"token":"a","owner":"x","expires_at":"bad"}']:
                path.write_text(value)
                for action in ['status', 'acquire']:
                    with self.assertRaises(ValueError):
                        transition(path, action, owner='x', issue=31, run='x', head=SHA)
                self.assertEqual(path.read_text(), value)

    def test_two_processes_have_exactly_one_winner(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = [sys.executable, str(ROOT / 'scripts/workflow/gui_lease.py'), '--state-dir', tmp,
                    'acquire', '--owner', 'test', '--issue', '31', '--run', 'test', '--head', SHA]
            processes = [subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for _ in range(2)]
            for process in processes:
                process.communicate(timeout=10)
            self.assertEqual(sorted(p.returncode for p in processes), [0, 1])

    def test_rejects_public_state_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp).chmod(0o755)
            with self.assertRaises(ValueError):
                with locked(Path(tmp)):
                    self.fail('public state accepted')


class PolicyTest(unittest.TestCase):
    def setUp(self):
        self.pr = {'head': {'ref': 'codex/31-test'}, 'base': {'ref': 'main'},
                   'body': 'Integration: tooling\nVerification: docs/verification/changes/issue-31.json\nIssue: #31\nGUI: not-required\nGUI reason: 運用規約とhookのみの変更。アプリ操作は不要。'}

    def test_matching_metadata(self):
        self.assertEqual(validate(self.pr), 31)

    def test_missing_mismatch_and_duplicate_are_rejected(self):
        for body in ['', self.pr['body'].replace('#31', '#32'), self.pr['body'] + '\nIssue: #31',
                     self.pr['body'].replace('not-required', 'passed'),
                     self.pr['body'].split('GUI reason:')[0] + 'GUI reason: <TODO reason>']:
            pr = copy.deepcopy(self.pr); pr['body'] = body
            with self.assertRaises(ValueError):
                validate(pr)

    def test_live_policy_entrypoints(self):
        workflow = (ROOT / '.github/workflows/pr-policy.yml').read_text()
        source = workflow.split("          python3 - <<'PY'\n", 1)[1].rsplit('          PY', 1)[0]
        namespace = {'__name__': 'inline_test'}
        exec(compile('\n'.join(line[10:] for line in source.splitlines()), '<workflow>', 'exec'), namespace)
        opened = dict(self.pr, number=112, state='open')
        closed = dict(opened, state='closed')
        invalid = dict(opened, body='invalid current metadata')
        issue = {'state': 'open', 'title': '[運用] policy',
                 'labels': ['type:maintenance', 'priority:P2', 'status:review']}
        done = dict(issue, state='closed', labels=['type:maintenance', 'priority:P2', 'status:done'])
        scenarios = [
            ('delayed merge', opened, [closed], 0, 'skipped'),
            ('event closed but reopened', closed, [opened, issue, opened], 0, 'passed'),
            ('reopened invalid metadata', closed, [invalid, invalid], 1, ''),
            ('reopened invalid issue', closed, [opened, done, opened], 1, ''),
            ('live metadata replaces event', invalid, [opened, issue, opened], 0, 'passed'),
            ('normal open', opened, [opened, issue, opened], 0, 'passed'),
            ('closed during issue fetch', opened, [opened, done, closed], 0, 'skipped'),
            ('closed after valid issue', opened, [opened, issue, closed], 0, 'skipped'),
            ('metadata changes mid-check', opened, [opened, issue, invalid], 1, ''),
            ('malformed initial object', opened, [None], 1, ''),
            ('malformed issue object', opened, [opened, []], 1, ''),
            ('malformed final object', opened, [opened, issue, None], 1, ''),
            ('initial api failure', opened, [OSError('offline')], 1, ''),
            ('issue api failure', opened, [opened, OSError('offline')], 1, ''),
            ('final api failure', opened, [opened, issue, OSError('offline')], 1, ''),
            ('unknown initial state', closed, [dict(opened, state='unknown')], 1, ''),
            ('unknown final state', opened, [opened, issue, dict(opened, state=None)], 1, ''),
            ('linked PR', opened, [opened, dict(issue, pull_request={}), opened], 1, ''),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            event_path = Path(tmp) / 'event.json'
            env = {'GITHUB_EVENT_PATH': str(event_path), 'GITHUB_REPOSITORY': 'test/repo', 'GITHUB_TOKEN': 'test'}
            for entry in (pr_policy.main, namespace['main']):
                for name, event, replies, expected, message in scenarios:
                    with self.subTest(entry=entry.__module__, scenario=name):
                        event_path.write_text(json.dumps({'pull_request': event}))
                        pending = iter(replies)
                        paths = []
                        def urlopen(request, timeout):
                            paths.append(request.full_url.split('/repos/test/repo/')[1])
                            response = next(pending)
                            if isinstance(response, Exception):
                                raise response
                            return io.StringIO(json.dumps(response))
                        out, err = io.StringIO(), io.StringIO()
                        with patch.dict(os.environ, env), patch.dict(entry.__globals__, urlopen=urlopen), redirect_stdout(out), redirect_stderr(err):
                            self.assertEqual(entry(), expected)
                        self.assertEqual(paths[0], 'pulls/112')
                        self.assertEqual(len(paths), len(replies))
                        if len(paths) == 3:
                            self.assertEqual(paths, ['pulls/112', 'issues/31', 'pulls/112'])
                        if message:
                            self.assertIn(message, out.getvalue())
                        if message == 'skipped':
                            self.assertIn('acceptance was not evaluated', out.getvalue())
                            self.assertNotIn('passed', out.getvalue())
                        if expected:
                            self.assertNotIn('passed', out.getvalue())
                            self.assertNotIn('skipped', out.getvalue())

    def test_inline_ci_validator_has_same_behavior(self):
        # The no-checkout CI job embeds this small validator; exercise the deployed copy too.
        workflow = (ROOT / '.github/workflows/pr-policy.yml').read_text()
        source = workflow.split("          python3 - <<'PY'\n", 1)[1].rsplit('          PY', 1)[0]
        source = '\n'.join(line[10:] for line in source.splitlines())
        namespace = {'__name__': 'inline_test'}
        exec(compile(source, '<workflow>', 'exec'), namespace)
        self.assertEqual(namespace['validate'](self.pr), 31)
        issue = {'title': '[運用] policy', 'state': 'open',
                 'labels': ['type:maintenance', 'priority:P2', 'status:review']}
        namespace['validate_issue'](issue)
        with self.assertRaises(ValueError):
            namespace['validate_issue'](dict(issue, labels=[]))
        bad = dict(self.pr, body='Issue: #99')
        with self.assertRaises(ValueError):
            namespace['validate'](bad)


if __name__ == '__main__':
    unittest.main()
