import copy
import unittest
from unittest.mock import patch
import qa_handoff as q
from issue_schema import validate_issue
from test_agent_loop import FakeGitHub, report, HEAD, BASE
import test_agent_loop as tal
import agent_loop as al

CHANGE = {'schema': 1, 'issue': 35, 'gui_required': False, 'reason': 'CLI tooling', 'cli_checks': ['unit tests'], 'cases': []}


class GH(FakeGitHub):
    def __init__(self):
        super().__init__()
        self.created = 0
        self.fail_create = False
        self.fail_link = False
        self.children = []
        self.fail_relationship = False
    def pages(self, path):
        if '/sub_issues?' in path:
            return copy.deepcopy(self.children)
        return [dict(copy.deepcopy(x), number=n) for n, x in self.issues.items()]
    def api(self, path, method='GET', data=None):
        if path.endswith('/sub_issues') and method == 'POST':
            if not self.fail_relationship:
                assert data['sub_issue_id'] == 10000
                self.children.append(self.issue(100))
            return self.issue(100)
        if path.endswith('/issues') and method == 'POST':
            self.created += 1
            self.issues[100] = dict(data, id=10000, number=100, state='open')
            if 'milestone' in data:
                self.issues[100]['milestone'] = {'number': data['milestone']}
            if self.fail_create:
                self.fail_create = False
                raise RuntimeError('lost create response')
            return self.issue(100)
        return super().api(path, method, data)
    def comment(self, n, body, comment_id=None):
        if n == 35 and self.fail_link:
            raise RuntimeError('link failed')
        return super().comment(n, body, comment_id)


class HandoffTests(unittest.TestCase):
    def setUp(self):
        self.gh = GH()
        self.pr = self.gh.pull
        self.pr.update(merged=True, merge_commit_sha='d'*40)
        self.pr['base']['ref'] = 'develop'
        self.pr['body'] = self.pr['body'].replace('tooling', 'develop')
    def transfer(self):
        return q.handoff(self.gh, al.REPO, self.pr, self.gh.issue(35), CHANGE)
    def test_success_and_retry_reuse_full_snapshot(self):
        self.assertEqual(self.transfer(), 100)
        self.assertEqual(self.transfer(), 100)
        self.assertEqual(self.gh.created, 1)
        self.assertIn('cases', self.gh.issue(100)['body'])
        self.assertEqual(len(self.gh.comments(35)), 1)
    def test_milestone_and_parent_are_preserved_on_retry(self):
        self.gh.issues[35]['milestone'] = {'number': 7}
        self.transfer()
        self.transfer()
        self.assertEqual(self.gh.issue(100)['milestone'], {'number': 7})
        self.assertEqual([i['number'] for i in self.gh.children], [100])
        self.gh.issues[100]['milestone'] = {'number': 8}
        with self.assertRaisesRegex(ValueError, 'milestone differs'):
            self.transfer()
        self.assertEqual(self.gh.issue(100)['milestone'], {'number': 8})

    def test_same_number_in_another_repository_is_not_the_qa(self):
        self.gh.children = [{'id': 99999, 'number': 100,
                             'repository_url': 'https://api.github.com/repos/owner/other'}]
        self.transfer()
        self.assertEqual([i['id'] for i in self.gh.children], [99999, 10000])

    def test_failed_relationship_readback_keeps_origin_open(self):
        self.gh.fail_relationship = True
        with self.assertRaisesRegex(ValueError, 'parent relationship readback'):
            self.transfer()
        self.assertEqual(self.gh.issue(35)['state'], 'open')
        self.assertEqual(self.gh.comments(35), [])
        self.gh.fail_relationship = False
        self.transfer()
        self.assertEqual(self.gh.created, 1)

    def test_lost_create_response_reuses_issue(self):
        self.gh.fail_create = True
        with self.assertRaises(RuntimeError): self.transfer()
        self.assertEqual(self.gh.issue(35)['state'], 'open')
        self.transfer()
        self.assertEqual(self.gh.created, 1)
    def test_link_failure_keeps_origin_and_retries(self):
        self.gh.fail_link = True
        with self.assertRaises(RuntimeError): self.transfer()
        self.assertEqual(self.gh.issue(35)['state'], 'open')
        self.gh.fail_link = False
        self.transfer()
        self.assertEqual(self.gh.created, 1)
    def test_duplicate_or_closed_qa_blocks(self):
        self.transfer()
        self.gh.issues[101] = self.gh.issue(100)
        with self.assertRaisesRegex(ValueError, 'Multiple'): self.transfer()
        del self.gh.issues[101]
        self.gh.issues[100].update(state='closed', labels=['type:qa','priority:P2','status:done'])
        with self.assertRaisesRegex(ValueError, 'closed'): self.transfer()
    def test_existing_pm_body_preserved_and_snapshot_appended(self):
        self.transfer()
        self.gh.issues[100]['body'] = '<!-- issue-qa-handoff:v1 origin=35 -->\nPM observations'
        self.transfer()
        self.assertIn('PM observations', self.gh.issue(100)['body'])
        self.assertEqual(len(self.gh.comments(100)), 1)
    def test_invalid_cases_block_before_creating_qa(self):
        invalid = dict(CHANGE, gui_required=True, cases=[])
        self.pr['body'] = self.pr['body'].replace('GUI: not-required', 'GUI: required')
        with self.assertRaises(ValueError):
            q.handoff(self.gh, al.REPO, self.pr, self.gh.issue(35), invalid)
        self.assertEqual(self.gh.created, 0)

    def test_unconfirmed_merge_blocks(self):
        self.pr['merged'] = False
        with self.assertRaises(ValueError): self.transfer()
        self.assertEqual(self.gh.created, 0)


