// Fuzz the pure rendering core against the load-bearing invariants:
// trunc must never panic, never split a char, never exceed the column
// budget (invariant 2 — hostile tab names crash the whole guest otherwise);
// scroll_window must never index out of bounds and must keep the active
// tab inside the viewport (invariant 3); parse_rescue_mark must never panic
// on a hostile pipe payload (P2 — it parses operator-supplied bytes).
#![no_main]
use libfuzzer_sys::fuzz_target;
use unicode_width::UnicodeWidthStr;
use zellij_vertical_tabs::{parse_rescue_mark, scroll_window, trunc};

fuzz_target!(|input: (&str, u16, u16, u16, u16)| {
    let (s, max, n, area, active_seed) = input;
    let max = max as usize % 512;

    // parse_rescue_mark runs on arbitrary operator input; it must only ever
    // return None or a value — never panic (no byte-slicing at char boundaries).
    let _ = parse_rescue_mark(s);

    let out = trunc(s, max);
    assert!(
        UnicodeWidthStr::width(out.as_str()) <= max,
        "trunc exceeded the column budget"
    );
    if !out.ends_with('\u{2026}') {
        assert!(s.starts_with(&out), "trunc emitted a non-prefix without cut");
    }

    let (n, area) = (n as usize, (area as usize).max(1));
    let active = if n == 0 { 0 } else { active_seed as usize % n };
    let start = scroll_window(n, area, active);
    if n <= area {
        assert_eq!(start, 0);
    } else {
        assert!(start + area <= n, "viewport past the end");
        assert!(
            start <= active && active < start + area,
            "active tab outside the viewport"
        );
    }
});
