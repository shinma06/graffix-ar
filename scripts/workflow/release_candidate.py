#!/usr/bin/env python3
"""Seal an iOS candidate once and verify the same bytes for device acceptance."""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
import subprocess

import ios_build
from verification import git_read, scoped_candidate

ROOT = Path(__file__).resolve().parents[2]


def sha256(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def build(source, directory, scope_issue=None, platform='device', signed=False):
    if not re.fullmatch(r'[0-9a-f]{40}', source):
        raise ValueError('Full candidate SHA required')
    if git_read('rev-parse', 'HEAD') != source or git_read('status', '--porcelain'):
        raise ValueError('Build from the clean, fixed candidate checkout')
    if scope_issue is None:
        git_read('fetch', '--no-tags', 'origin', 'develop')
        git_read('merge-base', '--is-ancestor', source, 'origin/develop')
    else:
        git_read('fetch', '--no-tags', 'origin', 'main')
        scoped_candidate(git_read('rev-parse', 'origin/main'), source, scope_issue)
    directory = Path(directory).resolve()
    if directory.is_relative_to(ROOT.resolve()):
        raise ValueError('Candidate output must be outside the checkout')
    directory.mkdir(parents=True, exist_ok=False)
    xcode = subprocess.check_output(['xcodebuild', '-version'], text=True).strip()
    app = ios_build.build(directory / 'DerivedData', platform, signed, 'Release')
    if git_read('rev-parse', 'HEAD') != source or git_read('status', '--porcelain'):
        raise ValueError('Source changed during the build; do not use this candidate')
    with (app / 'Info.plist').open('rb') as stream:
        info = plistlib.load(stream)
    artifact = directory / 'Graffix-AR.app.zip'
    subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(app), str(artifact)], check=True)
    manifest = {'schema': 1, 'source': source, 'platform': platform, 'signed': signed,
                'artifact': artifact.name, 'artifact_sha256': sha256(artifact),
                'executable_sha256': sha256(app / info['CFBundleExecutable']),
                'bundle_id': info['CFBundleIdentifier'], 'version': info['CFBundleShortVersionString'],
                'build': info['CFBundleVersion'], 'xcode': xcode}
    (directory / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return manifest


def candidate(directory, expected_hash):
    directory = Path(directory)
    manifest = json.loads((directory / 'manifest.json').read_text())
    if (manifest.get('schema') != 1 or not re.fullmatch(r'[0-9a-f]{40}', manifest.get('source', ''))
            or manifest.get('artifact') != 'Graffix-AR.app.zip'
            or manifest.get('platform') not in ('simulator', 'device')
            or type(manifest.get('signed')) is not bool
            or not re.fullmatch(r'[0-9a-f]{64}', expected_hash)):
        raise ValueError('Invalid candidate identity')
    if manifest.get('artifact_sha256') != expected_hash or sha256(directory / manifest['artifact']) != expected_hash:
        raise ValueError('Candidate bytes changed; rebuild and repeat acceptance')
    return manifest


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    create = sub.add_parser('build')
    create.add_argument('--source', required=True)
    create.add_argument('--directory', type=Path, required=True)
    create.add_argument('--scope-issue', type=int)
    create.add_argument('--platform', choices=('device', 'simulator'), default='device')
    create.add_argument('--signed', action='store_true')
    verify = sub.add_parser('check')
    verify.add_argument('--directory', type=Path, required=True)
    verify.add_argument('--sha256', required=True)
    args = parser.parse_args()
    result = (build(args.source, args.directory, args.scope_issue, args.platform, args.signed)
              if args.action == 'build' else candidate(args.directory, args.sha256))
    print(json.dumps(result, ensure_ascii=False, indent=2))
