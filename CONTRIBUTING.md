# Contributing to TWrite

Thanks for helping out. This guide covers the workflow, checks, and
conventions so a pull request can be reviewed and merged without friction.

## Ways to contribute

- **Report a bug**: open an issue with the bug report template (repro steps,
  expected vs actual, OS and commit).
- **Propose a feature**: open an issue with the feature request template
  (problem, proposed change, alternatives, scope).
- **Fix or build**: pick an open issue, small PRs preferred over large ones.
  If no issue exists for the work, open one first so the design is agreed
  before code is written.

## Setup

You need a stable Rust toolchain. On Linux, GPUI also needs system libraries:

```sh
sudo apt-get install -y pkg-config libfontconfig1-dev libwayland-dev \
  libx11-xcb-dev libxkbcommon-x11-dev libxkbcommon-dev
```

Then verify the checkout builds and the examples run:

```sh
cargo build --workspace
cargo run --example simple
```

## Codebase structure

Three crates plus examples and docs:

- `crates/twrite` is the facade most users depend on. It only re-exports
  the other two layers (`Editor` and hooks), so changes here are rare.
- `crates/twrite-core` is headless: rope buffer, cursor movement, undo
  history, syntax spans, folding, and the `EditorHook` trait with stock
  hooks (search, markdown, vim helpers). It has no GPUI dependency, so
  anything testable without a window belongs here.
- `crates/twrite-gpui` is the interactive layer: `Editor` entity
  (`editor/`), canvas rendering and text shaping (`canvas.rs`), input
  translation (`input.rs`), theming (`theme.rs`), configuration
  (`config.rs`), and the `gpui-new-api` shims (`compat.rs`).
- `examples/` holds the ordered tutorial apps (`simple` first) plus the
  standalone `examples/gpui-compat-zed` recipe for newer GPUI.
- `docs/` is the mdBook guide; API reference is generated with rustdoc.

Rule of thumb: buffer and hook logic in `twrite-core`, GPUI interaction in
`twrite-gpui`, and new user-facing behavior ships with an example update.

## Workflow

1. Branch from `main`: `fix/<slug>` for bugs, `feat/<slug>` for features,
   `docs/<slug>` for docs, `chore/<slug>` for tooling.
2. For behavior changes, write a failing test first, then implement.
3. Keep the diff focused: one issue, one PR. Do not mix refactors with fixes.
4. Use full words for names, write comments that explain *why* (not what),
   and propagate errors with `?` instead of discarding them.

## Checks

All of these must pass before pushing. They mirror the `CI` workflow.

```sh
cargo fmt --all -- --check
cargo install cargo-hack  # once; enumerates feature combinations
cargo hack clippy --workspace --feature-powerset --exclude-features gpui-new-api -- -D warnings
cargo hack test --lib --tests --workspace --feature-powerset --depth 1 --exclude-features gpui-new-api
cargo hack test --doc --workspace --feature-powerset --depth 1 --exclude-features gpui-new-api
cargo hack check --workspace --all-targets --feature-powerset --exclude-features gpui-new-api
```

If docs were touched, also run:

```sh
cargo doc --no-deps --workspace --lib --features markdown,wayland,x11,font-kit
mdbook build docs
```

## GPUI variants

The default build targets registry `gpui 0.2.2`. Newer GPUI (gpui-ce, zed,
bezel) is supported behind the `gpui-new-api` feature plus a
`[patch.crates-io]` pin; see the complete recipe in
`examples/gpui-compat-zed`. Because the flag only compiles against patched
GPUI, it is excluded from the matrices above: never use a bare
`--all-features` in CI-adjacent commands.

## Changelog, versioning, commits

- Add user-facing changes to `CHANGELOG.md` under `Unreleased` (Keep a
  Changelog format).
- Version bumps are a maintainer step at release time: patch for fixes,
  minor for features or public API changes.
- Commit messages are conventional (`fix:`, `feat:`, `docs:`, `chore:`,
  `ci:`) with no em dashes.
- Fill in `.github/pull_request_template.md`: summary, changes, testing
  evidence, docs checkboxes, breaking changes.

## Release (maintainers)

1. Bump the workspace version and the path pins in the root `Cargo.toml`,
   then regenerate the lockfile with `cargo metadata`.
2. Move the `CHANGELOG.md` entry from `Unreleased` to the versioned section
   with the release date.
3. Push a release branch, open a PR, merge it.
4. Tag `vX.Y.Z` after the merge: the `Release` workflow validates, publishes
   `twrite-core`, `twrite-gpui`, then `twrite` to crates.io in order, and
   creates the GitHub release.
