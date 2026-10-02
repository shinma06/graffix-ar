import json
import unittest
from datetime import datetime, timezone
from unittest.mock import patch

import governance_audit as g

NOW = datetime(2026, 9, 11, tzinfo=timezone.utc)


def audit(number=195, **changes):
    record = dict(status='completed', audited_through='2026-09-10T00:00:00Z',
                  completed_at='2026-09-10T01:00:00Z', health='MINOR ISSUES',
                  merge_threshold=10, audited_milestones=[1, 2], last_high_impact_change='#171')
    record.update(changes)
    return dict(number=number, state='closed', user={'login': g.OWNER}, html_url=f'https://example.test/issues/{number}',
                body=g.MARKER + '\n```json\n' + json.dumps(record) + '\n```')


def pull(number, **changes):
    pr = dict(number=number, title='Feature', html_url=f'https://example.test/pull/{number}',
              body='', merged_at='2026-09-10T02:00:00Z', base={'ref': 'develop'})
    pr.update(changes)
    return pr


class GovernanceAuditTests(unittest.TestCase):
    def test_initial_and_threshold_boundary_count_each_pr_once(self):
        self.assertIsNone(g.status([], [], [], NOW)['merge_count_since_audit'])
        self.assertTrue(g.status([], [], [], NOW)['due_reasons'])
        prs = [pull(n) for n in range(1, 11)]
        self.assertEqual([], g.status([audit()], prs[:9], [], NOW)['due_reasons'])
        result = g.status([audit()], prs + [prs[0]], [], NOW)
        self.assertEqual(10, result['merge_count_since_audit'])
        self.assertIn('10 meaningful merges reached', result['due_reasons'])

    def test_reset_exclusions_and_unmerged_or_other_target(self):
        prs = [pull(1, merged_at='2026-09-10T00:00:00Z'), pull(2, merged_at=None),
               pull(3, base={'ref': 'scratch'}), pull(4, body='Governance-count: exclude - reviewed mechanical update'),
               pull(5, body='Governance-count: exclude - '), pull(6, base={'ref': 'main'})]
        result = g.status([audit()], prs, [], NOW)
        self.assertEqual([5, 6], [p['number'] for p in result['merged_prs_to_review']])
        self.assertEqual([4], [p['number'] for p in result['excluded_prs']])
        newer = audit(200, audited_through='2026-09-10T03:00:00Z', completed_at='2026-09-10T04:00:00Z')
        self.assertEqual(0, g.status([audit(), newer], prs, [], NOW)['merge_count_since_audit'])
        newer['state'] = 'open'
        self.assertEqual(195, g.status([audit(), newer], prs, [], NOW)['last_audit']['issue'])

    def test_major_milestone_and_structural_health_are_not_count_gated(self):
        milestones = [dict(number=1, title='Phase complete', state='closed', closed_at='2026-09-10T03:00:00Z')]
        result = g.status([audit(health='STRUCTURAL PROBLEM')], [], milestones, NOW)
        self.assertEqual(2, len(result['due_reasons']))
        self.assertEqual([1], [m['number'] for m in result['closed_milestones']])
        self.assertIn('high-impact', result['human_judgment'])

    def test_invalid_or_duplicate_state_does_not_silently_reset_count(self):
        bad = audit()
        bad['body'] += g.MARKER
        for issue in [bad, audit(merge_threshold=0), audit(health='unknown'),
                      audit(audited_through='2027-01-01T00:00:00Z'),
                      audit(audited_through='2026-09-10T00:00:00')]:
            with self.subTest(issue=issue), self.assertRaises(ValueError):
                g.status([issue], [], [], NOW)

    def test_external_issues_cannot_replace_or_break_the_trusted_baseline(self):
        external = audit(999, audited_through='2026-09-10T03:00:00Z',
                         completed_at='2026-09-10T04:00:00Z', health='HEALTHY')
        external['user'] = {'login': 'external-contributor'}
        prs = [pull(n) for n in range(1, 11)]
        for body in [external['body'], g.MARKER + '\nmalformed']:
            external['body'] = body
            result = g.status([audit(), external], prs, [], NOW)
            self.assertEqual(195, result['last_audit']['issue'])
            self.assertEqual(10, result['merge_count_since_audit'])
            self.assertIn('10 meaningful merges reached', result['due_reasons'])

    def test_cli_uses_read_only_paginated_endpoints_and_propagates_failure(self):
        with patch.object(g, 'GitHub') as cls, patch('builtins.print'):
            cls.return_value.pages.side_effect = [[], [], []]
            g.main()
            self.assertEqual(3, cls.return_value.pages.call_count)
            cls.return_value.api.assert_not_called()
            cls.return_value.pages.side_effect = RuntimeError('offline')
            with self.assertRaisesRegex(RuntimeError, 'offline'):
                g.main()


if __name__ == '__main__':
    unittest.main()
