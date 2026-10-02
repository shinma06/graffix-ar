"""Real Git DAG regression for reviewed tooling syncs; no network or GUI evidence."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from test_agent_loop import pr_data
from test_verification import change, observation
from verification import source_commits, verify_pr


class SyncPromotionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('config', 'core.hooksPath', '/dev/null')
        self.git('remote', 'add', 'origin', str(self.root))
        self.write('src/product.txt', 'base')
        self.commit('base')
        self.git('checkout', '-b', 'develop')
        self.write('src/product.txt', 'reviewed product')
        product = change(36)
        product['cases'].append(dict(product['cases'][0], id='QA-2'))
        self.write('docs/verification/changes/issue-36.json', product)
        self.develop = self.commit('reviewed product squash')
        self.git('checkout', 'main')
        self.write('docs/tooling.md', 'accepted main tooling')
        self.base = self.commit('main tooling')
        self.git('checkout', '-b', 'sync', 'develop')
        self.git('merge', '--no-ff', '--no-edit', 'main')
        self.sources = {99: self.source(36, self.develop, gui=True)}

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, env=self.env,
                                       text=True, stderr=subprocess.DEVNULL).strip()

    def write(self, path, value):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(value) if isinstance(value, dict) else value)

    def commit(self, message):
        self.git('add', '.')
        self.git('commit', '-m', message)
        return self.git('rev-parse', 'HEAD')

    def source(self, issue, merge, gui=False):
        source = pr_data()
        source.update(merged=True, merge_commit_sha=merge)
        source['base'].update(ref='develop', sha=self.develop)
        source['body'] = (f'Issue: #{issue}\nIntegration: develop\n'
                          f'GUI: {"required" if gui else "not-required"}\n'
                          f'Verification: docs/verification/changes/issue-{issue}.json')
        return source

    def prepare(self):
        self.write('docs/verification/changes/issue-38.json', change(38, False))
        head = self.commit('sync acceptance')
        self.git('checkout', 'develop')
        self.git('merge', '--no-ff', '--no-edit', head)
        self.candidate = self.git('rev-parse', 'HEAD')
        self.sources[100] = self.source(38, self.candidate)
        self.sources[100]['head']['sha'] = head
        self.git('checkout', '-b', 'promotion')
        self.write('docs/verification/changes/issue-35.json', change(35, False))
        self.manifest = {'schema': 1, 'base': self.base, 'candidate': self.candidate,
                         'artifact_sha256': 'e' * 64,
                         'changes': [{'commit': sha, 'pr': 99 if sha == self.develop else 100}
                                     for sha in self.git('rev-list', f'{self.base}..{self.candidate}').splitlines()],
                         'results': {f'36:QA-{n}': dict(observation(), head=self.candidate) for n in (1, 2)}}
        self.pr = pr_data()
        self.pr['base']['sha'] = self.base
        self.pr['body'] = self.pr['body'].replace('tooling', 'promotion')
        self.record()

    def record(self):
        self.write('docs/verification/promotion.json', self.manifest)
        self.pr['head']['sha'] = self.commit('record test observations')

    def api(self, path):
        if path == 'git/ref/heads/develop':
            return {'object': {'sha': self.candidate}}
        return self.sources[int(path.split('/')[-1])]

    def verify(self):
        return verify_pr(self.pr, self.api, self.git)

    def test_sync_covers_internal_history_and_reads_all_fixed_merge_cases(self):
        # A later sync Case deletion must not shrink the product merge requirements.
        self.write('docs/verification/changes/issue-36.json', change(36))
        self.prepare()
        self.assertEqual(self.verify()['cases'], 2)
        self.assertEqual(len(self.manifest['changes']), 4)
        self.assertEqual(sum(item['pr'] == 100 for item in self.manifest['changes']), 3)
        self.manifest['results'].pop('36:QA-2')
        self.record()
        with self.assertRaisesRegex(ValueError, 'ALL required Cases'):
            self.verify()

    def test_non_object_results_keep_a_clear_validation_error(self):
        self.prepare()
        for malformed in (None, [], [{}]):
            with self.subTest(results=malformed):
                self.manifest['results'] = malformed
                self.record()
                with self.assertRaisesRegex(ValueError, 'results must be an object'):
                    self.verify()

    def test_missing_duplicate_and_reassigned_commits_are_rejected(self):
        self.prepare()
        original = copy.deepcopy(self.manifest)
        for mutation in ('missing', 'duplicate', 'product-as-sync', 'internal-as-product'):
            with self.subTest(mutation=mutation):
                self.manifest = copy.deepcopy(original)
                if mutation == 'missing':
                    self.manifest['changes'].pop()
                elif mutation == 'duplicate':
                    self.manifest['changes'].append(self.manifest['changes'][0])
                else:
                    item = next(x for x in self.manifest['changes'] if
                                (x['pr'] == 99 if mutation == 'product-as-sync' else
                                 x['pr'] == 100 and x['commit'] != self.candidate))
                    item['pr'] = 100 if mutation == 'product-as-sync' else 99
                self.record()
                with self.assertRaises(ValueError):
                    self.verify()

    def test_wrong_repo_unmerged_sha_and_base_head_mismatch_are_rejected(self):
        self.prepare()
        original = copy.deepcopy(self.sources[100])
        for mutation in ('unmerged', 'head-repo', 'base-repo', 'merge-sha', 'head-sha', 'base-sha', 'target'):
            with self.subTest(mutation=mutation):
                source = self.sources[100] = copy.deepcopy(original)
                if mutation == 'unmerged': source['merged'] = False
                elif mutation.endswith('-repo'): source[mutation.split('-')[0]]['repo']['full_name'] = 'other/repo'
                elif mutation == 'merge-sha': source['merge_commit_sha'] = self.develop
                elif mutation.endswith('-sha'): source[mutation.split('-')[0]]['sha'] = self.base
                else: source['base']['ref'] = 'main'
                with self.assertRaises(ValueError):
                    self.verify()

    def test_product_edit_then_revert_inside_sync_is_rejected(self):
        self.write('src/product.txt', 'unreviewed product')
        self.commit('hidden edit')
        self.write('src/product.txt', 'reviewed product')
        self.commit('revert hidden edit')
        self.prepare()
        with self.assertRaisesRegex(ValueError, 'Product change in sync history'):
            self.verify()

    def test_product_rename_into_docs_is_rejected(self):
        self.git('mv', 'src/product.txt', 'docs/renamed-product.txt')
        self.commit('rename product into tooling path')
        self.prepare()
        with self.assertRaisesRegex(ValueError, 'Product change in sync history'):
            self.verify()

    def test_octopus_is_rejected(self):
        tree = self.git('rev-parse', 'HEAD^{tree}')
        extra = self.git('commit-tree', tree, '-p', self.base, '-m', 'other ancestor')
        bad = self.git('commit-tree', tree, '-p', self.develop, '-p', self.base,
                       '-p', extra, '-m', 'octopus')
        self.git('checkout', '-b', 'bad-sync', bad)
        self.prepare()
        with self.assertRaisesRegex(ValueError, 'first parents'):
            self.verify()

    def test_side_branch_not_in_main_is_rejected_even_if_tooling_only(self):
        self.git('checkout', '-b', 'unreviewed')
        self.write('docs/unreviewed.md', 'not main')
        self.commit('side change')
        self.git('checkout', 'sync')
        self.git('merge', '--no-ff', '--no-edit', 'unreviewed')
        self.prepare()
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify()

    def test_arbitrary_merge_without_main_sync_is_rejected(self):
        self.git('checkout', '-b', 'ordinary', self.develop)
        self.prepare()
        with self.assertRaisesRegex(ValueError, 'must actually merge'):
            source_commits(self.sources[100], self.base, self.git)

    def test_unbound_normal_commit_cannot_use_product_pr(self):
        self.write('docs/unreviewed.md', 'not a PR result')
        normal = self.commit('ordinary commit')
        self.prepare()
        next(x for x in self.manifest['changes'] if x['commit'] == normal)['pr'] = 99
        self.record()
        with self.assertRaisesRegex(ValueError, 'provenance'):
            self.verify()

    def test_outer_merge_product_resolution_is_rejected(self):
        self.prepare()
        old = self.candidate
        self.git('checkout', 'develop')
        self.write('src/product.txt', 'unreviewed merge resolution')
        self.git('add', '.')
        tree = self.git('write-tree')
        self.candidate = self.git('commit-tree', tree, '-p', self.develop,
                                  '-p', self.sources[100]['head']['sha'], '-m', 'bad merge')
        self.sources[100]['merge_commit_sha'] = self.candidate
        # Use the modified merge as a real candidate and a metadata-only promotion.
        self.git('checkout', '-b', 'bad-promotion', self.candidate)
        self.manifest['candidate'] = self.candidate
        next(x for x in self.manifest['changes'] if x['commit'] == old)['commit'] = self.candidate
        self.write('docs/verification/changes/issue-35.json', change(35, False))
        self.record()
        with self.assertRaisesRegex(ValueError, 'Product change in sync merge result'):
            self.verify()

    def test_sync_cannot_remove_case_json_at_its_fixed_merge(self):
        self.prepare()
        original_git = self.git
        def missing(*args):
            if args == ('show', f'{self.candidate}:docs/verification/changes/issue-38.json'):
                raise subprocess.CalledProcessError(128, ['git', *args])
            return original_git(*args)
        self.git = missing
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify()
