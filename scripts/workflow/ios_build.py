#!/usr/bin/env python3
"""Compile the shared Xcode scheme. AR acceptance still requires a real device."""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PROJECT = 'Graffix-AR.xcodeproj'
SCHEME = 'Mock Up'


def build_command(directory, platform='simulator', signed=False, configuration='Debug'):
    if platform not in ('simulator', 'device') or configuration not in ('Debug', 'Release'):
        raise ValueError('Unsupported platform or configuration')
    if signed and platform != 'device':
        raise ValueError('Signing is only used for a device build')
    destination = 'generic/platform=iOS' + (' Simulator' if platform == 'simulator' else '')
    args = ['xcodebuild', '-project', PROJECT, '-scheme', SCHEME,
            '-configuration', configuration, '-destination', destination,
            '-derivedDataPath', str(directory), 'build']
    if not signed:
        args.append('CODE_SIGNING_ALLOWED=NO')
    return args


def build(directory, platform='simulator', signed=False, configuration='Debug'):
    command = build_command(directory, platform, signed, configuration)
    subprocess.run(command, cwd=ROOT, check=True)
    sdk = 'iphonesimulator' if platform == 'simulator' else 'iphoneos'
    app = Path(directory) / 'Build/Products' / f'{configuration}-{sdk}' / 'Graffix-AR.app'
    if not app.is_dir():
        raise ValueError('Xcode succeeded but the expected app is missing')
    return app


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--derived-data', type=Path, default=ROOT / '.harness-local/DerivedData')
    parser.add_argument('--platform', choices=('simulator', 'device'), default='simulator')
    parser.add_argument('--signed', action='store_true', help='Use existing Xcode signing settings for a device')
    parser.add_argument('--configuration', choices=('Debug', 'Release'), default='Debug')
    args = parser.parse_args()
    build(args.derived_data.resolve(), args.platform, args.signed, args.configuration)
    print('iOS build passed. No XCTest sources are configured; AR/device acceptance is separate.')
