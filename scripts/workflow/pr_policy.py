#!/usr/bin/env python3
"""Check PR metadata as data; never execute PR text or use a write token."""
import json
import os
import re
import sys
from urllib.request import Request, urlopen
from git_guard import BRANCH
from issue_schema import validate_issue


def validate(pr):
    branch = BRANCH.fullmatch(pr['head']['ref'])
    if not branch or pr['base']['ref'] not in ('main', 'develop'):
        raise ValueError('Use an Issue branch targeting main or develop')
    body = pr.get('body') or ''
    issues = re.findall(r'^Issue: #([1-9][0-9]*)\s*$', body, re.MULTILINE)
    if issues != [branch.group(2)]:
        raise ValueError('Exactly one Issue: #N must match the branch Issue number')
    gui = re.findall(r'^GUI: (required|not-required)\s*$', body, re.MULTILINE)
    if len(gui) != 1:
        raise ValueError('Exactly one GUI: required or GUI: not-required is needed')
    reason = re.findall(r'^GUI reason: (.+)$', body, re.MULTILINE)
    if len(reason) != 1 or len(reason[0].strip()) < 8 or '<' in reason[0] or 'TODO' in reason[0]:
        raise ValueError('Provide a concrete GUI reason (or linked cases)')
    integration = re.findall(r'^Integration: (develop|promotion|tooling)\s*$', body, re.M)
    if len(integration) != 1 or pr['base']['ref'] != ('develop' if integration[0] == 'develop' else 'main'):
        raise ValueError('Integration must match develop or main (promotion/tooling)')
    verification = re.findall(r'^Verification: (.+)$', body, re.M)
    if verification != [f'docs/verification/changes/issue-{issues[0]}.json']:
        raise ValueError('Verification must reference the Issue acceptance JSON')
    if pr['base']['ref'] == 'develop' and re.search(r'(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s+(?:#|https://github.com/)', body):
        raise ValueError('Develop PRs use Refs; do not auto-close acceptance Issues')
    return int(issues[0])


def api(path):
    repo = os.environ['GITHUB_REPOSITORY']
    request = Request(f'https://api.github.com/repos/{repo}/{path}', headers={
        'Authorization': 'Bearer ' + os.environ['GITHUB_TOKEN'],
        'Accept': 'application/vnd.github+json',
    })
    with urlopen(request, timeout=30) as response:
        try:
            data = json.load(response)
        except ValueError as error:
            raise OSError('Invalid GitHub API response') from error
    if not isinstance(data, dict):
        raise OSError('Expected a GitHub API object')
    return data


def closed(pr):
    if pr.get('state') not in ('open', 'closed'):
        raise ValueError('Cannot determine live PR state')
    if pr['state'] == 'closed':
        print('PR policy skipped: live PR is closed; acceptance was not evaluated.')
        return True
    return False


def main():
    try:
        with open(os.environ['GITHUB_EVENT_PATH']) as source:
            event = json.load(source)
        endpoint = f"pulls/{int(event['pull_request']['number'])}"
        pr = api(endpoint)
        if closed(pr):
            return 0
        error = None
        try:
            issue = validate(pr)
            data = api(f'issues/{issue}')
            if data.get('state') != 'open' or 'pull_request' in data:
                raise ValueError('The linked number must be an open Issue, not a PR')
            validate_issue(data)
        except (ValueError, KeyError, TypeError) as failure:
            error = failure
        # A merge may close both PR and Issue while the check is running.
        # Recheck even a rejected Issue, but never turn an API outage into a skip.
        latest = api(endpoint)
        if closed(latest):
            return 0
        if error is not None:
            raise error
        if any(latest.get(key) != pr.get(key) for key in ('head', 'base', 'body')):
            raise ValueError('Live PR metadata changed during validation; rerun the check')
        print(f'PR policy passed for Issue #{issue}. Human/agent review still verifies claims and evidence.')
        return 0
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
