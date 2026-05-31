# zellij-vertical-tabs

A robust vertical tab-bar **sidebar** plugin for [zellij](https://zellij.dev),
styled in Catppuccin Mocha. Renders the tab list down the side of the screen
(instead of the truncation-prone horizontal bar) with a framed header showing
session name, live clock/date, and input mode.

Self-coded (yolo-labz) because the existing `cfal/zellij-vertical-tabs` **crashes
under real load** — see the invariants below; they are the whole reason this
plugin exists. Do not "simplify" them away.

## Build

```bash
nix build                  # -> result/lib/zellij-vertical-tabs.wasm  (reproducible)
# or, in the dev shell / with rustup:
cargo build --release --target wasm32-wasip1
```

nixpkgs `rustc` does **not** ship the `wasm32-wasip1` std — the flake uses
`fenix` to supply the target. With rustup: `rustup target add wasm32-wasip1`.

## Use (zellij layout)

```kdl
pane size=24 borderless=true {
    plugin location="file:/abs/path/to/zellij-vertical-tabs.wasm" {
        timezone "America/Recife"   // optional; else $TZ; else UTC
    }
}
```

The plugin needs `ReadApplicationState` + `ChangeApplicationState`. For a
`file:`-loaded plugin grant them in `~/.cache/zellij/permissions.kdl` (keyed by
the wasm path) or accept the in-pane permission prompt on first run.

## LOAD-BEARING INVARIANTS (violating any one re-introduces a crash)

1. **NEVER subscribe to `EventType::PaneUpdate`.** zellij delivers a full
   `PaneManifest` into the wasm guest on *every* pane redraw. With many
   constantly-redrawing TUIs (e.g. 25 `claude` tabs) the wasmi interpreter's
   hard **16 MB** linear-memory cap is exhausted → `memory.grow` trap →
   `unreachable` → poisoned mutex → `no_threads.rs:19` abort ("Failed to apply
   event to plugin"). Returning `false` from `update()` does **not** help — the
   guest call runs on delivery regardless of the return value. The built-in
   `zellij:tab-bar` avoids PaneUpdate for exactly this reason. We subscribe to
   `TabUpdate + ModeUpdate + Mouse + Timer + PermissionRequestResult` only.
2. **Char/width-safe rendering only.** Never byte-slice strings (tab names
   contain emoji); truncate by iterating `char`s with `unicode-width`. Byte
   indexing at a non-char boundary panics → wasm OOB.
3. **No unbounded viewport arithmetic.** Scrolling uses `saturating_sub` +
   `.min()` clamps only. cfal's overflow/viewport math did an OOB under load.
4. **Clock:** `chrono::Local::now()` returns **UTC** under `wasm32-wasip1`
   (the local-time path is `#[cfg(unix)]`, wasi is excluded). Always
   `chrono::Utc::now().with_timezone(&tz)` with a `chrono_tz::Tz` (config
   `timezone`, else `$TZ` — inherited via zellij's `inherit_env` — else UTC).
5. **1 Hz clock tick = `set_timeout(1.0)` + re-arm on `Event::Timer`.** This is
   the *only* safe periodic re-render (low frequency). Do not poll high-freq
   events to drive redraws.

## Layout

- `src/main.rs` — the whole plugin (single file, ~200 lines).
- `Cargo.toml` — deps: `zellij-tile` (pin to the runtime zellij version, e.g.
  0.44.3), `unicode-width`, `chrono` (`default-features=false, features=["clock"]`),
  `chrono-tz`.
- `flake.nix` — fenix toolchain + reproducible wasm build + dev shell.
- `examples/` — sample layouts (sidebar left / right).
- `.specify/` — spec-kit (constitution + specs) for feature work.

## Conventions

- Catppuccin Mocha colors as 24-bit ANSI consts (no raw rainbow). Active tab =
  mauve `▌` bar + bold + surface0 band. Divider rule = overlay0.
- Commit style: conventional commits, `git commit -s` (DCO). MIT licensed.
- Release engineering follows the yolo-labz standard (signed releases, SBOM,
  attestations) — see the org's release-engineering rule.
