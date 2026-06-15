// Sophisticated, robust vertical tab sidebar for zellij — Catppuccin Mocha.
// Framed panel + emoji header (session/date/time/mode) + scrolling tab list
// (active always visible) + per-tab bell/sync flair + footer w/ overflow counts.
// Safe events only (TabUpdate/ModeUpdate/Timer/Mouse) — never PaneUpdate.
//
// Two modes, selected by the `mode` config arg:
//   - default (sidebar): the framed vertical tab bar described above.
//   - `mode "picker"`: a focusable floating fuzzy go-to-or-create surface. It is
//     selectable and subscribes to `Key` (a LOW-frequency event — fired only
//     while focused — so invariant 1 still holds; we never add PaneUpdate).
use chrono::Utc;
use chrono_tz::Tz;
use std::collections::BTreeMap;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

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

// Picker frame: top border + prompt + divider (PHEAD), hint + bottom border (PFOOT).
const PHEAD: usize = 3;
const PFOOT: usize = 2;

// Curated emoji pool for new-tab tagging — no rainbow/slop. The picker prefers
// one NOT already leading an existing tab name (invariant: byte-level
// `starts_with` is char-safe, so multi-byte emoji never panic).
const EMOJI_POOL: &[&str] = &[
    "\u{1f98a}", "\u{1f419}", "\u{1f980}", "\u{1f433}", "\u{1f332}", "\u{1f351}",
    "\u{1f6f0}\u{fe0f}", "\u{1f52d}", "\u{1f9ed}", "\u{1fa90}", "\u{1f344}", "\u{1f41d}",
    "\u{1f989}", "\u{1f335}", "\u{1f422}", "\u{1f98b}", "\u{1f525}", "\u{1f30a}",
    "\u{26a1}", "\u{1f319}", "\u{1f3b2}", "\u{1f9e9}", "\u{1f6e0}\u{fe0f}", "\u{1f680}",
];

struct State {
    tabs: Vec<TabInfo>,
    session: String,
    mode: String,
    tz: Tz,
    granted: bool,
    armed: bool,
    scroll_start: usize, // first visible tab index (set in render, read on click)
    tab_area_rows: usize,
    // picker mode
    picker: bool,
    query: String,
    sel: usize,           // selection index into the filtered list
    pick_scroll: usize,   // first visible filtered row (set in render, read on click)
    pick_area_rows: usize,
}
impl Default for State {
    fn default() -> Self {
        State {
            tabs: Vec::new(), session: String::new(), mode: String::new(),
            tz: chrono_tz::UTC, granted: false, armed: false, scroll_start: 0,
            tab_area_rows: 0, picker: false, query: String::new(), sel: 0,
            pick_scroll: 0, pick_area_rows: 0,
        }
    }
}
register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, cfg: BTreeMap<String, String>) {
        let tz_name = cfg.get("timezone").cloned()
            .or_else(|| std::env::var("TZ").ok())
            .unwrap_or_else(|| "America/Recife".to_string());
        self.tz = tz_name.parse().unwrap_or(chrono_tz::UTC);
        self.picker = cfg.get("mode").map(|m| m == "picker").unwrap_or(false);
        request_permission(&[PermissionType::ReadApplicationState, PermissionType::ChangeApplicationState]);
        if self.picker {
            // Key is low-frequency (only while focused) — invariant 1 still holds.
            subscribe(&[EventType::TabUpdate, EventType::Key, EventType::Mouse, EventType::PermissionRequestResult]);
            set_selectable(true);
        } else {
            subscribe(&[EventType::TabUpdate, EventType::ModeUpdate, EventType::Mouse, EventType::Timer, EventType::PermissionRequestResult]);
            set_selectable(false);
            self.arm();
        }
    }

    fn update(&mut self, event: Event) -> bool {
        if self.picker { self.update_picker(event) } else { self.update_sidebar(event) }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if self.picker { self.render_picker(rows, cols) } else { self.render_sidebar(rows, cols) }
    }
}

