"""Real Git histories exercise the main-scoped trust boundary without publishing."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from verification import scoped_candidate, verify_pr
from test_verification import change, observation


class ScopedPromotionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@example.invalid')
        self.issue = 35
        self.case_path = 'docs/verification/changes/issue-35.json'
        self.plan_path = 'docs/verification/scopes/issue-35.json'
        self.acceptance = change()
        self.plan = {'schema': 1, 'issue': 35, 'files': {'Graffix-AR.xcodeproj/project.pbxproj': '100644'}, 'acceptance': self.acceptance}
        self.write(self.plan_path, self.plan)
        self.write('Graffix-AR.xcodeproj/project.pbxproj', 'old')
        self.base = self.commit()
        self.write('Graffix-AR.xcodeproj/project.pbxproj', 'modern')
        self.candidate = self.commit()
        result = observation(); result['head'] = self.candidate
        self.promotion = {'schema': 1, 'scope': 'main', 'base': self.base, 'candidate': self.candidate,
                          'artifact_sha256': 'e' * 64, 'results': {'35:QA-1': result}}
        self.pr = {'number': 1, 'body': 'Issue: #35\nGUI: required\nIntegration: promotion\nVerification: '+self.case_path,
                   'base': {'ref': 'main', 'sha': self.base}, 'head': {'sha': self.candidate}}
        self.results_commit()

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, text=True, stderr=subprocess.PIPE).strip()

    def write(self, name, value):
        p = self.root / name; p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(value) if isinstance(value, dict) else value)

    def commit(self):
        self.git('add', '.'); self.git('commit', '-qm', 'test')
        return self.git('rev-parse', 'HEAD')

    def results_commit(self):
        self.write(self.case_path, self.acceptance)
        self.write('docs/verification/promotion.json', self.promotion)
        self.pr['head']['sha'] = self.commit()

    def verify(self):
        def api(_): raise AssertionError('Scoped verification must not consult moving develop')
        return verify_pr(self.pr, api, self.git)

    def test_accepts_all_cases_from_trusted_main(self):
        self.assertEqual(self.verify()['cases'], 1)
        self.assertEqual(self.verify()['candidate'], self.candidate)

    def test_requires_exact_cases_hash_observer_and_gui(self):
        for mutation in ['missing', 'extra', 'old-head', 'wrong-hash', 'pending', 'no-gui', 'edited-case', 'unknown-scope', 'develop-changes']:
            with self.subTest(mutation=mutation):
                promotion = copy.deepcopy(self.promotion); acceptance = copy.deepcopy(self.acceptance); body = self.pr['body']
                if mutation == 'missing': self.promotion['results'] = {}
                if mutation == 'extra': self.promotion['results']['35:OTHER'] = observation()
                if mutation == 'old-head': self.promotion['results']['35:QA-1']['head'] = self.base
                if mutation == 'wrong-hash': self.promotion['results']['35:QA-1']['artifact_sha256'] = 'f' * 64
                if mutation == 'pending': self.promotion['results']['35:QA-1']['status'] = 'pending'
                if mutation == 'no-gui':
                    self.pr['body'] = body.replace('GUI: required', 'GUI: not-required')
                    self.acceptance = change(gui=False)
                if mutation == 'edited-case': self.acceptance['cases'][0]['steps'] = ['Skip important step']
                if mutation == 'unknown-scope': self.promotion['scope'] = 'arbitrary'
                if mutation == 'develop-changes': self.promotion['changes'] = []
                self.results_commit()
                with self.assertRaises(ValueError): self.verify()
                self.promotion, self.acceptance, self.pr['body'] = promotion, acceptance, body
                self.results_commit()

    def test_plan_cannot_be_introduced_or_changed_by_candidate(self):
        self.plan['files']['src/new-feature.kt'] = '100644'
        self.write(self.plan_path, self.plan)
        self.pr['head']['sha'] = self.commit()
        with self.assertRaisesRegex(ValueError, 'outside trusted scope'): self.verify()
        with self.assertRaises((ValueError, subprocess.CalledProcessError)):
            scoped_candidate(self.base, self.candidate, 36, self.git)

    def test_product_edit_then_revert_after_candidate_is_rejected(self):
        self.write('Graffix-AR.xcodeproj/project.pbxproj', 'untested'); self.commit()
        self.write('Graffix-AR.xcodeproj/project.pbxproj', 'modern'); self.pr['head']['sha'] = self.commit()
        with self.assertRaisesRegex(ValueError, 'outside trusted scope'): self.verify()

    def test_candidate_merge_and_outside_edit_revert_are_rejected(self):
        self.git('checkout', '-qb', 'candidate-again', self.base)
        self.write('secret-feature.kt', 'new'); self.commit()
        self.git('rm', 'secret-feature.kt'); self.write('Graffix-AR.xcodeproj/project.pbxproj', 'modern')
        candidate = self.commit()
        with self.assertRaisesRegex(ValueError, 'outside trusted scope'): scoped_candidate(self.base, candidate, 35, self.git)
        self.git('checkout', '-qb', 'side', self.base)
        self.write('Graffix-AR.xcodeproj/project.pbxproj', 'side'); side = self.commit()
        tree = self.git('rev-parse', side+':')
        merge = self.git('commit-tree', tree, '-p', candidate, '-p', side, '-m', 'merge')
        with self.assertRaises(ValueError): scoped_candidate(self.base, merge, 35, self.git)

    def test_symlink_gitlink_and_executable_changes_are_rejected(self):
        for mode, object_ref in [('120000', self.git('rev-parse', self.base+':Graffix-AR.xcodeproj/project.pbxproj')),
                                 ('160000', self.base), ('100755', self.git('rev-parse', self.candidate+':Graffix-AR.xcodeproj/project.pbxproj'))]:
            with self.subTest(mode=mode):
                self.git('checkout', '--detach', '-q', self.base)
                self.git('update-index', '--cacheinfo', mode, object_ref, 'Graffix-AR.xcodeproj/project.pbxproj')
                tree = self.git('write-tree'); head = self.git('commit-tree', tree, '-p', self.base, '-m', 'mode')
                with self.assertRaisesRegex(ValueError, 'outside trusted scope'): scoped_candidate(self.base, head, 35, self.git)
                self.git('read-tree', self.base)

    def test_base_movement_rejects_old_candidate(self):
        self.pr['base']['sha'] = self.candidate
        with self.assertRaisesRegex(ValueError, 'current main base'): self.verify()
        self.promotion['base'] = self.candidate
        self.results_commit()
        with self.assertRaisesRegex(ValueError, 'no changes'): self.verify()



    def test_invalid_trusted_plans_fail_closed(self):
        for mutation in ('empty-files', 'empty-cases', 'gate-path', 'wrong-issue', 'duplicate-json', 'plan-symlink'):
            with self.subTest(mutation=mutation):
                plan = copy.deepcopy(self.plan)
                if mutation == 'empty-files': plan['files'] = {}
                if mutation == 'empty-cases': plan['acceptance']['cases'] = []
                if mutation == 'gate-path': plan['files']['scripts/workflow/verification.py'] = '100644'
                if mutation == 'wrong-issue': plan['issue'] = 36
                def git(*args):
                    if args[0] == 'ls-tree' and mutation == 'plan-symlink':
                        return '120000 blob ' + 'a' * 40 + '\t' + self.plan_path
                    if args == ('show', self.base + ':' + self.plan_path):
                        if mutation == 'duplicate-json': return '{"schema":1,"schema":1}'
                        return json.dumps(plan)
                    return self.git(*args)
                with self.assertRaises(ValueError): scoped_candidate(self.base, self.candidate, 35, git)
