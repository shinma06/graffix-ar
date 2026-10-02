import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from source_relocation import relocated_source


class SourceRelocationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / 'repo'
        self.root.mkdir()
        self.env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
        self.git('init', '-b', 'main')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@example.invalid')
        (self.root / 'file').write_text('base')
        self.git('add', 'file'); self.git('commit', '-m', 'base')
        self.old = self.root.parent / 'old'
        self.source = self.root.parent / 'moved source'
        self.git('worktree', 'add', '-b', 'codex/89-test', str(self.old))
        self.h = {'source': str(self.old), 'branch': 'codex/89-test', 'head': self.git('rev-parse', 'HEAD')}
        snapshot = {'head': self.h['head'], 'branch': 'refs/heads/' + self.h['branch'], 'status': '', 'inode': self.old.stat().st_ino}
        self.data = {'state': 'verified', 'moves': [{'old': str(self.old), 'new': str(self.source), 'kind': 'worktree',
                    'moved': True, 'before': dict(snapshot), 'after': dict(snapshot)}]}
        self.record = self.root.parent / 'migration.json'
        self.git('worktree', 'move', str(self.old), str(self.source))
        self.write_record()

    def git(self, *args, cwd=None):
        return subprocess.check_output(['git', *args], cwd=cwd or self.root, env=self.env,
                                       text=True, stderr=subprocess.DEVNULL).strip()

    def write_record(self):
        self.record.write_text(json.dumps(self.data))

    def relocate(self, source=None, record=True):
        return relocated_source(self.root, self.h, source or self.source, self.record if record else None, self.git)

    def test_real_move_preserves_original_identity(self):
        self.assertEqual(self.relocate(), self.source)
        self.assertEqual(self.h['source'], str(self.old))

    def test_missing_evidence_and_missing_old_source_are_rejected(self):
        with self.assertRaises(ValueError): self.relocate(record=False)

    def test_same_object_alias_is_accepted_without_record(self):
        self.old.symlink_to(self.source, target_is_directory=True)
        self.assertEqual(self.relocate(record=False), self.source)

    def test_existing_different_or_dangling_old_path_is_rejected(self):
        self.old.mkdir()
        with self.assertRaises(ValueError): self.relocate()
        self.old.rmdir(); self.old.symlink_to(self.root.parent / 'absent')
        with self.assertRaises(ValueError): self.relocate()

    def test_dirty_writer_is_rejected(self):
        (self.source / 'untracked').write_text('new writer')
        with self.assertRaises(ValueError): self.relocate()

    def test_new_writer_commit_is_rejected(self):
        (self.source / 'file').write_text('changed')
        self.git('commit', '-am', 'new writer', cwd=self.source)
        with self.assertRaises(ValueError): self.relocate()

    def test_changed_branch_is_rejected(self):
        self.git('checkout', '-b', 'codex/90-other', cwd=self.source)
        with self.assertRaises(ValueError): self.relocate()

    def test_main_subdirectory_and_other_repository_are_rejected(self):
        nested = self.source / 'sub'; nested.mkdir()
        other = self.root.parent / 'other'
        self.git('clone', str(self.root), str(other))
        for source in (self.root, nested, other):
            with self.subTest(source=source), self.assertRaises(ValueError): self.relocate(source)

    def test_mismatched_or_ambiguous_migration_evidence_is_rejected(self):
        original = json.loads(json.dumps(self.data))
        for key, value in [('head', 'f' * 40), ('branch', 'refs/heads/wrong'), ('status', ' M file'), ('inode', -1)]:
            for stage in ('before', 'after'):
                self.data = json.loads(json.dumps(original)); self.data['moves'][0][stage][key] = value
                self.write_record()
                with self.subTest(stage=stage, key=key), self.assertRaises(ValueError): self.relocate()
        for change in ('state', 'kind', 'moved', 'old', 'duplicate'):
            self.data = json.loads(json.dumps(original))
            if change == 'state': self.data['state'] = 'started'
            elif change == 'duplicate': self.data['moves'] *= 2
            else: self.data['moves'][0][change] = False
            self.write_record()
            with self.subTest(change=change), self.assertRaises(ValueError): self.relocate()


if __name__ == '__main__':
    unittest.main()