impl State {
    fn arm(&mut self) { if !self.armed { set_timeout(1.0); self.armed = true; } }
    fn switch_rel(&mut self, d: i64) {
        let n = self.tabs.len() as i64;
        if n == 0 { return; }
        let cur = self.tabs.iter().position(|t| t.active).unwrap_or(0) as i64;
        let next = (cur + d).rem_euclid(n);
        switch_tab_to(next as u32 + 1);
    }

    // ── sidebar mode ────────────────────────────────────────────────────────
    fn update_sidebar(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(_) => { self.granted = true; set_selectable(false); true }
            Event::TabUpdate(tabs) => { if self.tabs != tabs { self.tabs = tabs; true } else { false } }
            Event::ModeUpdate(mi) => {
                self.session = mi.session_name.unwrap_or_default();
                self.mode = format!("{:?}", mi.mode).to_uppercase();
                true
            }
            Event::Timer(_) => { self.armed = false; self.arm(); true }
            Event::Mouse(Mouse::LeftClick(row, _)) => {
                let r = row as usize;
                let tab_area = self.tab_area_rows;
                if r >= HEAD && r < HEAD + tab_area {
                    let i = self.scroll_start + (r - HEAD);
                    if i < self.tabs.len() { switch_tab_to(i as u32 + 1); }
                }
                false
            }
            Event::Mouse(Mouse::ScrollUp(_)) => { self.switch_rel(-1); false }
            Event::Mouse(Mouse::ScrollDown(_)) => { self.switch_rel(1); false }
            _ => false,
        }
    }

    fn render_sidebar(&mut self, rows: usize, cols: usize) {
        if !self.granted || cols < 10 || rows < HEAD + FOOT + 1 { return; }
        let inner = cols.saturating_sub(1);
        let bar = |n: usize| "\u{2500}".repeat(n);
        let now = Utc::now().with_timezone(&self.tz);
        let sess = if self.session.is_empty() { "zellij" } else { &self.session };
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
        // scroll so the active tab is visible (centered, clamped) — saturating, no OOB
        let active = self.tabs.iter().position(|t| t.active).unwrap_or(0);
        let start = if n <= tab_area { 0 } else { active.saturating_sub(tab_area / 2).min(n - tab_area) };
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
                format!("{OVERLAY0}\u{2502}{RESET} \u{1f3e0} {MAUVE}{BOLD}{}{RESET}", trunc(sess, inner.saturating_sub(4)))
            } else if row == 2 {
                format!("{OVERLAY0}\u{2502}{RESET} \u{1f4c5} {SUBTEXT}{}{RESET}", now.format("%a %d/%m"))
            } else if row == 3 {
                format!("{OVERLAY0}\u{2502}{RESET} \u{1f552} {SUBTEXT}{}{RESET}", now.format("%H:%M:%S"))
            } else if row == 4 {
                format!("{OVERLAY0}\u{2502}{RESET} {} {mode_col}{BOLD}{}{RESET}", mode_emoji, self.mode)
            } else if row == 5 {
                format!("{OVERLAY0}\u{251c}{}{RESET}", bar(inner))
            } else if row >= HEAD && row < HEAD + tab_area {
                let i = start + (row - HEAD);
                match self.tabs.get(i) {
                    Some(tab) => tab_line(tab, i + 1, inner),
                    None => format!("{OVERLAY0}\u{2502}{RESET}"),
                }
            } else if row == foot_div {
                format!("{OVERLAY0}\u{251c}{}{RESET}", bar(inner))
            } else if row == foot_cnt {
                let mut s = format!("{OVERLAY0}\u{2502}{RESET} {OVERLAY1}\u{f0312} {} tabs", n);
                if above > 0 { s.push_str(&format!("  \u{2191}{}", above)); }
                if below > 0 { s.push_str(&format!(" \u{2193}{}", below)); }
                s.push_str(RESET);
                s
            } else if row == foot_end {
                format!("{OVERLAY0}\u{2570}{}{RESET}", bar(inner))
            } else {
                format!("{OVERLAY0}\u{2502}{RESET}")
            };
            print!("{line}");
            if row + 1 < rows { println!(); }
        }
    }

    // ── picker mode ─────────────────────────────────────────────────────────
    fn update_picker(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(_) => { self.granted = true; set_selectable(true); true }
            Event::TabUpdate(tabs) => {
                if self.tabs != tabs {
                    self.tabs = tabs;
                    self.clamp_sel();
                    true
                } else { false }
            }
            Event::Key(k) => self.key(k),
            Event::Mouse(Mouse::LeftClick(row, _)) => { self.click(row as usize); false }
            _ => false,
        }
    }

    fn filtered(&self) -> Vec<usize> {
        self.tabs.iter().enumerate()
            .filter(|(_, t)| fuzzy(&t.name, &self.query))
            .map(|(i, _)| i)
            .collect()
    }

    fn clamp_sel(&mut self) {
        let max = self.filtered().len().saturating_sub(1);
        self.sel = self.sel.min(max);
    }

    fn move_sel(&mut self, d: i64) {
        let len = self.filtered().len();
        if len == 0 { self.sel = 0; return; }
        let max = (len - 1) as i64;
        self.sel = (self.sel as i64 + d).clamp(0, max) as usize;
    }

    fn key(&mut self, k: KeyWithModifier) -> bool {
        use BareKey::*;
        if k.is_key_with_ctrl_modifier(Char('c')) { close_self(); return false; }
        if k.is_key_with_ctrl_modifier(Char('n')) { self.move_sel(1); return true; }
        if k.is_key_with_ctrl_modifier(Char('p')) { self.move_sel(-1); return true; }
        match k.bare_key {
            Esc => { close_self(); false }
            Enter => { self.accept(); false }
            Backspace => { self.query.pop(); self.sel = 0; true }
            Down => { self.move_sel(1); true }
            Up => { self.move_sel(-1); true }
            // plain j/k type into the query (fuzzy finder); move with arrows / ^n ^p.
            Char(c) => { self.query.push(c); self.sel = 0; true }
            _ => false,
        }
    }

    fn click(&mut self, row: usize) {
        if row >= PHEAD && row < PHEAD + self.pick_area_rows {
            let i = self.pick_scroll + (row - PHEAD);
            if i < self.filtered().len() { self.sel = i; self.accept(); }
        }
    }

    // Go to the selected tab; or, if the query matches nothing, create a new tab
    // named "<unused-emoji> <query>". Either way the picker closes.
    fn accept(&mut self) {
        let f = self.filtered();
        if let Some(&idx) = f.get(self.sel) {
            switch_tab_to(idx as u32 + 1);
        } else {
            let q = self.query.trim();
            if !q.is_empty() {
                let name = format!("{} {}", self.pick_emoji(), q);
                focus_or_create_tab(&name);
            }
        }
        close_self();
    }

    // Pick a pool emoji not already leading an existing tab name. Seed from the
    // sub-second clock — `getrandom`/`rand` are fragile under wasm32-wasip1, and
    // `chrono` is already a dependency.
    fn pick_emoji(&self) -> &'static str {
        let used: Vec<&str> = self.tabs.iter()
            .filter_map(|t| EMOJI_POOL.iter().copied().find(|e| t.name.starts_with(e)))
            .collect();
        let candidates: Vec<&'static str> = EMOJI_POOL.iter().copied()
            .filter(|e| !used.contains(e))
            .collect();
        let seed = Utc::now().timestamp_subsec_nanos() as usize;
        if candidates.is_empty() {
            EMOJI_POOL[seed % EMOJI_POOL.len()]
        } else {
            candidates[seed % candidates.len()]
        }
    }

    fn render_picker(&mut self, rows: usize, cols: usize) {
        if !self.granted || cols < 10 || rows < PHEAD + PFOOT + 1 { return; }
        let inner = cols.saturating_sub(1);
        let bar = |n: usize| "\u{2500}".repeat(n);
        let f = self.filtered();
        let n = f.len();
        let area = rows - PHEAD - PFOOT;
        self.pick_area_rows = area;
        let sel = self.sel.min(n.saturating_sub(1));
        let start = if n <= area { 0 } else { sel.saturating_sub(area / 2).min(n - area) };
        self.pick_scroll = start;
        let hint_row = rows - 2;
        let end_row = rows - 1;

        for row in 0..rows {
            let line = if row == 0 {
                format!("{MAUVE}\u{256d}{}{RESET}", bar(inner))
            } else if row == 1 {
                // 🔍 prompt + cursor block
                format!("{MAUVE}\u{2502}{RESET} \u{1f50d} {TEXT}{BOLD}{}{MAUVE}\u{2588}{RESET}", trunc(&self.query, inner.saturating_sub(5)))
            } else if row == 2 {
                format!("{MAUVE}\u{251c}{}{RESET}", bar(inner))
            } else if row >= PHEAD && row < PHEAD + area {
                let li = start + (row - PHEAD);
                match f.get(li).and_then(|&ti| self.tabs.get(ti).map(|t| (ti, t))) {
                    Some((ti, tab)) => picker_line(tab, ti + 1, inner, li == sel),
                    None => format!("{MAUVE}\u{2502}{RESET}"),
                }
            } else if row == hint_row {
                let hint = if n == 0 && !self.query.trim().is_empty() {
                    " \u{23ce} new tab  \u{2022} esc"
                } else {
                    " \u{2191}\u{2193}/^n^p  \u{23ce} go  \u{2022} esc"
                };
                format!("{MAUVE}\u{2502}{RESET}{OVERLAY1}{}{RESET}", trunc(hint, inner))
            } else if row == end_row {
                format!("{MAUVE}\u{2570}{}{RESET}", bar(inner))
            } else {
                format!("{MAUVE}\u{2502}{RESET}")
            };
            print!("{line}");
            if row + 1 < rows { println!(); }
        }
    }
}

