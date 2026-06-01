# zellij-tab-jump

Pin-and-jump for [Zellij](https://zellij.dev) tabs.

Pin 2–4 tabs to home-row letters. Hit `Alt-d` then `f` / `d` / `s` / `a` to jump.

## Features

- **Pin** any tab to a single-letter slot. Slots are `fdsajkl;` by default
  (configurable).
- **Floating picker** showing pinned tabs first (with their hotkey letter)
  and unpinned tabs below.
- **Toggle**: `Alt-d` opens the picker; pressing it again hides it.
- **Quick-pin from anywhere**: `Alt-Shift-d` pins the focused tab without
  opening the picker. A desktop notification (macOS `osascript` /
  Linux `notify-send`) confirms the assigned slot. Idempotent — re-firing
  on an already-pinned tab just reconfirms the slot, never toggles off.
- **`Tab`** in the picker toggles to the previously-focused tab.
- **`/`** fuzzy-search filter across all tabs.
- **Persistent** across zellij restarts — state is keyed by session name,
  shared across plugin instances via a single state file with
  read-modify-write semantics.

## Install

```sh
cargo build --release
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zellij-tab-jump.wasm \
   ~/.config/zellij/plugins/tab-jump.wasm
```

## Keybindings

Add to `~/.config/zellij/config.kdl`:

```kdl
shared_except "locked" {
    // Toggle the picker: opens if hidden, hides if visible.
    bind "Alt d" {
        LaunchOrFocusPlugin "file:~/.config/zellij/plugins/tab-jump.wasm" {
            floating true
            move_to_focused_tab true
            // Optional override:
            // hotkeys "fdsajkl;"
        }
        MessagePlugin "file:~/.config/zellij/plugins/tab-jump.wasm" {
            name "toggle"
        }
    }
    // Pin the currently focused tab and fire a desktop notification.
    bind "Alt D" {
        MessagePlugin "file:~/.config/zellij/plugins/tab-jump.wasm" {
            name "pin-current"
        }
    }
}

// Preload so the Alt-Shift-d pipe always reaches an instance — the picker
// stays suppressed until Alt-d opens it.
load_plugins {
    "file:~/.config/zellij/plugins/tab-jump.wasm"
}
```

Required permissions: `ReadApplicationState`, `ChangeApplicationState`,
`RunCommands` (for the notification shell-out). The plugin requests them
on first load.

## Picker keys

| Key | Action |
|---|---|
| `f` `d` `s` `a` `j` `k` `l` `;` | jump to the pinned slot |
| `Tab` | toggle to the previously-focused tab |
| `↑` / `↓` + `Enter` | jump to highlighted tab |
| `/` | start fuzzy search; type to filter |
| `g` or `Space` | pin / unpin the highlighted tab |
| `Esc` or `Ctrl-c` | close |

## Notifications

`Alt-Shift-d` emits a desktop notification confirming the slot
(e.g. `tab-jump: pinned [f] → 3) zed@main`). The plugin probes for
`osascript` (macOS) first and falls back to `notify-send` (Linux); on
hosts with neither, the pin still succeeds but no toast appears.

On macOS, banner duration is owned by the OS — set Script Editor's
notification style to **Banners** under System Settings → Notifications
to get auto-dismissal.

To suppress notifications entirely, set the `notifications` config arg
(see below).

## Configuration

Pass plugin config inside the `LaunchOrFocusPlugin` block and on
`load_plugins`:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/tab-jump.wasm" {
        hotkeys "fdsajkl;"
        notifications "on"
    }
}
```

| key | default | description |
|---|---|---|
| `hotkeys` | `fdsajkl;` | Ordered list of single-char slot letters. Whitespace ignored. Tabs beyond `len(hotkeys)` can still be reached via arrows or search, just without a single-letter shortcut. |
| `notifications` | `on` | Set to `off` (or `false` / `0` / `no`) to suppress the `Alt-Shift-d` desktop notification. The pin itself still happens. |

## How it works

Two binding paths share one preloaded plugin instance:

```diagram
╭─────────────────╮   LaunchOrFocusPlugin   ╭──────────────────╮
│ Alt-d           │ ──────────────────────▶ │ floating picker  │
│                 │   MessagePlugin toggle  │ (per-press show/ │
│                 │ ──────────────────────▶ │  hide)           │
╰─────────────────╯                         ╰────────┬─────────╯
                                                     │ g / hotkey
                                                     ▼
╭─────────────────╮   MessagePlugin         ╭──────────────────╮
│ Alt-Shift-d     │ ──────────────────────▶ │ preloaded bg     │
│ (quick pin)     │      pin-current        │ instance         │
╰─────────────────╯                         ╰────────┬─────────╯
                                                     │ run_command
                                                     ▼
                                            ╭──────────────────╮
                                            │ osascript /      │
                                            │ notify-send      │
                                            ╰──────────────────╯
```

State lives in a single JSON file (`/tmp/zellij-tab-jump-state.json`),
shared by every running plugin instance for that session. Each mutation
re-reads the file, applies the change, and writes it back — so the
preloaded background instance and the visible picker can't race.

`Alt-Shift-d` is wired as a `MessagePlugin` pipe (not
`LaunchOrFocusPlugin`) so the picker never pops; the preloaded instance
handles it silently and triggers the host notifier.

## Development

```sh
# Build the wasm artifact
cargo build --release

# Install into your local zellij plugin dir
cp target/wasm32-wasip1/release/zellij-tab-jump.wasm \
   ~/.config/zellij/plugins/tab-jump.wasm

# Hot-reload an already-running instance (no zellij restart needed)
zellij action start-or-reload-plugin \
   file:~/.config/zellij/plugins/tab-jump.wasm
```

The crate is single-file (`src/main.rs`, ~850 lines). The `[build]`
target in `.cargo/config.toml` defaults to `wasm32-wasip1` so a plain
`cargo build` produces the wasm artifact.

## Design notes

- **Pinning is manual**, not auto-assigned. The expected workflow: pin
  your 2–3 active tabs once; everything else stays unpinned and
  reachable via arrows/search.
- **State path**: `/tmp/zellij-tab-jump-state.json`, keyed by session name.
  Multiple concurrent zellij sessions don't trample each other.
- **Pins are sticky**: renaming or moving a tab does NOT auto-prune the
  pin. The `TabUpdate` / `get_session_list` tab names don't always match
  the user-facing display names (tab-bar prefixes like `"1) "` aren't
  always part of `TabInfo.name`), so name-based pruning would wipe valid
  pins on every focus change. Clear stale pins with `g` in the picker.
- **Concurrent instances are safe**: every persisted mutation goes through
  a read-modify-write helper so the preloaded picker, the picker shown to
  the user, and any debugging launches can't clobber each other's writes.

## License

MIT
