// Sophisticated, robust vertical tab sidebar for zellij — Catppuccin Mocha.
// Framed panel + emoji header (session/date/time/mode) + scrolling tab list
// (active always visible) + per-tab bell/sync flair + footer w/ overflow counts.
// Safe events only (TabUpdate/ModeUpdate/Timer/Mouse/Visible) — never PaneUpdate.
// The 1 Hz clock ticks ONLY in the visible instance (Visible-gated) and the
// chain self-heals off other events if a Timer is ever dropped.
//
// The pure rendering core (trunc, scroll_window) lives in src/lib.rs so the
// host `cargo test` / proptest / cargo-fuzz can exercise it natively.
use chrono::Utc;
use chrono_tz::Tz;
use std::collections::BTreeMap;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;
use zellij_vertical_tabs::{MarkKind, parse_rescue_mark, scroll_window, trunc};

const MAUVE: &str = "\u{1b}[38;2;203;166;247m";
const BLUE: &str = "\u{1b}[38;2;137;180;250m";
const TEXT: &str = "\u{1b}[38;2;205;214;244m";
const SUBTEXT: &str = "\u{1b}[38;2;166;173;200m";
const OVERLAY1: &str = "\u{1b}[38;2;127;132;156m";
const OVERLAY0: &str = "\u{1b}[38;2;108;112;134m";
const GREEN: &str = "\u{1b}[38;2;166;227;161m";
const PEACH: &str = "\u{1b}[38;2;250;179;135m";
const YELLOW: &str = "\u{1b}[38;2;249;226;175m";
const SURFACE0_BG: &str = "\u{1b}[48;2;49;50;68m";
const BOLD: &str = "\u{1b}[1m";
const RESET: &str = "\u{1b}[0m";

const HEAD: usize = 6;
const FOOT: usize = 3;

// A rescue mark with no refresh self-expires after this long, so a rescue
// script that crashes mid-restore cannot strand stale flair on the sidebar.
const MARK_TTL_MS: i64 = 15 * 60_000;
// Hard backstop on the mark map. It is already bounded by the live tab count
// (only existing tabs are marked), but a pathological session cannot be allowed
// to grow the 16 MB-capped guest — new keys past this cap are dropped.
const MAX_MARKS: usize = 128;

// Build rev for the footer + manifest — set by the flake (self.shortRev);
// plain `cargo build` yields "dev".
const REV: &str = match option_env!("PLUGIN_REV") {
    Some(r) => r,
    None => "dev",
};

