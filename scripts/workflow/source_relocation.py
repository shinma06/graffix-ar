"""Validate an explicitly authorized relocation without adopting a different writer."""
import json
from pathlib import Path


def relocated_source(root, handoff, source, record, git):
    old = Path(handoff['source'])
    source = Path(source).resolve(strict=True)
    common = Path(git('rev-parse', '--path-format=absolute', '--git-common-dir', cwd=root)).resolve()
    if (source == Path(root).resolve() or
            Path(git('rev-parse', '--show-toplevel', cwd=source)).resolve() != source or
            Path(git('rev-parse', '--path-format=absolute', '--git-common-dir', cwd=source)).resolve() != common or
            Path(git('rev-parse', '--absolute-git-dir', cwd=source)).resolve() == common):
        raise ValueError('Relocation requires a linked worktree of the enrolling repository')
    entries = git('worktree', 'list', '--porcelain', '-z', cwd=root).split('\0\0')
    registered = []
    for entry in entries:
        fields = dict(field.split(' ', 1) for field in entry.split('\0') if ' ' in field)
        if fields.get('worktree') == str(source):
            registered.append(fields)
    expected_branch = 'refs/heads/' + handoff['branch']
    if (len(registered) != 1 or registered[0].get('HEAD') != handoff['head'] or
            registered[0].get('branch') != expected_branch or
            git('rev-parse', 'HEAD', cwd=source) != handoff['head'] or
            git('branch', '--show-current', cwd=source) != handoff['branch'] or
            git('status', '--porcelain', cwd=source)):
        raise ValueError('Relocated writer must remain registered, clean, and at the original branch/HEAD')
    if old.exists() or old.is_symlink():
        if not old.exists() or not old.samefile(source):
            raise ValueError('Existing old source is not the same worktree; preserve both')
        return source
    if record is None:
        raise ValueError('Missing old source requires an explicit verified migration record')
    data = json.loads(Path(record).read_text())
    if not isinstance(data, dict) or data.get('state') != 'verified' or not isinstance(data.get('moves'), list):
        raise ValueError('Migration record is not verified')
    moves = [m for m in data['moves'] if isinstance(m, dict) and
             m.get('old') == str(old) and m.get('new') == str(source)]
    expected = {'head': handoff['head'], 'branch': expected_branch, 'status': '', 'inode': source.stat().st_ino}
    if (len(moves) != 1 or moves[0].get('kind') != 'worktree' or moves[0].get('moved') is not True or
            any(not isinstance(moves[0].get(stage), dict) or
                any(moves[0][stage].get(k) != v for k, v in expected.items()) for stage in ('before', 'after'))):
        raise ValueError('Migration evidence does not identify the original worktree')
    return source
