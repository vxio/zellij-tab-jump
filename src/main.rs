//! zellij-tab-jump
//!
//! Pin-and-jump for Zellij tabs.
//!
//! Two pipe-message entry points — bind them to whatever zellij keys
//! you like in `config.kdl`:
//!
//! * `toggle` — show the floating picker if hidden, hide it if visible.
//!   Pair with `LaunchOrFocusPlugin` so the same key opens and closes.
//! * `pin-current` — pin the focused tab. Idempotent: re-firing on an
//!   already-pinned tab just reconfirms the slot, never toggles off.
//! * Pin/unpin inside the picker with `g` on the selected row.
//! * Pinned tabs claim a slot letter from the configured `hotkeys` set
//!   (default `fdsajkl;`); pressing the letter in the picker jumps there.
//! * `Tab` in the picker toggles to the previously-focused tab.
//! * `/` starts a fuzzy search over tab names; arrows + Enter jump.
//! * Desktop notification confirms pins made outside the picker, when
//!   opted in with `notifications = "on"`. Off by default. Backed by
//!   `osascript` on macOS and `notify-send` on Linux; silent no-op on
//!   hosts with neither.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;

use serde::{Deserialize, Serialize};
use zellij_tile::prelude::*;

const DEFAULT_HOTKEYS: &str = "fdsajkl;";

/// The wasi sandbox that zellij runs plugins in only exposes `/tmp`
/// as writable. On macOS that resolves to the per-user
/// `$TMPDIR` (e.g. `/private/var/folders/<hash>/T/zellij-<uid>/`),
/// so `/tmp` paths are already isolated per user. On Linux `/tmp`
/// is the shared host tmp; the file is namespaced just by its name.
const STATE_PATH: &str = "/tmp/zellij-tab-jump-state.json";

// ─── persisted state ──────────────────────────────────────────────────────

/// Everything stored across plugin reloads. Keyed by session name so multiple
/// concurrent zellij sessions on the same machine don't trample each other.
#[derive(Default, Serialize, Deserialize)]
struct PersistedState {
    /// session name → (tab name → slot index). Slots survive zellij restarts;
    /// renaming a tab loses its pin (the pin stays under the old name until
    /// the user unpins or re-pins).
    #[serde(default)]
    pinned: BTreeMap<String, BTreeMap<String, usize>>,
    /// session name → the tab name the user was on just before the current
    /// one. Used by `<Tab>` to toggle between two tabs.
    #[serde(default)]
    previous_tab: BTreeMap<String, String>,
    /// session name → the tab name that was highlighted when the picker
    /// last closed. Restored when the picker reopens.
    #[serde(default)]
    last_selected: BTreeMap<String, String>,
}

// ─── modes ────────────────────────────────────────────────────────────────

#[derive(PartialEq, Eq, Clone, Copy, Default)]
enum Mode {
    #[default]
    Normal,
    Search,
}