pub struct State {
    tabs: Vec<TabInfo>,
    session: String,
    mode: String,
    tz: Tz,
    granted: bool,
    armed: bool,
    // One plugin instance runs per tab (default_tab_template), so with N tabs
    // there are N clocks. Gate the 1 Hz tick on pane visibility: zellij sends
    // Event::Visible(true/false) to tiled plugin panes as their tab gains or
    // loses focus, so only the on-screen instance keeps a timer armed. Default
    // is true because a never-yet-visited tab receives no Visible event at
    // all (zellij only emits it on focus transitions) — those instances tick
    // like today until their first visit, then park when hidden.
    visible: bool,
    last_arm_ms: i64, // when the tick was last armed — drives dead-chain detection
    last_manifest_ms: i64, // last tab-manifest write — throttles the tick refresh
    scroll_start: usize, // first visible tab index (set in render, read on click)
    tab_area_rows: usize,
    // Rescue marks (P2): tab-name → (state, set_ms). Driven by `zellij pipe
    // --name rescue-mark -- '<tab>:<state>'` during a restore so the operator
    // watches convergence in-place. Bounded by the live tab count and
    // TTL-swept every render, so it can never grow the guest's linear memory.
    marks: BTreeMap<String, (MarkKind, i64)>,
}
impl Default for State {
    fn default() -> Self {
        State {
            tabs: Vec::new(),
            session: String::new(),
            mode: String::new(),
            tz: chrono_tz::UTC,
            granted: false,
            armed: false,
            visible: true,
            last_arm_ms: 0,
            last_manifest_ms: 0,
            scroll_start: 0,
            tab_area_rows: 0,
            marks: BTreeMap::new(),
        }
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, cfg: BTreeMap<String, String>) {
        let tz_name = cfg
            .get("timezone")
            .cloned()
            .or_else(|| std::env::var("TZ").ok())
            .unwrap_or_else(|| "America/Recife".to_string());
        self.tz = tz_name.parse().unwrap_or(chrono_tz::UTC);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
            EventType::Timer,
            EventType::Visible,
            EventType::PermissionRequestResult,
        ]);
        set_selectable(false);
        self.arm();
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(_) => {
                self.granted = true;
                set_selectable(false);
                true
            }
            Event::TabUpdate(tabs) => {
                self.heal_timer();
                if self.tabs != tabs {
                    self.tabs = tabs;
                    self.write_manifest();
                    true
                } else {
                    false
                }
            }
            Event::ModeUpdate(mi) => {
                self.heal_timer();
                let session = mi.session_name.unwrap_or_default();
                if session != self.session {
                    self.session = session;
                    self.write_manifest();
                }
                self.mode = format!("{:?}", mi.mode).to_uppercase();
                true
            }
            // Visibility transitions drive the clock lifecycle: on show, force a
            // fresh render (correct clock instantly, even if the timer died while
            // hidden) and restart the tick; on hide, let the in-flight timer park
            // itself in the Timer arm below.
            Event::Visible(v) => {
                self.visible = v;
                if v {
                    self.heal_timer();
                }
                v
            }
            Event::Timer(_) => {
                // Firing <900 ms after the last arm means a duplicate chain
                // (heal_timer false positive) — let this one die unrendered.
                let now_ms = Utc::now().timestamp_millis();
                if now_ms - self.last_arm_ms < 900 {
                    return false;
                }
                self.armed = false;
                if self.visible {
                    self.arm();
                    // keep the manifest mtime fresh (~1/min) so a stale file
                    // reliably means "session not alive", not "idle session"
                    if now_ms - self.last_manifest_ms > 60_000 {
                        self.write_manifest();
                    }
                    true
                } else {
                    false
                }
            }
            Event::Mouse(Mouse::LeftClick(row, _)) => {
                let r = row as usize;
                let tab_area = self.tab_area_rows;
                if r >= HEAD && r < HEAD + tab_area {
                    let i = self.scroll_start + (r - HEAD);
                    if i < self.tabs.len() {
                        switch_tab_to(i as u32 + 1);
                    }
                }
                false
            }
            Event::Mouse(Mouse::ScrollUp(_)) => {
                self.switch_rel(-1);
                false
            }
            Event::Mouse(Mouse::ScrollDown(_)) => {
                self.switch_rel(1);
                false
            }
            _ => false,
        }
    }

    // Rescue marks (P2). `pipe()` is a distinct trait method, NOT a subscribed
    // event — it only fires when an operator (or a restore script) runs
    // `zellij pipe`, so it is inherently low-frequency and invariant-1-safe.
    // A pipe broadcasts to every sidebar instance (one per tab); each keeps its
    // own mark map, so the sidebar shows the same marks whatever tab is focused.
    // Marks touch no files — the P1 manifest is written only on TabUpdate, so
    // there is no write-contention with this path.
    fn pipe(&mut self, msg: PipeMessage) -> bool {
        if msg.name != "rescue-mark" {
            return false;
        }
        let Some(payload) = msg.payload.as_deref() else {
            return false;
        };
        let Some((tab, kind)) = parse_rescue_mark(payload) else {
            return false;
        };
        // Sweep expired marks on every pipe, not only in render(): a hidden
        // instance never renders, so this is its ONLY chance to bound the map —
        // a broadcast pipe must never grow the 16 MB-capped guest (invariant 1).
        let now_ms = Utc::now().timestamp_millis();
        self.marks
            .retain(|_, &mut (_, set)| now_ms.saturating_sub(set) < MARK_TTL_MS);
        match kind {
            MarkKind::Clear if tab == "*" => self.marks.clear(),
            MarkKind::Clear => {
                self.marks.remove(&tab);
            }
            k => {
                // Only mark a tab that actually exists — this bounds the map to
                // the live tab count, so a broadcast pipe can't seed arbitrary
                // keys on every instance. MAX_MARKS is a hard backstop; an
                // existing key may always refresh.
                let known = self.tabs.iter().any(|t| t.name == tab);
                if known && (self.marks.contains_key(&tab) || self.marks.len() < MAX_MARKS) {
                    self.marks.insert(tab, (k, now_ms));
                }
            }
        }
        // Only the on-screen instance needs to repaint; hidden ones stored the
        // mark and repaint on their next Visible(true).
        self.visible
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if !self.granted || cols < 10 || rows < HEAD + FOOT + 1 {
            return;
        }
        // Drop rescue marks that outlived their TTL (a crashed rescue script
        // must not strand flair). Cheap: the map is bounded by the tab count.
        // saturating_sub: never trust the clock to be monotonic (invariant 3).
        let now_ms = Utc::now().timestamp_millis();
        self.marks
            .retain(|_, &mut (_, set)| now_ms.saturating_sub(set) < MARK_TTL_MS);
        let inner = cols.saturating_sub(1);
        let bar = |n: usize| "\u{2500}".repeat(n);
        let now = Utc::now().with_timezone(&self.tz);
        let sess = if self.session.is_empty() {
            "zellij"
        } else {
            &self.session
        };
        let (mode_emoji, mode_col) = match self.mode.as_str() {
            "NORMAL" => ("\u{1f513}", GREEN),
            "LOCKED" => ("\u{1f512}", PEACH),
            "SCROLL" | "SEARCH" | "ENTERSEARCH" => ("\u{1f50d}", BLUE),
            "RENAMETAB" | "RENAMEPANE" => ("\u{270f}\u{fe0f}", BLUE),
            _ => ("\u{2699}\u{fe0f}", BLUE),
        };
        let n = self.tabs.len();
        let tab_area = rows - HEAD - FOOT;
        self.tab_area_rows = tab_area;
        // scroll so the active tab is visible (centered, clamped) — saturating,
        // no OOB; the math is the property-tested scroll_window in src/lib.rs.
        let active = self.tabs.iter().position(|t| t.active).unwrap_or(0);
        let start = scroll_window(n, tab_area, active);
        self.scroll_start = start;
        let shown = tab_area.min(n.saturating_sub(start));
        let above = start;
        let below = n.saturating_sub(start + shown);

        let foot_div = rows - FOOT;
        let foot_cnt = rows - FOOT + 1;
        let foot_end = rows - 1;

        for row in 0..rows {
            let line = if row == 0 {
                format!("{OVERLAY0}\u{256d}{}{RESET}", bar(inner))
            } else if row == 1 {
                format!(
                    "{OVERLAY0}\u{2502}{RESET} \u{1f3e0} {MAUVE}{BOLD}{}{RESET}",
                    trunc(sess, inner.saturating_sub(4))
                )
            } else if row == 2 {
                format!(
                    "{OVERLAY0}\u{2502}{RESET} \u{1f4c5} {SUBTEXT}{}{RESET}",
                    now.format("%a %d/%m")
                )
            } else if row == 3 {
                format!(
                    "{OVERLAY0}\u{2502}{RESET} \u{1f552} {SUBTEXT}{}{RESET}",
                    now.format("%H:%M:%S")
                )
            } else if row == 4 {
                format!(
                    "{OVERLAY0}\u{2502}{RESET} {} {mode_col}{BOLD}{}{RESET}",
                    mode_emoji, self.mode
                )
            } else if row == 5 {
                format!("{OVERLAY0}\u{251c}{}{RESET}", bar(inner))
            } else if row >= HEAD && row < HEAD + tab_area {
                let i = start + (row - HEAD);
                match self.tabs.get(i) {
                    Some(tab) => {
                        let mark = self.marks.get(&tab.name).map(|&(k, _)| k);
                        tab_line(tab, i + 1, inner, mark)
                    }
                    None => format!("{OVERLAY0}\u{2502}{RESET}"),
                }
            } else if row == foot_div {
                format!("{OVERLAY0}\u{251c}{}{RESET}", bar(inner))
            } else if row == foot_cnt {
                let mut s = format!("{OVERLAY0}\u{2502}{RESET} {OVERLAY1}\u{f0312} {} tabs", n);
                if above > 0 {
                    s.push_str(&format!("  \u{2191}{}", above));
                }
                if below > 0 {
                    s.push_str(&format!(" \u{2193}{}", below));
                }
                s.push_str(RESET);
                s
            } else if row == foot_end {
                // version@rev in the closing border — makes mixed-wasm fleets
                // visible at a glance during rescue triage
                let label = format!("{}@{}", env!("CARGO_PKG_VERSION"), REV);
                let lw = UnicodeWidthStr::width(label.as_str());
                if inner >= lw + 4 {
                    format!(
                        "{OVERLAY0}\u{2570}\u{2500} {OVERLAY1}{label}{OVERLAY0} {}{RESET}",
                        bar(inner.saturating_sub(lw + 3))
                    )
                } else {
                    format!("{OVERLAY0}\u{2570}{}{RESET}", bar(inner))
                }
            } else {
                format!("{OVERLAY0}\u{2502}{RESET}")
            };
            print!("{line}");
            if row + 1 < rows {
                println!();
            }
        }
    }
}

