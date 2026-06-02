# Changelog

All notable changes to this project will be documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] — 2026-06-02

### Added
- **Recent tabs (MRU) section** in the picker. Tabs you've focused
  recently are listed below the pinned section in most-recent-first
  order. The top slot is labeled `[tab]` (jumps with the `Tab` key);
  subsequent slots get hotkeys from `recent_hotkeys` (default `1`–`9`).
- New `recent_hotkeys` config option (default `123456789`). The 1st
  MRU slot is always reserved for `Tab`, so `1`/`2`/… address the
  2nd, 3rd, … entries. If a key appears in both `hotkeys` and
  `recent_hotkeys`, the pinned slot wins.

### Changed
- Picker layout is now two sections: **Pinned → Recent**. The Recent
  section folds in never-visited tabs (by tab position) and the
  currently-focused tab at the end.
- `Tab` key now jumps to the top of the Recent section (labeled
  `[tab]`) instead of the formerly-tracked "previous tab" field.
- Dropped the `(last tab)` row annotation — the `[tab]` hotkey label
  is now the indicator.

### Fixed
- **`Alt-d` → `Esc` → `Alt-d` no longer warps you to a different tab.**
  Dismissing the picker now calls `close_self()` instead of
  `hide_self()`. Zellij remembers a suppressed plugin pane's tab and
  re-focuses it there on the next `LaunchOrFocusPlugin` — even with
  `move_to_focused_tab true`. Closing the pane outright forces the
  next launch to create a fresh floating pane on the user's current
  tab.

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

[Unreleased]: https://github.com/vxio/zellij-tab-jump/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/vxio/zellij-tab-jump/releases/tag/v0.2.0
[0.1.0]: https://github.com/vxio/zellij-tab-jump/releases/tag/v0.1.0
