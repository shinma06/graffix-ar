"""Delayed events must not turn live closed PRs into fresh acceptance decisions."""
import copy
import json
import subprocess
import unittest
from unittest.mock import Mock, patch

import acceptance_ci as ci
from verification import verify_pr

HEAD = 'a' * 40
BASE = 'b' * 40
MOVED = 'c' * 40


def pr_data():
    return {
        'number': 90, 'state': 'open', 'merged': False,
        'body': 'Issue: #90\nIntegration: tooling\n'
                'Verification: docs/verification/changes/issue-90.json\n'
                'GUI: not-required\nGUI reason: CI state handling only',
        'head': {'sha': HEAD, 'ref': 'codex/90-acceptance-closed-pr'},
        'base': {'sha': BASE, 'ref': 'main'},
    }


class DelayedAcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.event = pr_data()
        self.check_output = self.enterContext(patch.object(ci.subprocess, 'check_output', return_value=BASE))
        self.run = self.enterContext(patch.object(ci.subprocess, 'run'))
        self.verify = self.enterContext(patch.object(ci, 'verify_pr', return_value={'mode': 'tooling', 'cases': 0}))

    def closed(self, merged=True):
        return dict(pr_data(), state='closed', merged=merged)

    def test_delayed_merged_or_unmerged_closed_pr_skips_before_refs_and_validation(self):
        for merged in (True, False):
            with self.subTest(merged=merged):
                current = self.closed(merged)
                # A closed PR does not need a valid *current* body/base to be ignored.
                current['body'] = ''
                current['base'] = {'ref': 'main', 'sha': MOVED}
                api = Mock(return_value=current)
                result = ci.check_pr(self.event, api)
                self.assertEqual(result, {'status': 'skipped', 'reason': 'live-pr-closed', 'pr': 90})
                self.assertNotIn('gui_complete', result)
                api.assert_called_once_with('pulls/90')
        self.check_output.assert_not_called()
        self.run.assert_not_called()
        self.verify.assert_not_called()

    def test_closed_event_with_live_reopened_pr_runs_normal_gate(self):
        self.event.update(state='closed', merged=True)
        current = pr_data()
        api = Mock(side_effect=[current, {'object': {'sha': BASE}}, pr_data(), {'object': {'sha': BASE}}])
        self.assertEqual(ci.check_pr(self.event, api), {'mode': 'tooling', 'cases': 0})
        self.verify.assert_called_once()

    def test_unknown_or_inconsistent_live_state_is_not_skipped(self):
        for state, merged in ((None, False), ('unknown', False), ('open', True)):
            with self.subTest(state=state, merged=merged):
                current = dict(pr_data(), state=state, merged=merged)
                with self.assertRaisesRegex(ValueError, 'state is missing or inconsistent'):
                    ci.check_pr(self.event, Mock(return_value=current))
        self.verify.assert_not_called()

    def test_live_lookup_error_fails_closed(self):
        with self.assertRaisesRegex(OSError, 'API unavailable'):
            ci.check_pr(self.event, Mock(side_effect=OSError('API unavailable')))
        self.verify.assert_not_called()

    def test_open_event_head_or_target_mismatch_still_fails(self):
        for field, key, value in (('head', 'sha', MOVED), ('base', 'ref', 'develop')):
            with self.subTest(field=field):
                current = pr_data()
                current[field][key] = value
                with self.assertRaisesRegex(ValueError, 'changed after this workflow was queued'):
                    ci.check_pr(self.event, Mock(side_effect=[current, copy.deepcopy(current)]))
        self.verify.assert_not_called()

    def test_open_metadata_failure_is_preserved(self):
        current = pr_data()
        current['body'] = ''
        with self.assertRaises(ValueError):
            ci.check_pr(self.event, Mock(side_effect=[current, copy.deepcopy(current)]))
        self.run.assert_not_called()
        self.verify.assert_not_called()

    def test_open_validation_or_git_failure_is_preserved(self):
        for error in (ValueError('coverage incomplete'), subprocess.CalledProcessError(128, 'git')):
            with self.subTest(error=error):
                self.verify.side_effect = error
                api = Mock(side_effect=[pr_data(), {'object': {'sha': BASE}}, pr_data()])
                with self.assertRaises(type(error)) as raised:
                    ci.check_pr(self.event, api)
                self.assertIs(raised.exception, error)

    def test_closed_during_failed_validation_is_no_decision(self):
        for error in (ValueError('empty diff after merge'), subprocess.CalledProcessError(128, 'git')):
            with self.subTest(error=error):
                self.verify.side_effect = error
                api = Mock(side_effect=[pr_data(), {'object': {'sha': BASE}}, self.closed()])
                self.assertEqual(ci.check_pr(self.event, api)['status'], 'skipped')

    def test_lookup_failure_after_validation_error_does_not_report_skip(self):
        self.verify.side_effect = ValueError('coverage incomplete')
        api = Mock(side_effect=[pr_data(), {'object': {'sha': BASE}}, OSError('API unavailable')])
        with self.assertRaisesRegex(OSError, 'API unavailable'):
            ci.check_pr(self.event, api)

    def test_closed_during_successful_validation_discards_acceptance_result(self):
        api = Mock(side_effect=[pr_data(), {'object': {'sha': BASE}}, self.closed()])
        result = ci.check_pr(self.event, api)
        self.assertEqual(result['status'], 'skipped')
        self.assertNotIn('mode', result)
        self.assertEqual(api.call_count, 3)  # No new-base lookup for a closed PR.

    def test_open_final_head_target_body_or_base_race_still_fails(self):
        for change in ('head', 'target', 'body', 'base'):
            with self.subTest(change=change):
                latest = pr_data()
                latest_base = BASE
                if change == 'head': latest['head']['sha'] = MOVED
                if change == 'target': latest['base']['ref'] = 'develop'
                if change == 'body': latest['body'] += '\nchanged scope'
                if change == 'base': latest_base = MOVED
                api = Mock(side_effect=[pr_data(), {'object': {'sha': BASE}}, latest, {'object': {'sha': latest_base}}])
                with self.assertRaisesRegex(ValueError, 'changed during acceptance validation'):
                    ci.check_pr(self.event, api)

    def test_open_empty_tooling_diff_remains_invalid_with_real_validator(self):
        data = {'schema': 1, 'issue': 90, 'gui_required': False, 'reason': 'CI only',
                'cli_checks': ['python3 -m unittest'], 'cases': []}
        git = Mock(side_effect=[json.dumps(data), ''])
        self.verify.side_effect = lambda pr, api: verify_pr(pr, api, git)
        api = Mock(side_effect=[pr_data(), {'object': {'sha': BASE}}, pr_data()])
        with self.assertRaisesRegex(ValueError, 'Direct main tooling route'):
            ci.check_pr(self.event, api)


    def test_open_gui_case_coverage_remains_required_with_real_validator(self):
        current = pr_data()
        current['base']['ref'] = 'develop'
        current['body'] = current['body'].replace('Integration: tooling', 'Integration: develop').replace('GUI: not-required', 'GUI: required')
        event = copy.deepcopy(current)
        data = {'schema': 1, 'issue': 90, 'gui_required': True, 'reason': 'GUI changed',
                'cli_checks': ['python3 -m unittest'], 'cases': []}
        git = Mock(return_value=json.dumps(data))
        self.verify.side_effect = lambda pr, api: verify_pr(pr, api, git)
        api = Mock(side_effect=[current, {'object': {'sha': BASE}}, copy.deepcopy(current)])
        with self.assertRaisesRegex(ValueError, 'Required GUI Cases missing'):
            ci.check_pr(event, api)

if __name__ == '__main__':
    unittest.main()