// Case-insensitive subsequence match (empty query matches all).
fn fuzzy(name: &str, query: &str) -> bool {
    if query.is_empty() { return true; }
    let mut q = query.chars().flat_map(|c| c.to_lowercase());
    let mut want = q.next();
    for nc in name.chars().flat_map(|c| c.to_lowercase()) {
        match want {
            Some(w) if nc == w => want = q.next(),
            Some(_) => {}
            None => break,
        }
    }
    want.is_none()
}

fn tab_line(tab: &TabInfo, idx: usize, inner: usize) -> String {
    let mut flair = String::new();
    if tab.is_sync_panes_active { flair.push_str(" \u{1f517}"); }
    if tab.has_bell_notification { flair.push_str(" \u{1f514}"); }
    let flair_w = UnicodeWidthStr::width(flair.as_str());
    let name = trunc(&tab.name, inner.saturating_sub(6 + flair_w).max(1));
    if tab.active {
        let body = format!(" \u{258c} {idx:>2} {name}{flair}");
        let pad = inner.saturating_sub(UnicodeWidthStr::width(body.as_str()));
        format!("{OVERLAY0}\u{2502}{SURFACE0_BG}{MAUVE}{BOLD}{body}{}{RESET}", " ".repeat(pad))
    } else {
        format!("{OVERLAY0}\u{2502}{RESET}   {OVERLAY1}{idx:>2}{RESET} {TEXT}{name}{YELLOW}{flair}{RESET}")
    }
}

fn picker_line(tab: &TabInfo, idx: usize, inner: usize, selected: bool) -> String {
    let name = trunc(&tab.name, inner.saturating_sub(6).max(1));
    if selected {
        let body = format!(" \u{258c} {idx:>2} {name}");
        let pad = inner.saturating_sub(UnicodeWidthStr::width(body.as_str()));
        format!("{MAUVE}\u{2502}{SURFACE0_BG}{MAUVE}{BOLD}{body}{}{RESET}", " ".repeat(pad))
    } else {
        format!("{MAUVE}\u{2502}{RESET}   {OVERLAY1}{idx:>2}{RESET} {TEXT}{name}{RESET}")
    }
}

fn trunc(s: &str, max: usize) -> String {
    if UnicodeWidthStr::width(s) <= max { return s.to_string(); }
    if max <= 1 { return "\u{2026}".to_string(); }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthStr::width(ch.to_string().as_str());
        if w + cw > max.saturating_sub(1) { out.push('\u{2026}'); break; }
        out.push(ch); w += cw;
    }
    out
}