// ─── plugin state ─────────────────────────────────────────────────────────

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    current_session: Option<String>,
    current_tab_name: Option<String>,

    persisted: PersistedState,
    hotkeys: Vec<char>,
    /// When false (the default), the `pin-current` pipe skips the desktop
    /// notification call. Enabled by setting the plugin arg
    /// `notifications = "on"` in `config.kdl`.
    notifications_enabled: bool,

    mode: Mode,
    search_term: String,
    selected_index: usize,
    /// Transient error banner; cleared on the next key press.
    error: Option<String>,

    /// True once a TabUpdate has populated `tabs` for the current session.
    /// Used to gate the `pending_pin_current` deferred action when the plugin
    /// is pipe-launched (the pipe message arrives before TabUpdate fires).
    tabs_loaded: bool,
    /// Set by a `pin-current` pipe message when tabs aren't loaded yet. The
    /// next TabUpdate consumes it and performs the pin.
    pending_pin_current: bool,
    /// Whether to close the plugin pane after the next render. Set when we
    /// jump to a tab so the picker doesn't linger on screen.
    pending_close: bool,
    /// True between `Visible(true)` and `Visible(false)`. Read by the
    /// `toggle` pipe handler to decide whether the toggle key should hide
    /// the picker (visible → hide) or fall through to the paired
    /// `LaunchOrFocusPlugin` action (hidden → show).
    is_visible: bool,
    /// One-line banner shown at the top of the picker after the user pins
    /// a tab with `g` from inside the picker. Cleared on Timer expiry.
    pin_toast: Option<String>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, cfg: BTreeMap<String, String>) {
        self.reload_state();

        let raw = cfg
            .get("hotkeys")
            .map(String::as_str)
            .unwrap_or(DEFAULT_HOTKEYS);
        self.hotkeys = dedupe_hotkeys(raw);
        if self.hotkeys.is_empty() {
            self.hotkeys = DEFAULT_HOTKEYS.chars().collect();
        }

        self.notifications_enabled = cfg
            .get("notifications")
            .map(|v| matches!(v.as_str(), "on" | "true" | "1" | "yes"))
            .unwrap_or(false);

        let mut perms = vec![
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ];
        if self.notifications_enabled {
            perms.push(PermissionType::RunCommands);
        }
        request_permission(&perms);
        subscribe(&[
            EventType::TabUpdate,
            EventType::SessionUpdate,
            EventType::Key,
            EventType::Visible,
            EventType::Timer,
            EventType::PermissionRequestResult,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::TabUpdate(tabs) => {
                self.absorb_tabs(tabs);
                self.maybe_flush_pending_pin();
                true
            }
            Event::SessionUpdate(infos, _) => {
                let new_session = infos
                    .iter()
                    .find(|s| s.is_current_session)
                    .map(|s| s.name.clone());
                if new_session != self.current_session {
                    self.current_session = new_session;
                    self.tabs.clear();
                    self.current_tab_name = None;
                    self.tabs_loaded = false;
                }
                self.maybe_flush_pending_pin();
                true
            }
            Event::Visible(true) => {
                self.is_visible = true;
                self.reload_state();
                self.mode = Mode::Normal;
                self.search_term.clear();
                self.restore_selection();
                true
            }
            Event::Visible(false) => {
                self.is_visible = false;
                self.record_selection();
                false
            }
            Event::Timer(_) => {
                if self.pin_toast.is_some() {
                    self.pin_toast = None;
                    return true;
                }
                false
            }
            Event::Key(key) => self.handle_key(key),
            Event::PermissionRequestResult(_) => false,
            _ => false,
        }
    }

    fn pipe(&mut self, msg: PipeMessage) -> bool {
        match msg.name.as_str() {
            "pin-current" => {
                // Hidden plugin panes don't receive TabUpdate, so a pipe from
                // a background instance would see stale `current_tab_name`.
                // Pull fresh tab/session state from the server, and re-read
                // pins from disk in case a sibling instance modified them.
                self.refresh_from_server();
                self.reload_state();
                if self.tabs_loaded && self.current_session.is_some() {
                    self.pin_current_and_notify();
                } else {
                    self.pending_pin_current = true;
                }
            }
            // Paired with `LaunchOrFocusPlugin` on the toggle key to give
            // the binding toggle semantics. The kdl runs LaunchOrFocus
            // first, then this pipe; events are queued, so when we read
            // `is_visible` here it still reflects the state *before* the
            // binding fired.
            "toggle" if self.is_visible => {
                hide_self();
            }
            _ => {}
        }
        false
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // Only re-read pins from disk when the picker is actually visible.
        // The preloaded background instance still gets render calls and we
        // don't want to thrash IO on event churn; but we must still let
        // `draw` run, because the first render after `Visible(true)` for a
        // freshly-shown pane can race with the event delivery.
        if self.is_visible {
            self.reload_state();
        }
        self.draw(rows, cols);
        if self.pending_close {
            self.pending_close = false;
            hide_self();
        }
    }
}

// ─── tab snapshot ─────────────────────────────────────────────────────────

impl State {
    /// Apply a fresh list of tabs from zellij: update `current_tab_name` and
    /// record the focus change in `previous_tab` if the user moved off a
    /// known tab. Pins are intentionally *not* auto-pruned here — the live
    /// tab name reported by `TabUpdate` / `get_session_list` doesn't always
    /// match the display name we pinned under (e.g. tab-bar prefixes like
    /// "1) " aren't part of `TabInfo.name`), so name-based pruning would
    /// wipe valid pins on every focus change. Stale pins linger until the
    /// user clears them with `g` in the picker.
    fn absorb_tabs(&mut self, tabs: Vec<TabInfo>) {
        let new_focus = tabs.iter().find(|t| t.active).map(|t| t.name.clone());
        let focus_change = match (self.current_tab_name.as_ref(), new_focus.as_ref()) {
            (Some(old), Some(new)) if old != new => Some(old.clone()),
            _ => None,
        };
        if let (Some(old), Some(session)) = (focus_change, self.current_session.clone()) {
            self.mutate_state(|s| {
                s.previous_tab.insert(session, old);
            });
        }
        self.current_tab_name = new_focus;
        self.tabs = tabs;
        self.tabs_loaded = true;
    }

