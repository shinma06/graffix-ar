"""Issue naming and the three exclusive label axes."""
import re

TYPES = {'feature': '機能', 'bug': '修正', 'research': '調査', 'qa': '試験', 'maintenance': '運用', 'tracking': '追跡'}
AXES = {'type': set(TYPES), 'priority': {'P0', 'P1', 'P2'},
        'status': {'ready', 'in-progress', 'review', 'blocked', 'deferred', 'done'}}


def labels(issue):
    return [x['name'] if isinstance(x, dict) else x for x in issue.get('labels', [])]


def validate_issue(issue):
    if 'pull_request' in issue or issue.get('state') not in ('open', 'closed'):
        raise ValueError('Expected an Issue with a valid state')
    selected = {}
    for axis, allowed in AXES.items():
        values = [x.split(':', 1)[1] for x in labels(issue) if x.startswith(axis + ':')]
        if len(values) != 1 or values[0] not in allowed:
            raise ValueError('Exactly one supported ' + axis + ' label is required')
        selected[axis] = values[0]
    prefix = r'\[' + TYPES[selected['type']] + r'\] '
    if selected['type'] == 'qa':
        prefix += r'#[1-9][0-9]* '
    title = issue.get('title', '')
    if not re.fullmatch(prefix + r'\S.*', title) or re.search(r'\bP[012]\b', title):
        raise ValueError('Issue title must match its type and omit priority; QA needs #origin')
    if (issue['state'] == 'closed') != (selected['status'] == 'done'):
        raise ValueError('Closed Issues require status:done; open Issues must not be done')
    return selected


def done_labels(issue):
    return [x for x in labels(issue) if not x.startswith('status:')] + ['status:done']
