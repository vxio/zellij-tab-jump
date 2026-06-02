# Changelog

All notable changes to this project will be documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] — 2026-06-02

Initial public release.

### Added
- Floating picker with pinned tabs first and unpinned tabs below.
- `toggle` pipe message: opens the picker; re-firing closes it (when
  paired with `LaunchOrFocusPlugin`).
- `pin-current` pipe message: idempotent pin of the focused tab,
  with an optional desktop notification (off by default).
- Picker keys: configured hotkey letters jump to slot,
  `Tab` toggles to previously-focused tab,
  `↑`/`↓`/`Enter` pick from list, `/` fuzzy search,
  `g`/`Space` pin/unpin highlighted tab, `Esc`/`Ctrl-c` close.
- Configurable `hotkeys` (default `fdsajkl;`) — slot count is
  `len(hotkeys)`; duplicates and whitespace stripped.
- Configurable `notifications` (default `off`); cross-platform
  via `osascript` (macOS) or `notify-send` (Linux).
- Persistent state at `/tmp/zellij-tab-jump-state.json`, keyed by
  session name, with atomic temp-and-rename writes.
- GitHub Actions release workflow producing prebuilt `tab-jump.wasm`.

[Unreleased]: https://github.com/vxio/zellij-tab-jump/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/vxio/zellij-tab-jump/releases/tag/v0.1.0