    /// Synchronously fetch the current session's tabs from the server.
    /// Used by the pipe handler because zellij does not deliver `TabUpdate`
    /// to a hidden plugin pane — without this refresh, `current_tab_name`
    /// can be stale when a `pin-current` pipe fires.
    fn refresh_from_server(&mut self) {
        let Ok(snapshot) = get_session_list() else { return };
        let Some(current) = snapshot
            .live_sessions
            .into_iter()
            .find(|s| s.is_current_session)
        else {
            return;
        };
        self.current_session = Some(current.name);
        self.absorb_tabs(current.tabs);
    }
}

// ─── persistence ──────────────────────────────────────────────────────────

impl State {
    /// Read-modify-write the on-disk state. ALWAYS use this for any change
    /// that needs to persist — never mutate `self.persisted` and write
    /// separately. Multiple plugin instances run concurrently (picker +
    /// per-press `pin-current` workers); without reload-before-write they
    /// stomp each other's pins. This reloads disk into `self.persisted`,
    /// applies the mutation, then writes the result back via a temp +
    /// rename so a crash mid-write can't leave a truncated JSON file.
    fn mutate_state<F: FnOnce(&mut PersistedState)>(&mut self, f: F) {
        self.reload_state();
        f(&mut self.persisted);
        let Ok(raw) = serde_json::to_string(&self.persisted) else {
            return;
        };
        let tmp = format!("{STATE_PATH}.tmp");
        if fs::write(&tmp, raw).is_ok() {
            let _ = fs::rename(&tmp, STATE_PATH);
        }
    }

    /// Re-read pins/previous-tab/last-selected from disk. Concurrent plugin
    /// instances only share state through the state file, so anything that
    /// depends on the latest pins must call this first. A missing file is
    /// treated as empty state; an unreadable / malformed file is left in
    /// place and `self.persisted` keeps its current contents.
    fn reload_state(&mut self) {
        match fs::read_to_string(STATE_PATH) {
            Ok(raw) => {
                if let Ok(p) = serde_json::from_str::<PersistedState>(&raw) {
                    self.persisted = p;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.persisted = PersistedState::default();
            }
            Err(_) => {}
        }
    }
}

// ─── pin/unpin ────────────────────────────────────────────────────────────

impl State {
    fn session_pins(&self) -> Option<&BTreeMap<String, usize>> {
        self.persisted.pinned.get(self.current_session.as_ref()?)
    }

    fn slot_for_tab(&self, name: &str) -> Option<usize> {
        self.session_pins()?.get(name).copied()
    }

    fn hotkey_for_slot(&self, slot: usize) -> Option<char> {
        self.hotkeys.get(slot).copied()
    }

    fn slot_for_hotkey(&self, c: char) -> Option<usize> {
        self.hotkeys.iter().position(|&k| k == c)
    }

    /// Toggle the pin state of `tab_name`. If pinned, unpin (slot frees up).
    /// If unpinned, claim the lowest unused slot. Returns a short description
    /// of what happened, suitable for a banner ("pinned [f] → name" /
    /// "unpinned name"). Returns None on error.
    fn toggle_pin(&mut self, tab_name: &str) -> Option<String> {
        let Some(session) = self.current_session.clone() else {
            self.set_error("no current session".into());
            return None;
        };
        let hotkeys = self.hotkeys.clone();
        let name_owned = tab_name.to_string();
        let mut outcome: Option<Result<String, String>> = None;
        self.mutate_state(|s| {
            let entry = s.pinned.entry(session).or_default();
            if entry.remove(&name_owned).is_some() {
                outcome = Some(Ok(format!("unpinned: {}", name_owned)));
                return;
            }
            let Some(slot) = next_free_slot(entry, hotkeys.len()) else {
                outcome = Some(Err("all pin hotkeys are in use".into()));
                return;
            };
            entry.insert(name_owned.clone(), slot);
            let label = format_slot(&hotkeys, slot);
            outcome = Some(Ok(format!("pinned {} → {}", label, name_owned)));
        });
        match outcome {
            Some(Ok(msg)) => Some(msg),
            Some(Err(e)) => {
                self.set_error(e);
                None
            }
            None => None,
        }
    }

