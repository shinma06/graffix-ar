import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch
from types import SimpleNamespace

import agent_loop as al
from agent_policy import binding, next_action
from handoff_registry import register, resolve
from verification import metadata, validate_change, verify_pr, render_queue
from test_agent_loop import pr_data, report, HEAD, BASE, NEW
import test_agent_loop as tal


CASE = {'id': 'QA-1', 'change': '日本語表示', 'preconditions': '固定候補ZIPをロードする',
        'steps': ['設定を開く', '保存して開き直す'], 'expected': '設定を保持する',
        'provenance': ['https://github.com/example/issues/1'], 'next_action': '人間がまとめて確認',
        'gpt': {'status': 'blocked', 'reason': '環境が画面を取得できない'},
        'human': {'status': 'pending', 'reason': '候補未確認'}, 'fix_issue': None, 'fix_pr': None,
        'recheck': '修正後は新候補buildで再確認'}


def change(issue=35, gui=True):
    return {'schema': 1, 'issue': issue, 'gui_required': gui, 'reason': '設定表示の変更',
            'cli_checks': ['python3 scripts/workflow/ios_build.py: success'], 'cases': [copy.deepcopy(CASE)] if gui else []}


def observation():
    return {'status': 'pass', 'head': NEW, 'artifact_sha256': 'e' * 64, 'actor': 'human',
            'observer': 'reviewer-1', 'at': '2026-09-07T10:00:00+09:00', 'evidence': 'https://example.invalid/evidence',
            'loaded_identity': 'ZIP and loaded JAR verified', 'reason': '設定が保持された'}


class AcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.pr = pr_data()
        self.pr['body'] = self.pr['body'].replace('tooling', 'promotion')
        self.manifest = {'schema': 1, 'base': BASE, 'candidate': NEW, 'artifact_sha256': 'e' * 64,
                         'changes': [{'commit': NEW, 'pr': 99}], 'results': {'36:QA-1': observation()}}
        self.source = copy.deepcopy(self.pr)
        self.source.update(merged=True, merge_commit_sha=NEW)
        self.source['base']['ref'] = 'develop'
        self.source['body'] = ('Issue: #36\nGUI: required\nIntegration: develop\n'
                               'Verification: docs/verification/changes/issue-36.json')
        self.source['head']['ref'] = 'codex/36-change'
        self.documents = {f'{HEAD}:docs/verification/changes/issue-35.json': change(gui=False),
                          f'{NEW}:docs/verification/changes/issue-36.json': change(36)}
        self.range = [NEW]
        self.product_diff = []

    def git(self, *args):
        if args[0] == 'show':
            if args[1].endswith(':docs/verification/promotion.json'):
                return json.dumps(self.manifest)
            return json.dumps(self.documents[args[1]])
        if args[0] == 'diff':
            if args[2] == NEW:
                return '\n'.join(self.product_diff + ['docs/verification/promotion.json'])
            return 'docs/verification/promotion.json'
        if args[:2] == ('rev-list', '--parents'):
            return args[-1] + ' ' + BASE
        if args[0] == 'rev-list':
            return '' if '--not' in args else '\n'.join(self.range)
        if args[0] == 'fetch':
            return ''
        if args[0] == 'merge-base':
            return ''
        raise AssertionError(args)

    def api(self, path):
        if path.startswith('issues/'):
            return {'state': 'open'}
        return {'object': {'sha': NEW}} if path.startswith('git/ref') else self.source

    def verify(self):
        return verify_pr(self.pr, self.api, self.git)

    def test_all_fixed_candidate_cases_allow_gpt_or_human(self):
        for actor in ('gpt', 'human'):
            self.manifest['results']['36:QA-1']['actor'] = actor
            self.assertEqual(self.verify()['cases'], 1)

    def test_one_pass_cannot_promote_two_commits(self):
        self.range.append('f' * 40)
        with self.assertRaisesRegex(ValueError, 'EVERY'):
            self.verify()

    def test_missing_required_case_rejected(self):
        self.documents[f'{NEW}:docs/verification/changes/issue-36.json']['cases'].append(dict(CASE, id='QA-2'))
        with self.assertRaisesRegex(ValueError, 'ALL'):
            self.verify()

    def test_old_build_unknown_blocked_failed_or_unsigned_results_rejected(self):
        original = copy.deepcopy(self.manifest)
        for patch_value in ({'head': HEAD}, {'artifact_sha256': 'a' * 64}, {'status': 'blocked'},
                            {'status': 'pending'}, {'status': 'fail'}, {'observer': ''}, {'at': ''}, {'evidence': ''}):
            self.manifest = copy.deepcopy(original)
            self.manifest['results']['36:QA-1'].update(patch_value)
            with self.assertRaises(ValueError):
                self.verify()

    def test_untested_product_change_in_promotion_rejected(self):
        self.product_diff = ['src/main/kotlin/Product.kt']
        with self.assertRaisesRegex(ValueError, 'tree differs'):
            self.verify()

    def test_closed_origin_does_not_weaken_or_block_fixed_candidate_gate(self):
        original = self.api
        def api(path):
            if path.startswith('issues/'):
                return {'state': 'closed', 'labels': [{'name': 'status:done'}]}
            return original(path)
        self.api = api
        self.assertEqual(self.verify()['mode'], 'promotion')

    def test_wrong_source_merge_base_or_moving_develop_rejected(self):
        self.source['merge_commit_sha'] = HEAD
        with self.assertRaises(ValueError): self.verify()
        self.source['merge_commit_sha'] = NEW
        self.manifest['base'] = HEAD
        with self.assertRaises(ValueError): self.verify()
        self.manifest['base'] = BASE
        original_git = self.git
        self.git = lambda *args: (_ for _ in ()).throw(ValueError('not ancestor')) if args == ('merge-base', '--is-ancestor', NEW, 'f' * 40) else original_git(*args)
        self.api = lambda _: {'object': {'sha': 'f' * 40}}
        with self.assertRaisesRegex(ValueError, 'not ancestor'): self.verify()

    def test_develop_allows_pending_blocked_and_tracked_product_fail(self):
        self.pr['base']['ref'] = 'develop'
        self.pr['body'] = self.pr['body'].replace('promotion', 'develop').replace('GUI: not-required', 'GUI: required')
        for status in ('pending', 'blocked', 'fail'):
            data = change()
            data['cases'][0]['gpt']['status'] = status
            data['cases'][0]['fix_issue'] = 100 if status == 'fail' else None
            self.documents[f'{HEAD}:docs/verification/changes/issue-35.json'] = data
            self.assertEqual(self.verify()['mode'], 'develop')
        data['cases'][0]['fix_issue'] = None
        with self.assertRaisesRegex(ValueError, 'dedicated fix'): self.verify()

    def test_missing_matrix_repro_steps_or_no_gui_contradiction_rejected(self):
        for mutation in ({'cases': []}, {'gui_required': False}):
            with self.assertRaises(ValueError): validate_change(dict(change(), **mutation), 35, True)
        data = change(); data['cases'][0]['steps'] = []
        with self.assertRaises(ValueError): validate_change(data, 35, True)

    def test_direct_main_readme_only_and_mixed_product_paths(self):
        self.pr['body'] = self.pr['body'].replace('promotion', 'tooling')
        for extra in ('', 'Graffix-AR/App/Product.swift', 'Graffix-AR.xcodeproj/project.pbxproj', 'README.md/Product.kt', 'README.kt'):
            with self.subTest(extra=extra):
                files = '\n'.join(filter(None, ['README.md', 'docs/verification/changes/issue-35.json', extra]))
                git = lambda *args: files if args[0] == 'diff' else self.git(*args)
                if extra:
                    with self.assertRaisesRegex(ValueError, 'restricted'):
                        verify_pr(self.pr, self.api, git)
                else:
                    self.assertEqual(verify_pr(self.pr, self.api, git)['mode'], 'tooling')

    def test_direct_main_product_no_gui_route_rejected(self):
        self.pr['body'] = self.pr['body'].replace('promotion', 'tooling')
        with self.assertRaises(ValueError):
            verify_pr(self.pr, self.api, lambda *args: 'Graffix-AR/App/Product.swift' if args[0] == 'diff' else self.git(*args))

    def test_target_change_invalidates_review(self):
        bound = binding(self.pr, {'body': ''})
        other = copy.deepcopy(self.pr); other['base']['ref'] = 'develop'
        self.assertNotEqual(bound, binding(other, {'body': ''}))
        state = {'review': report(), 'binding': bound}
        self.assertEqual(next_action(state, bound, 'failure', True, {'allowed': True}), 'ci-failed')
        self.assertEqual(next_action(state, bound, 'success', True, {'allowed': True}), 'merge')
        self.assertEqual(next_action(state, bound, 'success', False, {'allowed': False}), 'acceptance-wait')

    def test_probe_artifact_and_cua_execution_are_not_substitutable(self):
        case = self.documents[f'{NEW}:docs/verification/changes/issue-36.json']['cases'][0]
        case.update(artifact='swing_probe', required_execution='computer_use')
        with self.assertRaises(ValueError): self.verify()
        self.manifest['artifacts'] = {'swing_probe': 'f' * 64}
        result = self.manifest['results']['36:QA-1']
        result.update(artifact_sha256='f' * 64, execution='manual')
        with self.assertRaisesRegex(ValueError, 'execution'): self.verify()
        result['execution'] = 'computer_use'
        self.assertEqual(self.verify()['cases'], 1)

    def test_current_candidate_results_render_observer_and_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'case.json'; path.write_text(json.dumps(change(36)))
            output = render_queue([path], self.manifest)
            self.assertIn('reviewer-1', output)
            self.assertIn('https://example.invalid/evidence', output)
            self.assertIn('Case合格', output)

    def test_public_queue_contains_steps_and_never_promotes_historical_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'case.json'; path.write_text(json.dumps(change()))
            output = render_queue([path])
            self.assertIn('1. 設定を開く', output)
            self.assertIn('固定候補のpass未登録', output)
            self.assertIn('初期登録時のAgent（互換キーgpt）:', output)
            self.assertIn('初期登録時の人間:', output)


