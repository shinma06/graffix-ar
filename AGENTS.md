# Graffix-AR (rebuilt)

This repository was rebuilt from concept in November 2026. The pre-rebuild
Swift implementation and its development harness were removed on purpose:
do not reference or restore pre-rebuild code, docs, or workflow conventions.

## Source of truth

- `docs/concept/` — the original SHIBUYA+FUN PROJECT deck and `VISION.md`.
  Read-only. Product decisions come from there and from the owner, not from
  git history.
- `README.md` — architecture, spray model, build steps, verified/unverified.

## Stack

- Rust core in `core/` (crate `graffix-core`, UniFFI bindings) + Swift shell
  in `Graffix-AR/` (SwiftUI + RealityKit `ARView`). iOS 27 only.
- Computation belongs in the Rust core with unit tests; the Swift shell only
  does I/O (ARKit, sensors, storage) and rendering.
- Keep the toolchain and every dependency on the latest stable release.
  When bumping UniFFI, regenerate the Swift bindings and note that
  bindgen resolves `cargo metadata` from the working directory
  (`scripts/build-xcframework.sh` already runs it inside `core/`).

## Build and checks

The required checks are the two jobs in `.github/workflows/ci.yml`; run the
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
- There are no XCTest/UI tests. A green build is not an app, AR, haptics or
  device pass: state plainly what was not verified.

## How work lands

- A Devin parent session splits work into one task per child session.
- Each child branches off the integration branch named in its task, opens
  one PR into that branch, and gets `.github/workflows/ci.yml` green before
  reporting. The parent reviews and integrates; nobody else merges.
- Never push directly or force-push to `main`/`master`/`develop` or to the
  integration branch. `main` and `develop` are also protected server-side
  by GitHub rulesets.
- The owner's explicit instructions outrank this file. Open product or
  design questions go to the owner; do not guess.
- Human-facing text (PR descriptions, UI, README) is Japanese.

## Session tracking (Entire)

- Entire checkpoints are enabled for Devin sessions via the owner's
  `entire-agent-devin` (external agent protocol). `.devin/hooks.v1.json`
  and `.entire/.gitignore` are committed on purpose.
- Checkpoints (full transcripts) go to the private `checkpoints` remote,
  never into this repository. `.entire/settings.local.json` stays local.
