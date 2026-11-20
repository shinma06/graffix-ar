# Graffix-AR

iOS app (Shibuya Graffix AR): move the phone like a spray can to paint digital graffiti on real walls. Rust core (`core/`, crate `graffix-core`) exposed through UniFFI to a Swift shell (`Graffix-AR/`, SwiftUI + RealityKit `ARView`), iOS 27 only. Product facts: `docs/project.md`, `README.md`, `docs/concept/` (read-only source of truth).

## Work

A Devin parent session delegates one task per child session, reviews the result and integrates it. Entire records the sessions.

- One task → one child session → one branch `devin/<slug>` from `main` → one PR into `main`. Children never push to `main`, `master` or `develop`, never force-push and never merge: the parent session reviews and merges into `main` after both required checks are green.
- This applies to every Devin session working on this repository, including sessions the parent did not start: no session merges a PR it created into `main`, even when CI is green. `main` only receives PRs whose two required checks are green and that the parent session has reviewed and merged.
- A PR is ready when both `.github/workflows/ci.yml` jobs, `Rust core (Linux)` and `iOS app (Xcode)`, are green on its head. Fix real failures; never weaken a check.
- Stay inside the files your task names; if you must touch others, keep it minimal and say so in the PR.
- Product and design decisions the task and the repository do not settle go to the owner: stop and ask with the options instead of guessing.
- Report what CI verified and what it did not (on-device AR, LiDAR, haptics, UI behaviour).

## Build and test

```bash
# core/ (any host): what the Linux CI job runs
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo check --locked --lib --target aarch64-apple-ios
cargo check --locked --lib --target aarch64-apple-ios-sim

# repository root (macOS + Xcode): what the iOS CI job runs
./scripts/build-xcframework.sh
git diff --exit-code -- Graffix-AR/Generated
xcodebuild build -project Graffix-AR.xcodeproj -scheme Graffix-AR \
  -destination 'generic/platform=iOS' CODE_SIGNING_ALLOWED=NO
xcodebuild build -project Graffix-AR.xcodeproj -scheme Graffix-AR \
  -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO
```

On a Linux host only the core commands run; the macOS CI job is the only Swift compile check. Commit `Graffix-AR/Generated/graffix_core.swift` byte-for-byte as generated (it contains trailing whitespace; stripped copies fail the drift check). When bumping UniFFI, regenerate the bindings; bindgen resolves `cargo metadata` from the working directory, so run it inside `core/` as the script does.

Passing CI does not mean the app works: the only XCTest/UI tests are the works-detail UI tests (`Graffix-ARUITests`, run in CI on an iOS 27 Simulator with seeded fixture works); nothing else in the Swift shell is exercised, and the Simulator has no ARKit world tracking, so on-device AR and haptics acceptance is a human check on a LiDAR iPhone. Say which was verified.

## Rules

- Computation goes in the Rust core with unit tests; the Swift shell only does I/O (ARKit, sensors, storage, sharing) and rendering. `cargo test` stays green at every push.
- Keep the toolchain and every dependency on the latest stable release.
- `docs/concept/` is read-only. Do not reference or restore pre-rebuild code, docs or workflow.
- User-facing text (PRs, UI, README, docs) is Japanese; agent-only instructions are English.
- Entire records Devin sessions. In every new worktree run `entire-agent-devin bootstrap --checkpoint-repo shinma06/entire-checkpoints`; checkpoints must only go to the private `checkpoints` remote, never `origin`. `.devin/hooks.v1.json`, `.entire/.gitignore` and `.entire/settings.json` (checkpoint remote pinned to the private `shinma06/entire-checkpoints`) are committed on purpose; `.entire/settings.local.json` stays local.
- Never commit credentials.

The previous multi-agent operation is preserved; see `docs/legacy-ops/README.md`.
