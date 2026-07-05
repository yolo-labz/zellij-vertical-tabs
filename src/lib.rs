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

#[cfg(test)]
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
    }
}
