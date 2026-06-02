# zellij-tab-jump

Pin-and-jump for [Zellij](https://zellij.dev) tabs. Built against the
`zellij-tile` 0.44 API.

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
  per-user `$TMPDIR`), keyed by session name, with atomic
  temp-and-rename writes that survive crashes and concurrent instances.

## Install

### Option 1 — Zellij URL loading

Zellij can fetch a remote `.wasm` and cache it. No Rust toolchain
needed; nothing to download manually. Reference the release URL
directly in your bindings:

```kdl
shared_except "locked" {
    bind "Alt d" {
        LaunchOrFocusPlugin "https://github.com/vxio/zellij-tab-jump/releases/latest/download/tab-jump.wasm" {
            floating true
            move_to_focused_tab true
        }
        MessagePlugin "https://github.com/vxio/zellij-tab-jump/releases/latest/download/tab-jump.wasm" {
            name "toggle"
        }
    }
}

load_plugins {
    "https://github.com/vxio/zellij-tab-jump/releases/latest/download/tab-jump.wasm"
}
```

Zellij downloads the wasm on first run and caches it indefinitely
under `~/.cache/zellij/plugins/` keyed by URL. **The `latest/download`
URL does not auto-refresh** — Zellij sees the same URL and reuses the
cache. See [Updating](#updating) for how to refresh.

### Option 2 — Download prebuilt wasm

If you'd rather keep the file local:

```sh
mkdir -p ~/.config/zellij/plugins
curl -L \
  https://github.com/vxio/zellij-tab-jump/releases/latest/download/tab-jump.wasm \
  -o ~/.config/zellij/plugins/tab-jump.wasm
```

Then use `file:~/.config/zellij/plugins/tab-jump.wasm` in your kdl
config (see [Keybindings](#keybindings)).

### Option 3 — Build from source

For local development or platforms where the prebuilt artifact won't do.

Prerequisites: Rust toolchain with the `wasm32-wasip1` target
(`rustup target add wasm32-wasip1`).

```sh
git clone https://github.com/vxio/zellij-tab-jump
cd zellij-tab-jump
cargo build --release
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zellij-tab-jump.wasm \
   ~/.config/zellij/plugins/tab-jump.wasm
```

## Updating

Zellij has no built-in plugin update mechanism. Pick whichever fits
your install style:

**For Options 1 and 2** — re-fetch the latest release and clear
Zellij's URL cache (or replace the file in `~/.config/zellij/plugins/`),
then hot-reload running pickers:

```sh
curl -L https://github.com/vxio/zellij-tab-jump/releases/latest/download/tab-jump.wasm \
  -o ~/.config/zellij/plugins/tab-jump.wasm
zellij action start-or-reload-plugin file:~/.config/zellij/plugins/tab-jump.wasm
```

For URL-loaded installs (Option 1), also clear the cached download:

```sh
rm -rf ~/.cache/zellij/plugins
```

**For Option 3 (source)** — `git pull && cargo build --release` and
re-copy the wasm.

To pin a specific version instead of `latest`, swap the URL for a
tagged release: `…/releases/download/v0.2.0/tab-jump.wasm`.

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

`load_plugins` preloads a single plugin instance at startup. Both
bindings target *that* instance via pipe messages, so there's only
ever one process holding state:

```diagram
╭───────────────╮  pipe "toggle"      ╭──────────────────╮     ╭──────────────╮
│ toggle key    │ ──────────────────▶ │                  │ ──▶ │ state.json   │
╰───────────────╯                     │  tab-jump        │     ╰──────────────╯
                                      │  (one preloaded  │
╭───────────────╮  pipe "pin-current" │   instance)      │     ╭──────────────╮
│ quick-pin key │ ──────────────────▶ │                  │ ──▶ │ osascript /  │
╰───────────────╯                     ╰──────────────────╯     │ notify-send  │
                                                               │ (optional)   │
                                                               ╰──────────────╯
```

State lives in `/tmp/zellij-tab-jump-state.json`. Every mutation
re-reads the file, applies the change, and writes it back via a temp +
rename, so a crash mid-write can't leave a truncated file and any
sibling plugin instance (e.g. from a stray `LaunchPlugin`) can't
overwrite recent pins.

The path is deliberately under `/tmp`: zellij's wasi sandbox only
exposes `/tmp` as writable, so XDG paths under `$HOME` aren't reachable
from the plugin. macOS resolves `/tmp` to the per-user
`$TMPDIR` (e.g. `/private/var/folders/<hash>/T/zellij-<uid>/`), so the
file is already user-isolated there. On Linux it sits in the shared
host `/tmp`; per-session keying inside the JSON keeps concurrent zellij
sessions from clashing, but two users running this plugin on the same
machine would share one file.

The quick-pin key is wired as a `MessagePlugin` pipe (not
`LaunchOrFocusPlugin`) so the picker never pops; the preloaded
instance handles it silently and triggers the host notifier.

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

MIT — see [LICENSE](./LICENSE).
