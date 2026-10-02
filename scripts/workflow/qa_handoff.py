"""Read-back verified QA transfer. Called only by the single coordinator after merge."""
import json
import re
from issue_schema import validate_issue
from verification import metadata, validate_change


def handoff(gh, repo, pr, origin, change):
    number, path, mode = metadata(pr)
    if mode != 'develop' or not pr.get('merged') or not re.fullmatch(r'[0-9a-f]{40}', pr.get('merge_commit_sha', '')):
        raise ValueError('QA transfer requires a confirmed develop merge')
    axes = validate_issue(origin)
    validate_change(change, number, 'GUI: required' in pr['body'])
    marker = f'<!-- issue-qa-handoff:v1 origin={number} -->'
    # List all Issues (not search indexing) so a lost create response is retryable.
    candidates = [x for x in gh.pages(f'repos/{repo}/issues?state=all&per_page=100')
                  if 'pull_request' not in x and marker in (x.get('body') or '')]
    if len(candidates) > 1:
        raise ValueError('Multiple QA Issues match origin; reconcile before closing')
    record = f'<!-- issue-qa-record:v1 origin={number} pr={pr["number"]} merge={pr["merge_commit_sha"]} -->'
    main_tracking = {
        'id': 'MAIN-REFLECTION', 'origin': number, 'pr': pr['number'],
        'merge_sha': pr['merge_commit_sha'], 'status': 'pending',
        'steps': ['固定候補とbuildを識別し、元PRの全Caseを確認する',
                  'promotion PRとmain履歴で元変更の反映を照合する'],
        'expected': '全必要Caseの証拠とmainへの反映が一致する',
        'gpt': {'status': 'pending', 'reason': 'main反映未確認'},
        'human': {'status': 'pending', 'reason': 'main反映未確認'},
        'fixed_build': None, 'fix_issue': None, 'fix_pr': None,
        'next_action': 'PMが候補/buildを固定して試験・main反映を照合する',
        'recheck': '製品failは専用修正Issue/PRから新候補を再確認する'}
    payload = (record + f'\n元Issue: #{number}\nPR: #{pr["number"]}\nmerge SHA: {pr["merge_commit_sha"]}\n'
               f'Verification: https://github.com/{repo}/blob/{pr["merge_commit_sha"]}/{path}\n\n'
               '## 受入・次操作・依存\n固定候補とbuildを指定し、全必要Caseを確認後main反映を照合する。'
               '元IssueのcloseはGUI pass/main反映を意味しない。固定build: 未登録。'
               '既存の観察は履歴であり新候補のpassではない。製品failは修正Issue/PRと再確認へ引き継ぐ。\n'
               'GUI不要でもmain反映確認をこのマトリクスで追跡する。\n\n```json\n' +
               json.dumps({'change': change, 'main_tracking': main_tracking}, ensure_ascii=False, indent=2) + '\n```')
    milestone = origin.get('milestone')
    if not candidates:
        summary = re.sub(r'^\[[^]]+\]\s*', '', origin['title'])
        data = {'title': f'[試験] #{number} {summary}', 'body': marker + '\n' + payload,
                'labels': ['type:qa', 'priority:' + axes['priority'], 'status:ready']}
        if milestone:
            data['milestone'] = milestone['number']
        qa = gh.api(f'repos/{repo}/issues', 'POST', data)
    else:
        qa = candidates[0]
    qa = gh.issue(qa['number'])
    if validate_issue(qa)['type'] != 'qa' or marker not in (qa.get('body') or '') or not qa['title'].startswith(f'[試験] #{number} '):
        raise ValueError('QA identity readback failed')
    if qa['state'] != 'open':
        raise ValueError('Existing QA is closed; reconcile remaining verification before closure')
    if milestone and (qa.get('milestone') or {}).get('number') != milestone['number']:
        raise ValueError('QA milestone differs from origin; reconcile before closure')
    children_path = f'repos/{repo}/issues/{number}/sub_issues'
    if not any(x['id'] == qa['id'] for x in gh.pages(children_path + '?per_page=100')):
        gh.api(children_path, 'POST', {'sub_issue_id': qa['id']})
    if not any(x['id'] == qa['id'] for x in gh.pages(children_path + '?per_page=100')):
        raise ValueError('QA parent relationship readback failed')
    if payload not in qa['body']:
        comments = gh.comments(qa['number'])
        if not any(payload == c['body'] for c in comments):
            gh.comment(qa['number'], payload)
        if not any(payload == c['body'] for c in gh.comments(qa['number'])):
            raise ValueError('QA content readback failed')
    link = f'<!-- issue-qa-link:v1 origin={number} qa={qa["number"]} -->'
    text = link + f'\n実装はPR #{pr["number"]} / {pr["merge_commit_sha"]}でdevelopへ統合。残る試験/main反映は #{qa["number"]}。GUI pass/main反映済みではありません。'
    if not any(c['body'] == text for c in gh.comments(number)):
        gh.comment(number, text)
    if not any(c['body'] == text for c in gh.comments(number)):
        raise ValueError('Origin backlink readback failed')
    return qa['number']
