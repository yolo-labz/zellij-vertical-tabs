// Sophisticated, robust vertical tab sidebar for zellij — Catppuccin Mocha.
// Framed panel + emoji header (session/date/time/mode) + scrolling tab list
// (active always visible) + per-tab bell/sync flair + footer w/ overflow counts.
// Safe events only (TabUpdate/ModeUpdate/Timer/Mouse) — never PaneUpdate.
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

struct State {
    tabs: Vec<TabInfo>,
    session: String,
    mode: String,
    tz: Tz,
    granted: bool,
    armed: bool,
    scroll_start: usize, // first visible tab index (set in render, read on click)
    tab_area_rows: usize,
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
            scroll_start: 0,
            tab_area_rows: 0,
        }
    }
}
register_plugin!(State);

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
                if self.tabs != tabs {
                    self.tabs = tabs;
                    true
                } else {
                    false
                }
            }
            Event::ModeUpdate(mi) => {
                self.session = mi.session_name.unwrap_or_default();
                self.mode = format!("{:?}", mi.mode).to_uppercase();
                true
            }
            Event::Timer(_) => {
                self.armed = false;
                self.arm();
                true
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

    fn render(&mut self, rows: usize, cols: usize) {
        if !self.granted || cols < 10 || rows < HEAD + FOOT + 1 {
            return;
        }
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
        // scroll so the active tab is visible (centered, clamped) — saturating, no OOB
        let active = self.tabs.iter().position(|t| t.active).unwrap_or(0);
        let start = if n <= tab_area {
            0
        } else {
            active.saturating_sub(tab_area / 2).min(n - tab_area)
        };
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
                    Some(tab) => tab_line(tab, i + 1, inner),
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
                format!("{OVERLAY0}\u{2570}{}{RESET}", bar(inner))
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

fn tab_line(tab: &TabInfo, idx: usize, inner: usize) -> String {
    let mut flair = String::new();
    if tab.is_sync_panes_active {
        flair.push_str(" \u{1f517}");
    }
    if tab.has_bell_notification {
        flair.push_str(" \u{1f514}");
    }
    let flair_w = UnicodeWidthStr::width(flair.as_str());
    let name = trunc(&tab.name, inner.saturating_sub(6 + flair_w).max(1));
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

fn trunc(s: &str, max: usize) -> String {
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "\u{2026}".to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthStr::width(ch.to_string().as_str());
        if w + cw > max.saturating_sub(1) {
            out.push('\u{2026}');
            break;
        }
        out.push(ch);
        w += cw;
    }
    out
}

impl State {
    fn arm(&mut self) {
        if !self.armed {
            set_timeout(1.0);
            self.armed = true;
        }
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
