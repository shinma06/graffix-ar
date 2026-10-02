#!/usr/bin/env python3
"""Report executable presence only. Do not read credentials or contact providers."""
import json
from pathlib import Path
import shutil
import subprocess


def main():
    tools = {name: shutil.which(name) is not None for name in
             ['git', 'gh', 'python3', 'xcodebuild', 'codex', 'claude', 'agent']}
    xcode = subprocess.run(['xcodebuild', '-version'], capture_output=True, text=True) if tools['xcodebuild'] else None
    result = subprocess.run(['git', 'config', '--get', 'core.hooksPath'], capture_output=True, text=True)
    print(json.dumps({
        'executables_present': tools,
        'repository_entrypoints': {name: Path(name).exists() for name in
                                  ['AGENTS.md', 'CLAUDE.md', '.agents/skills', '.claude/skills', '.cursor/rules']},
        'harness_hooks_selected': result.returncode == 0 and result.stdout.strip() == '.githooks',
        'xcode_selected': bool(xcode and xcode.returncode == 0),
        'xcode_version': xcode.stdout.strip() if xcode and xcode.returncode == 0 else None,
        'not_checked': ['authentication', 'MCP connectivity', 'plugin trust', 'OS permissions',
                        'server rulesets', 'scheduler execution', 'remote hosts'],
    }, indent=2))


if __name__ == '__main__':
    main()
