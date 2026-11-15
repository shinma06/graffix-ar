# Graffix-AR (rebuilt)

This repository was rebuilt from concept in November 2026. The pre-rebuild
Swift implementation and its development harness were removed on purpose:
do not reference pre-rebuild code, docs, or workflow conventions.

## Source of truth

- `docs/concept/` — the original SHIBUYA+FUN PROJECT deck and `VISION.md`.
  Product decisions come from there and from the owner, not from git history.
- `README.md` — architecture, spray model, build steps, verified/unverified.

## Stack

- Rust core in `core/` (crate `graffix-core`, UniFFI bindings) + Swift shell
  in `Graffix-AR/`. iOS 27 only.
- Keep the toolchain and every dependency on the latest stable release.
  When bumping UniFFI, regenerate the Swift bindings and note that
  bindgen resolves `cargo metadata` from the working directory
  (`scripts/build-xcframework.sh` already runs it inside `core/`).

## Build and checks

The required checks are the jobs in `.github/workflows/ci.yml`; run the
same commands locally where the platform allows.

- **Rust core (Linux)** — in `core/`: `cargo test --locked`,
  `cargo clippy --locked --all-targets --all-features -- -D warnings`,
  `cargo check --locked --lib --target aarch64-apple-ios` and
  `--target aarch64-apple-ios-sim`. `cargo test` stays green at every push.
- **iOS app (Xcode)** — macOS only: `./scripts/build-xcframework.sh`
  (XCFramework + UniFFI Swift bindings), then
  `git diff --exit-code -- Graffix-AR/Generated` and
  `xcodebuild build -project Graffix-AR.xcodeproj -scheme Graffix-AR`
  for `generic/platform=iOS` and `generic/platform=iOS Simulator`
  with `CODE_SIGNING_ALLOWED=NO`.
- Commit `Graffix-AR/Generated/graffix_core.swift` byte-for-byte as
  generated (it contains trailing whitespace; stripped copies fail CI).
- There are no XCTest/UI tests. A green build is not an app, AR,
  haptics or device pass: state plainly what was not verified.

## Session tracking (Entire)

- Entire checkpoints are enabled for Devin sessions via the owner's
  `entire-agent-devin` (external agent protocol). `.devin/hooks.v1.json`
  and `.entire/.gitignore` are committed on purpose.
- Checkpoints (full transcripts) go to the private `checkpoints` remote,
  never into this repository. `.entire/settings.local.json` stays local.

## Working rules

- The owner's explicit instructions outrank this file.
- Computation belongs in the Rust core with unit tests; the Swift shell
  only does I/O (ARKit, sensors, storage) and rendering.
- `docs/concept/` is read-only.
- `main` and `develop` are protected by GitHub rulesets, so changes land
  through a pull request. Never push directly or force-push to
  `main`/`master`/`develop`. That is a server-side rule, not a workflow to
  extend; `.github/*-ruleset.json` mirror the server settings and are the
  owner's.
