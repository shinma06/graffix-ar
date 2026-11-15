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

## Checks

- `cargo test` in `core/` must be green; also `cargo check` for
  `aarch64-apple-ios` and `aarch64-apple-ios-sim`.
- Swift compilation and on-device AR can only be verified on macOS with
  Xcode. State plainly what was not verified instead of implying a pass.

## Session tracking (Entire)

- Entire checkpoints are enabled for Devin sessions via the owner's
  `entire-agent-devin` (external agent protocol). `.devin/hooks.v1.json`
  and `.entire/.gitignore` are committed on purpose.
- Checkpoints (full transcripts) go to the private `checkpoints` remote,
  never into this repository. `.entire/settings.local.json` stays local.

## Working rules

- The owner's explicit instructions outrank this file.
- `main` is protected by GitHub rules, so changes land through a pull
  request. That is a server-side rule, not a workflow to extend.
