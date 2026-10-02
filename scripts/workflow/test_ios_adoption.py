"""iOS build identity and protection checks without a device or GitHub mutations."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest
from unittest.mock import Mock, patch

import agent_loop
import ios_build
import release_candidate as rc
from verification import baseline_cases
from test_verification import change


class AdoptionTests(unittest.TestCase):
    def test_bootstrap_is_limited_to_known_base_and_named_migration(self):
        workflow = (ios_build.ROOT / '.github/workflows/acceptance.yml').read_text()
        script = textwrap.dedent(workflow.split('        run: |\n', 1)[1])
        with tempfile.TemporaryDirectory() as tmp:
            for name, source in {'git': '#!/bin/sh\nprintf "%s" "$FIXTURE_SHA"\n',
                                 'python3': '#!/bin/sh\nexit 17\n'}.items():
                path = Path(tmp) / name
                path.write_text(source); path.chmod(0o755)
            initial = '7b55e8bff1c4583d2c1c68e7e7360e82a543ccf9'
            cases = [(initial, 'main', 'codex/2-adopt-harness', 0),
                     ('4cdc460372a068ce3349badaf0068d960907998c', 'develop', 'codex/3-sync-harness', 0),
                     ('a' * 40, 'main', 'codex/2-adopt-harness', 17),
                     (initial, 'main', 'codex/2-other', 17),
                     (initial, 'develop', 'codex/2-adopt-harness', 17)]
            for sha, target, branch, expected in cases:
                env = dict(os.environ, PATH=tmp, FIXTURE_SHA=sha, BASE_REF=target, HEAD_REF=branch)
                result = subprocess.run(['/bin/bash', '-c', script], env=env, capture_output=True)
                self.assertEqual(result.returncode, expected, result.stderr)

    def test_legacy_history_needs_exact_trusted_plan_and_real_cases(self):
        base, old = 'a' * 40, 'b' * 40
        entries = [{'commit': old, 'baseline': True}]
        document = {'schema': 1, 'issue': 35, 'commits': [old], 'acceptance': change()}
        with patch('verification.regular_json', return_value=document) as read:
            self.assertEqual(baseline_cases(base, entries, None), {'35:QA-1': (None, 'app')})
            self.assertEqual(read.call_args.args[0], base)
            with self.assertRaises(ValueError):
                baseline_cases(base, [{'commit': 'c' * 40, 'baseline': True}], None)
            document['acceptance']['cases'] = []
            with self.assertRaises(ValueError): baseline_cases(base, entries, None)

    def test_build_uses_shared_scheme_and_explicit_signing(self):
        command = ios_build.build_command(Path('/tmp/derived'))
        self.assertEqual(command[command.index('-scheme') + 1], 'Mock Up')
        self.assertEqual(command[command.index('-destination') + 1], 'generic/platform=iOS Simulator')
        self.assertIn('CODE_SIGNING_ALLOWED=NO', command)
        self.assertNotIn('CODE_SIGNING_ALLOWED=NO', ios_build.build_command(Path('/tmp/derived'), 'device', True))
        with self.assertRaises(ValueError):
            ios_build.build_command(Path('/tmp/derived'), 'simulator', True)

    def test_candidate_rejects_stale_bytes_and_unsafe_filename(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            artifact = root / 'Graffix-AR.app.zip'
            artifact.write_bytes(b'fixed candidate')
            digest = rc.sha256(artifact)
            manifest = {'schema': 1, 'source': 'a' * 40, 'platform': 'device', 'signed': True,
                        'artifact': artifact.name, 'artifact_sha256': digest}
            path = root / 'manifest.json'
            path.write_text(json.dumps(manifest))
            self.assertEqual(rc.candidate(root, digest), manifest)
            artifact.write_bytes(b'rebuilt candidate')
            with self.assertRaises(ValueError):
                rc.candidate(root, digest)
            path.write_text(json.dumps(dict(manifest, artifact='../outside.zip')))
            with self.assertRaises(ValueError):
                rc.candidate(root, digest)

    def test_build_checks_clean_source_and_trusted_scope_before_building(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = 'a' * 40
            def git(*args):
                if args == ('rev-parse', 'HEAD'): return source
                if args == ('rev-parse', 'origin/main'): return 'b' * 40
                return ''
            with patch.object(rc, 'git_read', side_effect=git), patch.object(rc, 'scoped_candidate', side_effect=ValueError('scope')) as scope, patch.object(ios_build, 'build') as build:
                with self.assertRaisesRegex(ValueError, 'scope'):
                    rc.build(source, Path(tmp) / 'output', 12)
                scope.assert_called_once_with('b' * 40, source, 12)
                build.assert_not_called()

    def test_missing_protection_or_bypass_blocks_merge(self):
        gh = agent_loop.GitHub()
        pull = {'number': 1, 'head': {'sha': 'a' * 40}, 'base': {'ref': 'develop'}, 'draft': False,
                'body': 'Issue: #1\nIntegration: develop\nVerification: docs/verification/changes/issue-1.json'}
        rules = [{'type': name, 'ruleset_id': 12} for name in ('deletion', 'non_fast_forward')]
        rules += [{'type': 'pull_request', 'ruleset_id': 12, 'parameters': {'required_review_thread_resolution': True}},
                  {'type': 'required_status_checks', 'ruleset_id': 12, 'parameters': {
                      'strict_required_status_checks_policy': True,
                      'required_status_checks': [{'context': x} for x in ('test', 'PR policy', 'Acceptance gate', 'Agent review')]}}]
        for invalid in ([], rules[:-1]):
            gh.api = Mock(return_value=invalid)
            with self.assertRaises(ValueError): gh.merge(pull)
            self.assertEqual(gh.api.call_count, 1)
        gh.api = Mock(side_effect=[rules, {'bypass_actors': [{}]}])
        with self.assertRaises(ValueError): gh.merge(pull)
        gh.api = Mock(side_effect=[copy.deepcopy(rules), {'bypass_actors': []}, {'merged': True}])
        gh.merge(pull)
        self.assertEqual(gh.api.call_args.args[1], 'PUT')
        self.assertEqual(gh.api.call_args.args[2], {'sha': 'a' * 40, 'merge_method': 'squash'})


if __name__ == '__main__':
    unittest.main()