fn tab_line(tab: &TabInfo, idx: usize, inner: usize, mark: Option<MarkKind>) -> String {
    let mut flair = String::new();
    // Rescue mark leads the flair cluster (right of the name). It is a width-2,
    // self-coloured emoji, so it drops straight into the same char/width-safe
    // path as the bell/sync flair — no ANSI, no active-band reset, and its
    // width is folded into flair_w below like any other glyph.
    if let Some(g) = mark.map(MarkKind::glyph).filter(|g| !g.is_empty()) {
        flair.push(' ');
        flair.push_str(g);
    }
    if tab.is_sync_panes_active {
        flair.push_str(" \u{1f517}");
    }
    if tab.has_bell_notification {
        flair.push_str(" \u{1f514}");
    }
    // Keep the whole row within `inner`. The fixed lead (│ + 3-col gutter +
    // 2-col index + space ≈ 6 cols) plus the flair must leave room for the
    // name. On a pane too narrow to fit the advisory flair, DROP the flair and
    // keep the name — the mark is a hint, the name is the point. No `.max(1)`
    // here: when there is genuinely no room, trunc(_, 0) yields "" rather than
    // forcing a column that overflows the pane width (invariant 2).
    const LEAD: usize = 6;
    let mut flair_w = UnicodeWidthStr::width(flair.as_str());
    if LEAD + flair_w > inner {
        flair.clear();
        flair_w = 0;
    }
    let name = trunc(&tab.name, inner.saturating_sub(LEAD + flair_w));
    if tab.active {
        let body = format!(" \u{258c} {idx:>2} {name}{flair}");
        let pad = inner.saturating_sub(UnicodeWidthStr::width(body.as_str()));
        format!(
            "{OVERLAY0}\u{2502}{SURFACE0_BG}{MAUVE}{BOLD}{body}{}{RESET}",
            " ".repeat(pad)
        )
    } else {
        format!(
            "{OVERLAY0}\u{2502}{RESET}   {OVERLAY1}{idx:>2}{RESET} {TEXT}{name}{YELLOW}{flair}{RESET}"
        )
    }
}

