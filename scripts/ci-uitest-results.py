#!/usr/bin/env python3
"""Read UI test .xcresult bundles in CI (macOS, Xcode 16+ `xcresulttool`).

  interrupted BUNDLE  print `-only-testing` IDs of failed tests that ran only once
                      (timed out or the runner died: Xcode's retry did not run them)
  failed BUNDLE       print IDs of failed tests that Xcode already retried
  summary BUNDLE...   per-test result / duration / runs table to stdout and the
                      job summary; a `::warning::` for every test that ran >1 time
"""
import json
import os
import subprocess
import sys


def test_cases(bundle):
    raw = subprocess.run(
        ["xcrun", "xcresulttool", "get", "test-results", "tests", "--path", bundle],
        check=True, capture_output=True, text=True).stdout

    def walk(node, target, suite):
        kind = node.get("nodeType")
        if kind in ("Unit test bundle", "UI test bundle"):
            target = node.get("name", "").removesuffix(".xctest")
        elif kind == "Test Suite":
            suite = node.get("name", "")
        elif kind == "Test Case":
            yield f"{target}/{suite}/{node.get('name', '').removesuffix('()')}", node
        for child in node.get("children", []):
            yield from walk(child, target, suite)

    for root in json.loads(raw).get("testNodes", []):
        yield from walk(root, "", "")


def runs(case):
    return [c for c in case.get("children", []) if c.get("nodeType") == "Repetition"]


def seconds(node):
    d = node.get("durationInSeconds")
    return f"{d:.1f} s" if isinstance(d, (int, float)) else node.get("duration", "?")


def main(cmd, bundles):
    if cmd in ("interrupted", "failed"):
        for test_id, case in test_cases(bundles[0]):
            if case.get("result") == "Failed" and (len(runs(case)) < 2) == (cmd == "interrupted"):
                print(test_id)
        return
    out = []
    for bundle in bundles:
        out += [f"### {os.path.basename(bundle)}", "",
                "| Test | Result | Duration | Runs |", "|---|---|---|---|"]
        try:
            cases = list(test_cases(bundle))
        except Exception as e:  # incomplete bundle: report, do not mask the test step's result
            out += [f"| (could not read: {e}) | | | |", ""]
            continue
        for test_id, case in cases:
            reps = runs(case)
            detail = "; ".join(f"{r.get('name', '?')}: {r.get('result', '?')} ({seconds(r)})"
                               for r in reps) or "1 run"
            out.append(f"| {test_id} | {case.get('result')} | {seconds(case)} | {detail} |")
            if len(reps) > 1:
                print(f"::warning title=UI test retried::{test_id} ran {len(reps)} times: {detail}")
        out.append("")
    text = "\n".join(out)
    print(text)
    if "GITHUB_STEP_SUMMARY" in os.environ:
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as f:
            f.write("## UI tests\n\n" + text + "\n")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2:])
