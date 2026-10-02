#!/usr/bin/env python3
"""Read-only trigger status. Audit decisions and corrections remain with the owner."""
import json
import re
from datetime import datetime, timezone

from agent_loop import GitHub, OWNER, REPO

MARKER = '<!-- governance-audit-state:v1 -->'
HEALTH = {'HEALTHY', 'MINOR ISSUES', 'NEEDS CLEANUP', 'STRUCTURAL PROBLEM'}


def timestamp(value):
    result = datetime.fromisoformat(value.replace('Z', '+00:00'))
    if result.tzinfo is None:
        raise ValueError('Audit timestamps must include a timezone')
    return result


def latest_audit(issues, now):
    records = []
    for issue in issues:
        if issue.get('user', {}).get('login') != OWNER:
            continue
        body = issue.get('body') or ''
        if 'pull_request' in issue or MARKER not in body:
            continue
        if body.count(MARKER) != 1:
            raise ValueError(f'Duplicate audit state in issue #{issue["number"]}')
        match = re.match(r'\s*```json\s*\n(.*?)\n```', body.split(MARKER, 1)[1], re.S)
        if not match:
            raise ValueError(f'Missing audit state JSON in issue #{issue["number"]}')
        record = json.loads(match[1])
        if record['status'] != 'completed' or issue['state'] != 'closed':
            continue
        cutoff = timestamp(record['audited_through'])
        completed = timestamp(record['completed_at'])
        if not cutoff <= completed <= now or record['health'] not in HEALTH:
            raise ValueError(f'Invalid audit boundary or health in issue #{issue["number"]}')
        if type(record['merge_threshold']) is not int or record['merge_threshold'] < 1:
            raise ValueError('Audit merge threshold must be a positive integer')
        if not isinstance(record['audited_milestones'], list) or 'last_high_impact_change' not in record:
            raise ValueError('Audit milestone and high-impact review are required')
        records.append(dict(record, issue=issue['number'], url=issue['html_url']))
    return max(records, key=lambda r: (timestamp(r['audited_through']), r['issue']), default=None)


def status(issues, pulls, milestones, now=None):
    now = now or datetime.now(timezone.utc)
    audit = latest_audit(issues, now)
    cutoff = timestamp(audit['audited_through']) if audit else None
    merged = sorted({p['number']: p for p in pulls if p.get('merged_at') and
                     p['base']['ref'] in {'main', 'develop'} and
                     (cutoff is None or timestamp(p['merged_at']) > cutoff)}.values(),
                    key=lambda p: (timestamp(p['merged_at']), p['number']))
    meaningful, excluded = [], []
    for pr in merged:
        item = {'number': pr['number'], 'title': pr['title'], 'url': pr['html_url']}
        reason = re.search(r'^Governance-count: exclude - (\S[^\r\n]*)$', pr.get('body') or '', re.M)
        if reason:
            excluded.append(dict(item, reason=reason[1]))
        else:
            meaningful.append(item)
    closed = [m for m in milestones if m['state'] == 'closed' and m.get('closed_at') and
              (cutoff is None or timestamp(m['closed_at']) > cutoff)]
    reasons = []
    threshold = audit['merge_threshold'] if audit else 10
    if audit is None:
        reasons.append('initial audit: no completed baseline')
    if audit and len(meaningful) >= threshold:
        reasons.append(f'{threshold} meaningful merges reached')
    if closed:
        reasons.append('milestone closed: assess major milestone trigger')
    if audit and audit['health'] == 'STRUCTURAL PROBLEM':
        reasons.append('structural problem: follow the recorded remediation')
    return {'last_audit': audit, 'merge_count_since_audit': len(meaningful) if audit else None,
            'merge_threshold': threshold, 'due_reasons': reasons,
            'merged_prs_to_review': meaningful, 'excluded_prs': excluded,
            'closed_milestones': [{'number': m['number'], 'title': m['title']} for m in closed],
            'human_judgment': 'Check high-impact changes, governance changes and anomalies even below threshold; this is not an audit pass.'}


def main():
    gh = GitHub()
    result = status(gh.pages(f'repos/{REPO}/issues?state=all&per_page=100'),
                    gh.pages(f'repos/{REPO}/pulls?state=all&per_page=100'),
                    gh.pages(f'repos/{REPO}/milestones?state=all&per_page=100'))
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