impl State {
    fn arm(&mut self) {
        if !self.armed {
            set_timeout(1.0);
            self.armed = true;
            self.last_arm_ms = Utc::now().timestamp_millis();
        }
    }
    // Self-healing tick. If a Timer event is ever lost, `armed` sticks true and
    // the chain dies — that instance's clock freezes, which is the observed
    // cross-tab "sidebar out of sync". A pending timer and a dead one look the
    // same from in here, so use the arm timestamp: >5s without the ~1s timer
    // firing means the chain is dead — restart it. A false positive (zellij
    // merely delayed the event >5s) briefly creates a duplicate chain, which
    // the 900 ms guard in the Timer arm collapses within one tick.
    fn heal_timer(&mut self) {
        if !self.visible {
            return;
        }
        let now_ms = Utc::now().timestamp_millis();
        if !self.armed || now_ms - self.last_arm_ms > 5000 {
            self.armed = false;
            self.arm();
        }
    }
    // P1 rescue instrument: the visible instance mirrors the live tab set to
    // /data/tab-manifest.txt (atomic tmp+rename). Written only from a LIVE
    // session, so a stale mtime reliably means the session is gone — immune to
    // the dead-session-dump poisoning class from the 04/07 incident. Restore
    // tooling discovers it via glob + newest mtime + the in-file session name
    // (host path: ~/.cache/zellij/<session>/<plugin-url>/<id>-<client>/).
    // Failures are ignored on purpose: a rescue aid must never break rendering.
    fn write_manifest(&mut self) {
        if !self.visible || self.session.is_empty() {
            return;
        }
        let now_ms = Utc::now().timestamp_millis();
        let mut body = String::with_capacity(96 + self.tabs.len() * 32);
        body.push_str("# zellij-vertical-tabs tab manifest v1\n");
        body.push_str(&format!("plugin: {}@{}\n", env!("CARGO_PKG_VERSION"), REV));
        body.push_str(&format!("session: {}\n", self.session));
        body.push_str(&format!("written_ms: {}\n", now_ms));
        body.push_str(&format!("tabs: {}\n", self.tabs.len()));
        for (i, t) in self.tabs.iter().enumerate() {
            let name = t.name.replace(['\n', '\t'], " ");
            let mark = if t.active { "*" } else { "-" };
            body.push_str(&format!("{}\t{}\t{}\n", i + 1, mark, name));
        }
        if std::fs::write("/data/tab-manifest.tmp", &body).is_ok() {
            let _ = std::fs::rename("/data/tab-manifest.tmp", "/data/tab-manifest.txt");
        }
        self.last_manifest_ms = now_ms;
    }
    fn switch_rel(&mut self, d: i64) {
        let n = self.tabs.len() as i64;
        if n == 0 {
            return;
        }
        let cur = self.tabs.iter().position(|t| t.active).unwrap_or(0) as i64;
        let next = (cur + d).rem_euclid(n);
        switch_tab_to(next as u32 + 1);
    }
}