class RegistryTests(unittest.TestCase):
    def test_opaque_roundtrip_and_tampering_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            public = register(tmp, {'pr': 12, 'scope': ['src/'], 'owner': 'agent-loop'}, '/private/worktree', 'private-host')
            self.assertNotIn('private', json.dumps(public))
            self.assertEqual(resolve(tmp, public, 'private-host')['source'], '/private/worktree')
            for altered, host in ((dict(public, scope=['/']), 'private-host'), (public, 'other-host'),
                                  (dict(public, registry_id='../escape'), 'private-host')):
                with self.assertRaises(ValueError): resolve(tmp, altered, host)
            path = Path(tmp) / 'registry' / (public['registry_id'] + '.json')
            path.chmod(0o644)
            with self.assertRaises(ValueError): resolve(tmp, public, 'private-host')
            path.unlink()
            with self.assertRaises(ValueError): resolve(tmp, public, 'private-host')

    def test_unknown_new_registry_cannot_fall_back_to_legacy_writer(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(al, 'git', return_value=tmp):
            gh = Mock()
            gh.comments.return_value = [
                {'id': 1, 'user': {'login': al.OWNER}, 'body': al.pack(al.HANDOFF, {'host': al.HOST, 'source': '/old'})},
                {'id': 2, 'user': {'login': al.OWNER}, 'body': al.pack(al.HANDOFF_V2, {'version': 2, 'registry_id': 'a' * 32})}]
            loop = al.Loop(gh, tmp)
            with self.assertRaises(ValueError): loop.load(12)

    def test_no_local_paths_or_host_in_public_state_and_errors(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(al, 'git', return_value=tmp):
            loop = al.Loop(Mock(), tmp)
            data = loop.public_state({'source': '/private/worktree', 'host': al.HOST,
                                      'next': f'failed {tmp}/secret.log on {al.HOST}'})
            self.assertNotIn(tmp, json.dumps(data)); self.assertNotIn(al.HOST, json.dumps(data))
            reason = loop.private_error(RuntimeError('/private/token.log raw content'))
            self.assertNotIn('raw content', reason)

    def test_cleanup_explicitly_protects_main_and_develop(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(al, 'git', return_value=tmp):
            loop = al.Loop(Mock(), tmp)
            for branch in ('main', 'master', 'develop'):
                with self.assertRaisesRegex(ValueError, 'never be cleaned'):
                    loop._cleanup_resources(pr_data(), {'branch': branch})


class DevelopLoopTests(unittest.TestCase):
    setUp = tal.LoopTests.setUp
    # Reuse the real transition fixture without duplicating inherited scenario discovery.
    def test_develop_tracking_merge_keeps_issue_open(self):
        self.gh.issues[35].update(title="[追跡] parent", labels=["type:tracking", "priority:P2", "status:review"])
        self.gh.pull['base']['ref'] = 'develop'
        self.gh.pull['body'] = self.gh.pull['body'].replace('tooling', 'develop')
        h, _, _, _ = self.loop.load(36)
        h['target'] = 'develop'
        self.gh.messages[36][0]['body'] = al.pack(al.HANDOFF, h)
        original_git = al.git.side_effect
        al.git.side_effect = lambda *args, **kw: BASE if args[:2] == ('rev-parse', 'origin/develop') else original_git(*args, **kw)
        self.loop.acceptance.return_value = {'allowed': True, 'mode': 'develop', 'cases': 1}
        self.loop.tick(36)
        self.assertEqual(self.loop.tick(36)['phase'], 'done')
        self.assertEqual(self.gh.issues[35]['state'], 'open')
        self.assertEqual(self.gh.issues[1]['body'], '- [ ] #35 task')

    def test_promotion_completion_uses_candidate_acceptance_not_old_gui_shape(self):
        self.gh.pull['body'] = self.gh.pull['body'].replace('tooling', 'promotion')
        h, _, _, _ = self.loop.load(36)
        h['gui_required'] = True
        self.gh.messages[36][0]['body'] = al.pack(al.HANDOFF, h)
        self.worker.side_effect = lambda *a, **kw: dict(report(), gui_required=True)
        self.loop.acceptance.return_value = {'allowed': True, 'mode': 'promotion', 'cases': 1,
                                            'gui_complete': True, 'candidate': NEW}
        self.loop.tick(36)
        self.assertEqual(self.loop.tick(36)['phase'], 'done')
        self.assertEqual(self.gh.issues[35]['state'], 'closed')
        _, state, _, _ = self.loop.load(36)
        self.assertNotIn('gui', state)
        self.assertEqual(state['acceptance']['candidate'], NEW)

    def test_new_enrollment_publishes_only_opaque_registry_reference(self):
        self.gh.messages.clear()
        original_git = al.git.side_effect
        al.git.side_effect = lambda *args, **kw: 'codex/35-test' if args == ('branch', '--show-current') else original_git(*args, **kw)
        args = SimpleNamespace(pr=36, source=self.checkout, owner='gpt-test-session', scope=['src/'],
                    parent=1, close_issue=False, writer_stopped=True)
        public = al.enroll(self.loop, args)
        self.assertEqual(public['version'], 2)
        encoded = json.dumps(self.gh.messages)
        self.assertNotIn(str(self.checkout), encoded)
        self.assertNotIn(al.HOST, encoded)
        h, _, _, _ = self.loop.load(36)
        self.assertEqual(h['source'], str(self.checkout.resolve()))
        with self.assertRaises(ValueError): al.enroll(self.loop, args)

    def test_rebind_legacy_target_keeps_owner_private_and_invalidates_review(self):
        self.loop.tick(36)
        h, _, _, _ = self.loop.load(36)
        h['owner'] = 'agent-loop@' + al.HOST
        self.gh.messages[36][0]['body'] = al.pack(al.HANDOFF, h)
        self.gh.pull['base']['ref'] = 'develop'
        self.gh.pull['body'] = self.gh.pull['body'].replace('tooling', 'develop')
        public = al.rebind_target(self.loop, SimpleNamespace(pr=36, writer_stopped=True, source=None, migration_record=None))
        self.assertNotIn(al.HOST, json.dumps(public))
        self.assertNotIn('source', public)
        resolved, state, _, _ = self.loop.load(36)
        self.assertEqual(resolved['source'], str(self.checkout))
        self.assertEqual(resolved['target'], 'develop')
        self.assertNotIn('review', state)
        self.assertNotIn('binding', state)

    def test_explicit_source_rebind_preserves_original_head_and_managed_expected(self):
        h, state, sid, _ = self.loop.load(36)
        original_head = 'e' * 40
        h['head'] = original_head
        h['previous_owner'] = 'gpt-original-writer'
        self.gh.messages[36][0]['body'] = al.pack(al.HANDOFF, h)
        self.loop.save(self.gh.pull, dict(state, expected_head=HEAD), sid)
        moved = self.checkout.parent / 'relocated'
        with patch('source_relocation.relocated_source', return_value=moved) as relocation:
            public = al.rebind_target(self.loop, SimpleNamespace(pr=36, source=moved, migration_record=Path('record')))
        relocation.assert_called_once()
        resolved, state, _, _ = self.loop.load(36)
        self.assertEqual(resolved['source'], str(moved))
        self.assertEqual(public['head'], original_head)
        self.assertEqual(state['expected_head'], HEAD)
        self.assertEqual(public['previous_owner'], h['previous_owner'])
        self.assertNotIn(str(moved), json.dumps(self.gh.messages))

    def test_rebind_missing_source_does_not_silently_reregister(self):
        h, _, _, _ = self.loop.load(36)
        h['source'] = str(self.checkout.parent / 'missing')
        self.gh.messages[36][0]['body'] = al.pack(al.HANDOFF, h)
        with self.assertRaisesRegex(ValueError, 'Missing source'):
            al.rebind_target(self.loop, SimpleNamespace(pr=36, source=None, migration_record=None))

    def test_rebind_cannot_adopt_foreign_head_or_running_worker(self):
        self.gh.pull['head']['sha'] = NEW
        with self.assertRaises(ValueError): al.rebind_target(self.loop, SimpleNamespace(pr=36, source=None, migration_record=None))
        self.gh.pull['head']['sha'] = HEAD
        (self.checkout.parent / 'worker.json').write_text('{}')
        with self.assertRaises(ValueError): al.rebind_target(self.loop, SimpleNamespace(pr=36, source=None, migration_record=None))


class RealPromotionHistoryTests(unittest.TestCase):
    def test_two_batches_with_later_develop_and_main_tooling_sync(self):
        import os
        import subprocess
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=root, env=env, text=True, stderr=subprocess.DEVNULL).strip()
            git('init', '-b', 'main'); git('config', 'user.name', 'Test'); git('config', 'user.email', 'test@example.invalid')
            git('config', 'core.hooksPath', '/dev/null')
            # Fetch uses this disposable repo only, never the network.
            git('remote', 'add', 'origin', str(root))
            def write(path, data):
                target = root / path; target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(json.dumps(data) if isinstance(data, dict) else data)
            def commit(message):
                git('add', '.'); git('commit', '-m', message); return git('rev-parse', 'HEAD')
            write('product.txt', 'base'); base = commit('base')
            git('checkout', '-b', 'develop')
            sources = {}
            def source_commit(issue, number, gui=True):
                write(f'docs/verification/changes/issue-{issue}.json', change(issue, gui))
                sha = commit(f'change {issue}')
                source = pr_data(); source.update(number=number, merged=True, merge_commit_sha=sha)
                source['base']['ref'] = 'develop'; source['head']['ref'] = f'codex/{issue}-feature'
                source['body'] = (f'Issue: #{issue}\nIntegration: develop\nGUI: {"required" if gui else "not-required"}\n'
                                  f'Verification: docs/verification/changes/issue-{issue}.json')
                sources[number] = source
                return sha
            write('product.txt', 'batch 1'); first = source_commit(36, 99)
            # Development continues while the FIRST candidate is under observation.
            write('product.txt', 'batch 2'); later = source_commit(37, 100)
            def api(path):
                return {'object': {'sha': git('rev-parse', 'develop')}} if path.startswith('git/ref') else sources[int(path.split('/')[-1])]
            def promotion(issue, candidate, main, covered, keys):
                git('checkout', '-b', f'codex/{issue}-promotion', candidate)
                git('merge', '--no-edit', main)
                data = {'schema': 1, 'base': main, 'candidate': candidate, 'artifact_sha256': 'e' * 64,
                        'changes': covered, 'results': {key: dict(observation(), head=candidate) for key in keys}}
                write('docs/verification/promotion.json', data)
                write(f'docs/verification/changes/issue-{issue}.json', change(issue, False))
                head = commit('record candidate observations')
                pr = pr_data(); pr['head'].update(sha=head, ref=f'codex/{issue}-promotion'); pr['base']['sha'] = main
                pr['body'] = (f'Issue: #{issue}\nIntegration: promotion\nGUI: not-required\n'
                              f'Verification: docs/verification/changes/issue-{issue}.json')
                return pr
            one = promotion(35, first, base, [{'commit': first, 'pr': 99}], ['36:QA-1'])
            self.assertEqual(verify_pr(one, api, git)['candidate'], first)
            # An unobserved product change after candidate is never accepted as metadata.
            write('product.txt', 'untested'); untested = commit('hidden product update')
            bad = copy.deepcopy(one); bad['head']['sha'] = untested
            with self.assertRaisesRegex(ValueError, 'tree differs'): verify_pr(bad, api, git)
            # Even net-identical product content cannot hide a later develop commit in ancestry.
            git('checkout', '-b', 'codex/41-history-smuggling', one['head']['sha'])
            git('merge', '--no-edit', later)
            write('product.txt', 'batch 1'); disguised = commit('restore candidate tree')
            # Also restore the later Case JSON so the final tree contains only permitted metadata.
            git('rm', 'docs/verification/changes/issue-37.json'); disguised = commit('hide extra case')
            bad = copy.deepcopy(one); bad['head']['sha'] = disguised
            with self.assertRaisesRegex(ValueError, 'history|ancestry'): verify_pr(bad, api, git)
            git('checkout', 'main'); git('merge', '--no-ff', '--no-edit', one['head']['sha'])
            # main progresses with an unrelated tooling change, then is synchronized via a develop squash PR.
            write('docs/main-tooling.md', 'new tooling'); main_two = commit('main tooling')
            git('checkout', 'develop'); git('merge', '--squash', 'main')
            synced = source_commit(38, 101, gui=False)
            two = promotion(40, synced, main_two, [{'commit': later, 'pr': 100}, {'commit': synced, 'pr': 101}], ['37:QA-1'])
            self.assertEqual(verify_pr(two, api, git)['cases'], 1)
            # Reusing batch one's old candidate after it already reached main is rejected (empty range).
            older = copy.deepcopy(two)
            data = json.loads((root / 'docs/verification/promotion.json').read_text())
            data.update(candidate=first, changes=[], results={})
            write('docs/verification/promotion.json', data); older['head']['sha'] = commit('invalid old candidate')
            with self.assertRaises(ValueError): verify_pr(older, api, git)


class AcceptanceCITests(unittest.TestCase):
    def test_stale_event_sha_cannot_run_an_old_validator_successfully(self):
        import acceptance_ci as ci
        import io
        import os
        with tempfile.TemporaryDirectory() as tmp:
            event = Path(tmp) / 'event.json'
            pr = pr_data(); event.write_text(json.dumps({'pull_request': pr}))
            responses = [pr, {'object': {'sha': NEW}}, pr]
            with patch.dict(os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_REPOSITORY=al.REPO, GITHUB_TOKEN='test'), \
                    patch.object(ci, 'urlopen', side_effect=lambda *a, **kw: io.BytesIO(json.dumps(responses.pop(0)).encode())), \
                    patch.object(ci.subprocess, 'check_output', return_value=BASE), \
                    patch.object(ci.subprocess, 'run') as execute, patch.object(ci, 'verify_pr') as verify:
                with self.assertRaisesRegex(ValueError, 'validator checkout is stale'): ci.main()
                execute.assert_not_called(); verify.assert_not_called()

    def test_current_validator_accepts_stale_event_base_only_after_live_binding(self):
        import acceptance_ci as ci
        import io
        import os
        with tempfile.TemporaryDirectory() as tmp:
            event = Path(tmp) / 'event.json'
            pr = pr_data(); event.write_text(json.dumps({'pull_request': pr}))
            responses = [pr, {'object': {'sha': NEW}}, pr, {'object': {'sha': NEW}}]
            with patch.dict(os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_REPOSITORY=al.REPO, GITHUB_TOKEN='test'), \
                    patch.object(ci, 'urlopen', side_effect=lambda *a, **kw: io.BytesIO(json.dumps(responses.pop(0)).encode())), \
                    patch.object(ci.subprocess, 'check_output', return_value=NEW), \
                    patch.object(ci.subprocess, 'run'), patch.object(ci, 'verify_pr', return_value={'mode': 'tooling'}) as verify, \
                    patch('builtins.print'):
                ci.main()
                self.assertEqual(verify.call_args.args[0]['base']['sha'], NEW)