    /// Pin the focused tab if it isn't pinned yet. Truly idempotent: the
    /// read-modify-write inside `mutate_state` re-checks pinned state after
    /// reloading from disk, so a stale in-memory snapshot can't cause a
    /// pin → unpin flip via `toggle_pin`. Used by the `pin-current` pipe.
    fn pin_current_only(&mut self) -> Option<String> {
        let Some(name) = self.current_tab_name.clone() else {
            self.set_error("no focused tab".into());
            return None;
        };
        let Some(session) = self.current_session.clone() else {
            self.set_error("no current session".into());
            return None;
        };
        let hotkeys = self.hotkeys.clone();
        let mut outcome: Option<Result<String, String>> = None;
        self.mutate_state(|s| {
            let entry = s.pinned.entry(session).or_default();
            if let Some(&slot) = entry.get(&name) {
                outcome = Some(Ok(format!(
                    "already pinned {} → {}",
                    format_slot(&hotkeys, slot),
                    name
                )));
                return;
            }
            let Some(slot) = next_free_slot(entry, hotkeys.len()) else {
                outcome = Some(Err("all pin hotkeys are in use".into()));
                return;
            };
            entry.insert(name.clone(), slot);
            outcome = Some(Ok(format!(
                "pinned {} → {}",
                format_slot(&hotkeys, slot),
                name
            )));
        });
        match outcome {
            Some(Ok(msg)) => Some(msg),
            Some(Err(e)) => {
                self.set_error(e);
                None
            }
            None => None,
        }
    }

    /// Consume a pending `pin-current` pipe message only once we know both
    /// the current session and the focused tab name. Pipe messages can arrive
    /// before SessionUpdate / TabUpdate populate that state, so we defer
    /// until both are present rather than silently failing with "no current
    /// session".
    fn maybe_flush_pending_pin(&mut self) {
        if !self.pending_pin_current {
            return;
        }
        if self.current_session.is_none() || self.current_tab_name.is_none() {
            return;
        }
        self.pending_pin_current = false;
        self.pin_current_and_notify();
    }

    /// Pin the focused tab and surface the result as a desktop notification.
    /// Inside the picker, `g` uses the in-pane toast instead — this path is
    /// for the `pin-current` pipe where we don't want to pop the picker.
    fn pin_current_and_notify(&mut self) {
        let Some(msg) = self.pin_current_only() else { return };
        if self.notifications_enabled {
            notify_user(&msg);
        }
    }
}

/// Emit a desktop notification by shelling out to whichever notifier the
/// host provides — `osascript` on macOS, `notify-send` on Linux. We delegate
/// the detection to `sh` so a single `run_command` call works on both
/// platforms; the WASM plugin can't `cfg!(target_os = …)` because it's
/// compiled once for `wasm32-wasip1` regardless of host.
///
/// The message is passed as a positional shell parameter (`$1`) so tab
/// names containing quotes or backslashes can't break the inner command.
///
/// On hosts with neither command, the notification is silently dropped. The
/// pin itself still succeeds — only the toast is missing.
///
/// Notifications are off by default. Opt in with the plugin config option
/// `notifications = "on"` (or `true` / `1` / `yes`).
fn notify_user(msg: &str) {
    let script = r#"if command -v osascript >/dev/null 2>&1; then
    osascript -e "display notification \"$1\" with title \"tab-jump\""
elif command -v notify-send >/dev/null 2>&1; then
    notify-send 'tab-jump' "$1"
fi"#;
    run_command(
        &["sh", "-c", script, "sh", msg],
        BTreeMap::new(),
    );
}

/// Lowest free slot index strictly less than `max_slots`.
fn next_free_slot(pins: &BTreeMap<String, usize>, max_slots: usize) -> Option<usize> {
    let used: BTreeSet<usize> = pins.values().copied().collect();
    (0..max_slots).find(|s| !used.contains(s))
}

/// Render a slot as either its hotkey (`[f]`) or numeric fallback (`slot 3`).
fn format_slot(hotkeys: &[char], slot: usize) -> String {
    hotkeys
        .get(slot)
        .map(|c| format!("[{}]", c))
        .unwrap_or_else(|| format!("slot {}", slot + 1))
}

/// Strip whitespace and duplicate characters from a hotkey config string,
/// preserving the order of first occurrence. Duplicate hotkeys would create
/// unreachable slots, so we silently drop them.
fn dedupe_hotkeys(raw: &str) -> Vec<char> {
    let mut seen = BTreeSet::new();
    raw.chars()
        .filter(|c| !c.is_whitespace())
        .filter(|c| seen.insert(*c))
        .collect()
}

// ─── view helpers ─────────────────────────────────────────────────────────

impl State {
    /// All tabs in display order: pinned first (sorted by slot), then unpinned
    /// (in tab position order).
    fn visible_tabs(&self) -> Vec<&TabInfo> {
        let pins = self.session_pins();
        let mut pinned: Vec<&TabInfo> = Vec::new();
        let mut unpinned: Vec<&TabInfo> = Vec::new();
        for t in &self.tabs {
            if pins.is_some_and(|p| p.contains_key(&t.name)) {
                pinned.push(t);
            } else {
                unpinned.push(t);
            }
        }
        pinned.sort_by_key(|t| {
            pins.and_then(|p| p.get(&t.name).copied())
                .unwrap_or(usize::MAX)
        });
        unpinned.sort_by_key(|t| t.position);
        pinned.extend(unpinned);
        pinned
    }

