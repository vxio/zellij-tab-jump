# zellij-tab-jump

Pin-and-jump for [Zellij](https://zellij.dev) tabs.

Pin tabs to any keys. For example, bind the picker to `Alt-d`, pin a
few tabs to `f` / `d` / `s` / `a`, and jump with one keystroke.

## Features

- **Pin** any tab to a single-letter slot. Slots are `fdsajkl;` by default
  (configurable).
- **Floating picker** showing pinned tabs first (with their hotkey letter)
  and unpinned tabs below.
- **Toggle**: one key opens the picker; pressing it again hides it.
- **Quick-pin from anywhere**: a second key pins the focused tab without
  opening the picker. Idempotent — re-firing on an already-pinned tab
  just reconfirms the slot, never toggles off. Optional desktop
  notification (off by default; macOS `osascript` / Linux `notify-send`)
  confirms the assigned slot.
- **`Tab`** in the picker toggles to the previously-focused tab.
- **`/`** fuzzy-search filter across all tabs.
- **Persistent** across zellij restarts. State lives in
  `/tmp/zellij-tab-jump-state.json` (which on macOS resolves to the
  per-user `$TMPDIR`), keyed by session name, shared across plugin
  instances via read-modify-write with atomic temp-and-rename writes.

## Install

Prerequisites: Rust toolchain with the `wasm32-wasip1` target installed
(`rustup target add wasm32-wasip1`).

```sh
git clone https://github.com/vxio/zellij-tab-jump
cd zellij-tab-jump
cargo build --release
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zellij-tab-jump.wasm \
   ~/.config/zellij/plugins/tab-jump.wasm
```

## Keybindings

The plugin exposes two **pipe message names** — `toggle` and
`pin-current` — and lets you bind them to whatever keys you want in
`~/.config/zellij/config.kdl`. There are no hard-coded shortcuts.

| Pipe message | What it does |
|---|---|
| `toggle` | Show the picker if hidden, hide it if visible. Pair with `LaunchOrFocusPlugin` so the same key both opens and closes. |
| `pin-current` | Pin the currently focused tab. Idempotent — re-firing on an already-pinned tab is a no-op. Optionally fires a desktop notification (off by default; see [Notifications](#notifications)). |

Drop this block into `config.kdl` and swap the `bind` keys to taste
(e.g. `Ctrl t` / `Ctrl Shift t`, `Alt o` / `Alt Shift o`, …):

```kdl
shared_except "locked" {
    // Open / toggle the picker. Change "Alt d" to any key.
    bind "Alt d" {
        LaunchOrFocusPlugin "file:~/.config/zellij/plugins/tab-jump.wasm" {
            floating true
            move_to_focused_tab true
        }
        MessagePlugin "file:~/.config/zellij/plugins/tab-jump.wasm" {
            name "toggle"
        }
    }

    // Quick-pin the focused tab. Change "Alt D" to any key.
    bind "Alt D" {
        MessagePlugin "file:~/.config/zellij/plugins/tab-jump.wasm" {
            name "pin-current"
        }
    }
}

// Preload so the quick-pin binding always reaches an instance — the
// picker stays suppressed until the toggle key opens it. Plugin config
// (hotkeys, notifications) goes here; see Configuration below.
load_plugins {
    "file:~/.config/zellij/plugins/tab-jump.wasm"
}
```

Required permissions: `ReadApplicationState` and `ChangeApplicationState`
(always); `RunCommands` is requested only when `notifications = "on"`
since it's exclusively used for the notifier shell-out. The plugin
requests permissions on first load and zellij prompts once.

The rest of the docs use **toggle key** and **quick-pin key** to mean
"whatever you bound to the `toggle` and `pin-current` pipe messages."

## Picker keys

| Key | Action |
|---|---|
| configured hotkey letter (default `f` `d` `s` `a` `j` `k` `l` `;`) | jump to the pinned slot |
| `Tab` | toggle to the previously-focused tab |
| `↑` / `↓` + `Enter` | jump to highlighted tab |
| `/` | start fuzzy search; type to filter |
| `g` or `Space` | pin / unpin the highlighted tab |
| `Esc` or `Ctrl-c` | close |

## Notifications

**Off by default.** Opt in with `notifications = "on"` in the plugin
config (see below) to get a desktop toast on every quick-pin
(e.g. `tab-jump: pinned [f] → 3) zed@main`).

When enabled, the plugin probes the host for a notifier and uses
whichever it finds:

| Host | Tool used | Notes |
|---|---|---|
| macOS | `osascript` | Ships with the OS. Banner duration is OS-controlled — set Script Editor's notification style to **Banners** in System Settings → Notifications for auto-dismiss. |
| Linux | `notify-send` | Provided by `libnotify` on most desktop distros. Install via your package manager if missing (e.g. `apt install libnotify-bin`). |
| Other / neither installed | — | Pin still succeeds; the toast is silently dropped. |

Either way, the pin itself always happens — the notification is just
the visual confirmation.

## Configuration

Plugin config goes inside the `load_plugins` block. Any args set here
apply to the preloaded background instance, which the toggle and
quick-pin bindings reuse — so this is the single source of truth.

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/tab-jump.wasm" {
        hotkeys "fdsajkl;"
        notifications "off"
    }
}
```

| key | default | description |
|---|---|---|
| `hotkeys` | `fdsajkl;` | Ordered list of single-char slot letters. Whitespace and duplicate characters are stripped. The number of pin slots equals `len(hotkeys)`; attempting to pin past that fails with an error in the picker. Unpinned tabs are always reachable via arrows or search. |
| `notifications` | `off` | Set to `on` (or `true` / `1` / `yes`) to enable the quick-pin desktop notification. Any other value (or omitting) leaves it off. |

### How many pins can I have?

`len(hotkeys)`. The default is 8 — the home row is the sweet spot for
muscle memory — but you can scale up to ~20 by adding top- and
bottom-row letters. Any printable character works except those reserved
by the picker (`g` = pin, `/` = search, `Space`, `Tab`, `Enter`, `Esc`,
arrows, `Ctrl-c`).

A ~16-slot home + top-row config:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/tab-jump.wasm" {
        hotkeys "fdsajkl;weruioqp"
    }
}
```

## How it works

Two binding paths share one preloaded plugin instance:

```diagram
╭─────────────────╮   LaunchOrFocusPlugin   ╭──────────────────╮
│ toggle key      │ ──────────────────────▶ │ floating picker  │
│                 │   MessagePlugin toggle  │ (per-press show/ │
│                 │ ──────────────────────▶ │  hide)           │
╰─────────────────╯                         ╰────────┬─────────╯
                                                     │ g / hotkey
                                                     ▼
╭─────────────────╮   MessagePlugin         ╭──────────────────╮
│ quick-pin key   │ ──────────────────────▶ │ preloaded bg     │
│                 │      pin-current        │ instance         │
╰─────────────────╯                         ╰────────┬─────────╯
                                                     │ run_command
                                                     ▼
                                            ╭──────────────────╮
                                            │ osascript /      │
                                            │ notify-send      │
                                            ╰──────────────────╯
```

State lives in `/tmp/zellij-tab-jump-state.json`, shared by every
running plugin instance. Each mutation re-reads the file, applies the
change, and writes it back via a temp + rename so a crash mid-write
can't leave a truncated file.

The path is deliberately under `/tmp`: zellij's wasi sandbox only
exposes `/tmp` as writable, so XDG paths under `$HOME` aren't reachable
from the plugin. macOS resolves `/tmp` to the per-user
`$TMPDIR` (e.g. `/private/var/folders/<hash>/T/zellij-<uid>/`), so the
file is already user-isolated there. On Linux it sits in the shared
host `/tmp`; per-session keying inside the JSON keeps concurrent zellij
sessions from clashing, but two users running this plugin on the same
machine would share one file.

The quick-pin key is wired as a `MessagePlugin` pipe (not
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
- **State path**: `/tmp/zellij-tab-jump-state.json` (per-user on macOS
  via `$TMPDIR`; shared on Linux). Keyed by session name so multiple
  concurrent zellij sessions don't trample each other. See
  [How it works](#how-it-works) for why `/tmp` instead of XDG.
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