class ClosureTests(unittest.TestCase):
    setUp = tal.LoopTests.setUp
    def finish(self, complete=True, fail_link=False, change_origin=False):
        gh = GH(); gh.fail_link = fail_link; self.loop.gh = gh
        if change_origin:
            original_comment = gh.comment
            def comment(n, body, comment_id=None):
                result = original_comment(n, body, comment_id)
                if n == 35: gh.issues[35]["body"] += " new acceptance"
                return result
            gh.comment = comment
        gh.pull.update(merged=True, merge_commit_sha='d'*40)
        gh.pull['base']['ref'] = 'develop'
        gh.pull['body'] = gh.pull['body'].replace('tooling','develop')
        state = {'binding': al.binding(gh.pull, gh.issue(35)), 'review': dict(report(), issue_complete=complete)}
        h = {'issue':35,'parent':1,'close_issue':False,'gui_required':False}
        with patch.object(al, 'git', return_value=__import__('json').dumps(CHANGE)) as git:
            result = self.loop.finish(gh.pull, h, state, None)
            if complete:
                self.assertEqual([c.args[0] for c in git.call_args_list], ['fetch', 'show'])
        return gh, result
    def test_completed_implementation_closes_after_qa_even_without_flag(self):
        gh, result = self.finish()
        self.assertTrue(result['issue_closed'])
        self.assertEqual(gh.issue(35)['state'], 'closed')
        self.assertIn('status:done', gh.issue(35)['labels'])
        self.assertEqual(gh.issue(1)['body'], '- [ ] #35 task')
        self.assertEqual(gh.created, 1)
    def test_failed_link_never_closes_or_cleans_up(self):
        with self.assertRaisesRegex(RuntimeError, 'link failed'): self.finish(fail_link=True)
        self.assertEqual(self.loop.gh.issue(35)['state'], 'open')
        self.loop.cleanup.assert_not_called()

    def test_changed_acceptance_during_transfer_never_closes(self):
        with self.assertRaisesRegex(ValueError, 'acceptance changed'): self.finish(change_origin=True)
        self.assertEqual(self.loop.gh.issue(35)['state'], 'open')
        self.loop.cleanup.assert_not_called()

    def test_unfinished_implementation_stays_open(self):
        gh, result = self.finish(False)
        self.assertFalse(result['issue_closed'])
        self.assertEqual(gh.created, 0)


class SchemaTests(unittest.TestCase):
    def test_axes_title_and_closed_status(self):
        valid = {'title':'[試験] #35 試験', 'state':'open', 'labels':['type:qa','priority:P1','status:ready']}
        validate_issue(valid)
        for change in ({'title':'[試験] 試験'}, {'title':'[試験] #35 P1 試験'}, {'state':'closed'},
                       {'labels':['type:qa','priority:P1','status:ready','status:review']},
                       {'labels':['type:qa','priority:P1','status:unknown']}):
            with self.assertRaises(ValueError): validate_issue(dict(valid, **change))
        validate_issue(dict(valid, state='closed', labels=['type:qa','priority:P1','status:done']))

if __name__ == '__main__': unittest.main()