    /// Filtered list (search-aware). Both pinned and unpinned participate in
    /// the filter.
    fn filtered_tabs(&self) -> Vec<&TabInfo> {
        let v = self.visible_tabs();
        if self.mode != Mode::Search || self.search_term.is_empty() {
            return v;
        }
        let needle = self.search_term.to_lowercase();
        v.into_iter()
            .filter(|t| fuzzy_match(&t.name.to_lowercase(), &needle))
            .collect()
    }

    fn clamp_selection(&mut self) {
        let len = self.filtered_tabs().len();
        if len == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= len {
            self.selected_index = len - 1;
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.filtered_tabs().len() as i32;
        if len == 0 {
            self.selected_index = 0;
            return;
        }
        let new = (self.selected_index as i32 + delta).rem_euclid(len);
        self.selected_index = new as usize;
    }

    /// Restore `selected_index` to the previous-focused tab so `Enter`
    /// double-taps as a tab toggle. Falls back to the last highlighted tab,
    /// then to the first row.
    fn restore_selection(&mut self) {
        let visible = self.filtered_tabs();
        if visible.is_empty() {
            self.selected_index = 0;
            return;
        }
        let session = self.current_session.clone();
        let prev = session
            .as_ref()
            .and_then(|s| self.persisted.previous_tab.get(s).cloned());
        let last = session
            .as_ref()
            .and_then(|s| self.persisted.last_selected.get(s).cloned());
        for candidate in [prev, last].iter().flatten() {
            if let Some(idx) = visible.iter().position(|t| &t.name == candidate) {
                self.selected_index = idx;
                return;
            }
        }
        self.selected_index = 0;
    }

    fn record_selection(&mut self) {
        let name = self
            .filtered_tabs()
            .get(self.selected_index)
            .map(|t| t.name.clone());
        let Some(session) = self.current_session.clone() else {
            return;
        };
        self.mutate_state(|s| match name {
            Some(n) => {
                s.last_selected.insert(session, n);
            }
            None => {
                s.last_selected.remove(&session);
            }
        });
    }

    fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
    }
}

// ─── input handling ───────────────────────────────────────────────────────

impl State {
    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let had_error = self.error.is_some();
        self.error = None;
        let dirty = match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Search => self.handle_search_key(key),
        };
        dirty || had_error
    }

    fn handle_normal_key(&mut self, key: KeyWithModifier) -> bool {
        if key.has_modifiers(&[KeyModifier::Ctrl]) {
            if let BareKey::Char('c') = key.bare_key {
                hide_self();
                return false;
            }
            return false;
        }
        if !key.has_no_modifiers() {
            return false;
        }
        match key.bare_key {
            BareKey::Char('/') => {
                self.mode = Mode::Search;
                self.search_term.clear();
                self.selected_index = 0;
                true
            }
            BareKey::Tab => {
                self.jump_to_previous();
                true
            }
            BareKey::Down => {
                self.move_selection(1);
                true
            }
            BareKey::Up => {
                self.move_selection(-1);
                true
            }
            BareKey::Enter => {
                self.confirm_selection();
                true
            }
            BareKey::Esc => {
                hide_self();
                false
            }
            BareKey::Char(' ') | BareKey::Char('g') => {
                self.toggle_pin_selected();
                true
            }
            BareKey::Char(c) => {
                if let Some(slot) = self.slot_for_hotkey(c) {
                    self.jump_to_slot(slot, c);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn handle_search_key(&mut self, key: KeyWithModifier) -> bool {
        if key.has_modifiers(&[KeyModifier::Ctrl]) {
            if key.bare_key == BareKey::Char('/') {
                self.search_term.clear();
                self.selected_index = 0;
                return true;
            }
            if key.bare_key == BareKey::Char('c') {
                hide_self();
                return false;
            }
            return false;
        }
        if !key.has_no_modifiers() {
            return false;
        }
        match key.bare_key {
            BareKey::Esc => {
                self.mode = Mode::Normal;
                self.search_term.clear();
                self.selected_index = 0;
                true
            }
            BareKey::Enter => {
                self.confirm_selection();
                true
            }
            BareKey::Backspace => {
                self.search_term.pop();
                self.selected_index = 0;
                true
            }
            BareKey::Down => {
                self.move_selection(1);
                true
            }
            BareKey::Up => {
                self.move_selection(-1);
                true
            }
            BareKey::Char(c) => {
                self.search_term.push(c);
                self.selected_index = 0;
                true
            }
            _ => false,
        }
    }
}

// ─── actions ──────────────────────────────────────────────────────────────

impl State {
    fn jump_to_tab(&mut self, name: &str) {
        if self.current_tab_name.as_deref() == Some(name) {
            hide_self();
            return;
        }
        // Record the focus change synchronously: zellij doesn't deliver
        // TabUpdate to hidden panes, so relying on `absorb_tabs` to capture
        // the previous tab would miss the case where the picker hides
        // immediately after jumping.
        if let (Some(session), Some(old)) =
            (self.current_session.clone(), self.current_tab_name.clone())
        {
            self.mutate_state(|s| {
                s.previous_tab.insert(session, old);
            });
        }
        go_to_tab_name(name);
        self.pending_close = true;
    }

    fn confirm_selection(&mut self) {
        let target = self
            .filtered_tabs()
            .get(self.selected_index)
            .map(|t| t.name.clone());
        if let Some(name) = target {
            self.jump_to_tab(&name);
        }
    }

    fn jump_to_slot(&mut self, slot: usize, hotkey: char) {
        let target = self
            .session_pins()
            .and_then(|p| p.iter().find(|(_, &s)| s == slot).map(|(n, _)| n.clone()));
        match target {
            Some(name) => self.jump_to_tab(&name),
            None => self.set_error(format!("no tab pinned to '{}'", hotkey)),
        }
    }

    fn jump_to_previous(&mut self) {
        let Some(session) = self.current_session.as_ref() else {
            self.set_error("no current session".into());
            return;
        };
        if let Some(prev) = self.persisted.previous_tab.get(session).cloned() {
            if self.tabs.iter().any(|t| t.name == prev) {
                self.jump_to_tab(&prev);
                return;
            }
        }
        self.set_error("no previous tab".into());
    }

    fn toggle_pin_selected(&mut self) {
        let target = self
            .filtered_tabs()
            .get(self.selected_index)
            .map(|t| t.name.clone());
        match target {
            Some(name) => {
                if let Some(msg) = self.toggle_pin(&name) {
                    self.pin_toast = Some(msg);
                    set_timeout(1.4);
                }
            }
            None => self.set_error("nothing selected".into()),
        }
    }
}

// ─── rendering ────────────────────────────────────────────────────────────

const CSI: &str = "\u{1b}[";

impl State {
    fn draw(&mut self, rows: usize, cols: usize) {
        self.clamp_selection();
        let mut lines: Vec<String> = Vec::with_capacity(rows);

        let visible = self.filtered_tabs();
        // Show pinned tabs that are actually present in the current tab
        // list; stale pins for renamed/closed tabs aren't counted.
        let live_pin_count = visible
            .iter()
            .filter(|t| self.slot_for_tab(&t.name).is_some())
            .count();

        let session = self.current_session.as_deref().unwrap_or("<no session>");
        let tab_count = self.tabs.len();
        lines.push(format!(
            " {CSI}1;36mTabs ({tab_count}){CSI}0m  {CSI}90m· {live_pin_count} pinned  · session: {CSI}33m{session}{CSI}0m"
        ));

        if let Some(toast) = self.pin_toast.as_deref() {
            lines.push(format!(" {CSI}1;32m✓ {}{CSI}0m", toast));
        }

        let prompt = match self.mode {
            Mode::Search => format!(
                " {CSI}36m/{CSI}0m{}{CSI}5m_{CSI}0m",
                self.search_term
            ),
            Mode::Normal => String::new(),
        };
        lines.push(prompt);
        lines.push(String::new());

        if visible.is_empty() {
            lines.push(format!(" {CSI}90m(no tabs){CSI}0m"));
        }
        let last_tab_name = self
            .current_session
            .as_ref()
            .and_then(|s| self.persisted.previous_tab.get(s).cloned());
        let mut separator_shown = false;
        for (i, t) in visible.iter().enumerate() {
            let slot = self.slot_for_tab(&t.name);
            let is_pinned = slot.is_some();

            if !is_pinned && !separator_shown && live_pin_count > 0 && i > 0 {
                lines.push(format!(" {CSI}90m─── unpinned ───{CSI}0m"));
                separator_shown = true;
            }

            let hotkey_str = match slot.and_then(|s| self.hotkey_for_slot(s)) {
                Some(c) => {
                    let color = if t.active { "90" } else { "36" };
                    format!("{CSI}{color}m[{c}]{CSI}0m")
                }
                None => format!("{CSI}90m · {CSI}0m"),
            };
            let is_selected = i == self.selected_index;
            let marker = if is_selected { "▶" } else { " " };
            let current_str = if t.active {
                format!("  {CSI}33m(current){CSI}0m")
            } else if last_tab_name.as_deref() == Some(t.name.as_str()) {
                format!("  {CSI}35m(last tab){CSI}0m")
            } else {
                String::new()
            };
            let name = if t.name.is_empty() { "(unnamed)" } else { t.name.as_str() };

            let line = if is_selected {
                format!(
                    "{CSI}48;5;236m {marker} {hotkey_str}{CSI}48;5;236m {CSI}1;37m{name}{CSI}0m{CSI}48;5;236m{current_str}{CSI}0m"
                )
            } else {
                format!(" {marker} {hotkey_str} {CSI}37m{name}{CSI}0m{current_str}")
            };
            lines.push(line);
        }

        let footer = match self.mode {
            Mode::Normal => " hotkey · /search · Tab=last · g=pin · ↵=jump · Esc",
            Mode::Search => " type to filter · ↵=jump · ^/ clear · Esc cancel",
        };
        let footer_line = format!("{CSI}90m{footer}{CSI}0m");
        let error_line = self
            .error
            .as_deref()
            .map(|e| format!("{CSI}1;31m {e}{CSI}0m"));

        // Reserve the footer row (always) and the error row (when present),
        // but only if we have the rows to spare. Saturating math keeps tiny
        // pane sizes from producing negative `max_content`.
        let footer_reserved = usize::from(rows > 0);
        let error_reserved =
            usize::from(error_line.is_some() && rows > footer_reserved);
        let max_content = rows.saturating_sub(footer_reserved + error_reserved);
        lines.truncate(max_content);
        while lines.len() < max_content {
            lines.push(String::new());
        }
        if error_reserved == 1 {
            if let Some(e) = error_line {
                lines.push(e);
            }
        }
        if footer_reserved == 1 {
            lines.push(footer_line);
        }

        let _ = cols;
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "{CSI}2J{CSI}H");
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                let _ = write!(out, "\r\n");
            }
            let _ = write!(out, "{line}{CSI}K");
        }
        let _ = out.flush();
    }
}

// ─── utils ────────────────────────────────────────────────────────────────

/// Subsequence fuzzy match — true if every char of `needle` appears in
/// `haystack` in order. Both should be lowercased by the caller.
fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let mut it = haystack.chars();
    for nc in needle.chars() {
        if it.find(|&hc| hc == nc).is_none() {
            return false;
        }
    }
    true
}
