// Pure rendering core — host-buildable on purpose so `cargo test`, proptest
// and cargo-fuzz exercise the two functions that guard the load-bearing
// invariants (see docs/design-principles.md):
//   - `trunc`: char/width-safe truncation (invariant: emoji/width tricks in
//     tab names must never panic the renderer or overflow the pane width)
//   - `scroll_window`: clamped viewport math (invariant: no OOB indexing,
//     active tab always inside the window)
// No zellij imports here — the plugin shell lives in src/main.rs behind the
// wasm target gate.
use unicode_width::UnicodeWidthStr;

/// Truncate `s` to at most `max` display columns, char-safely, appending `…`
/// when anything was cut. `max == 0` yields an empty string.
pub fn trunc(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    if max == 1 {
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

/// First visible row of the tab viewport: centers `active` in a window of
/// `area` rows over `n` tabs, clamped so the window never runs past the end.
pub fn scroll_window(n: usize, area: usize, active: usize) -> usize {
    if n <= area {
        0
    } else {
        active.saturating_sub(area / 2).min(n - area)
    }
}

/// A rescue mark's state, parsed from a `rescue-mark` pipe payload. Rendered as
/// a per-tab status glyph so a restore operator sees convergence in-place
/// instead of polling `ps` (rescue instrument P2 — see docs/design-principles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Ok,    // ✅ converged / done
    Wip,   // ⏳ in progress
    Fail,  // ❌ failed
    Clear, // remove the mark (never stored)
}

impl MarkKind {
    /// The status glyph. Width-2, self-coloured emoji so it drops into the
    /// existing bell/sync flair slot with no ANSI and no band-reset (invariant
    /// 2: width-safe). `Clear` has no glyph — it is a removal, never rendered.
    pub fn glyph(self) -> &'static str {
        match self {
            MarkKind::Ok => "\u{2705}",
            MarkKind::Wip => "\u{23f3}",
            MarkKind::Fail => "\u{274c}",
            MarkKind::Clear => "",
        }
    }
}

/// Parse a `rescue-mark` pipe payload of the form `<tab>:<state>`.
///
/// Splits on the LAST `:` so a tab name may itself contain colons. Tab `*` is
/// the wildcard (only meaningful with `clear` → clear every mark). An unknown
/// state or an empty tab yields `None` — a rescue aid must be inert on garbage
/// rather than paint noise. Pure + panic-free: the host proptest/fuzz harness
/// exercises it against hostile input.
///
/// Key contract: marks are keyed by the **trimmed** tab name. This is the
/// deliberate name-key choice (survives tab reorder where a positional index
/// would smear; orphaned marks TTL out in the plugin). Two consequences the
/// caller must accept: a tab whose name has leading/trailing whitespace is not
/// separately addressable (the space is trimmed), and same-named tabs share a
/// single mark. Rescue-tab names are distinct emoji-prefixed slugs, so neither
/// bites in practice.
pub fn parse_rescue_mark(payload: &str) -> Option<(String, MarkKind)> {
    let (tab, state) = payload.trim().rsplit_once(':')?;
    let tab = tab.trim();
    if tab.is_empty() {
        return None;
    }
    let kind = match state.trim().to_ascii_lowercase().as_str() {
        "ok" | "done" | "ready" => MarkKind::Ok,
        "wip" | "pending" | "run" | "running" => MarkKind::Wip,
        "fail" | "err" | "error" => MarkKind::Fail,
        "clear" | "none" => MarkKind::Clear,
        _ => return None,
    };
    Some((tab.to_string(), kind))
}

// Host-only tests: proptest is a host-gated dev-dependency (no wasi support
// in its process-spawning machinery).
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use unicode_width::UnicodeWidthStr;

    proptest! {
        // trunc never panics and never exceeds the column budget.
        #[test]
        fn trunc_respects_width(s in "\\PC*", max in 0usize..64) {
            let out = trunc(&s, max);
            prop_assert!(UnicodeWidthStr::width(out.as_str()) <= max);
        }

        // Wide input always gets the ellipsis marker; narrow input is unchanged.
        #[test]
        fn trunc_is_faithful(s in "\\PC*", max in 1usize..64) {
            let out = trunc(&s, max);
            if UnicodeWidthStr::width(s.as_str()) <= max {
                prop_assert_eq!(out, s);
            } else {
                // prop_assert! treats braces in stringified exprs as format
                // args, so keep the ellipsis literal out of the macro body.
                let ellipsis = '\u{2026}';
                let cut = out.ends_with(ellipsis);
                prop_assert!(cut);
                let prefix = &out[..out.len() - ellipsis.len_utf8()];
                let is_prefix = s.starts_with(prefix);
                prop_assert!(is_prefix);
            }
        }

        // The viewport never runs past the end and always contains the active
        // tab (when there is one to contain).
        #[test]
        fn scroll_window_in_bounds(n in 0usize..500, area in 1usize..100, active_seed in 0usize..500) {
            let active = if n == 0 { 0 } else { active_seed % n };
            let start = scroll_window(n, area, active);
            if n <= area {
                prop_assert_eq!(start, 0);
            } else {
                prop_assert!(start + area <= n);
                prop_assert!(start <= active);
                prop_assert!(active < start + area);
            }
        }

        // parse_rescue_mark never panics on arbitrary input (fuzz-grade, but
        // cheap enough to also assert here).
        #[test]
        fn parse_mark_never_panics(s in "\\PC*") {
            let _ = parse_rescue_mark(&s);
        }

        // A `<tab>:<known-state>` payload always parses and preserves the
        // (trimmed) tab name — colons only ever split off the trailing state.
        #[test]
        fn parse_mark_preserves_tab(
            tab in "[^:[:space:]][^:]{0,30}",
            st in prop::sample::select(vec!["ok", "wip", "fail", "clear", "done", "err", "pending"]),
        ) {
            let (got, _) = parse_rescue_mark(&format!("{tab}:{st}")).unwrap();
            prop_assert_eq!(got, tab.trim_end());
        }
    }

    #[test]
    fn parse_mark_states_and_edges() {
        // known states
        assert_eq!(
            parse_rescue_mark("build:ok"),
            Some(("build".to_string(), MarkKind::Ok))
        );
        // emoji tab name + whitespace + case-insensitive state (the real shape:
        // `zellij pipe --name rescue-mark -- '🔨 Rescue:wip'`)
        assert_eq!(
            parse_rescue_mark("  \u{1f528} Rescue : WIP "),
            Some(("\u{1f528} Rescue".to_string(), MarkKind::Wip))
        );
        // last-colon split keeps colons inside the tab name
        assert_eq!(
            parse_rescue_mark("a:b:fail"),
            Some(("a:b".to_string(), MarkKind::Fail))
        );
        // wildcard clear
        assert_eq!(
            parse_rescue_mark("*:clear"),
            Some(("*".to_string(), MarkKind::Clear))
        );
        // garbage / malformed → inert
        assert_eq!(parse_rescue_mark("t:bogus"), None);
        assert_eq!(parse_rescue_mark("nocolon"), None);
        assert_eq!(parse_rescue_mark(":ok"), None);
        assert_eq!(parse_rescue_mark(""), None);
    }
}
