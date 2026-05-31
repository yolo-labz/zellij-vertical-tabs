# zellij-vertical-tabs Constitution

Project principles for spec-driven development. Non-negotiable unless amended here.

## I. Stability over features (LOAD-BEARING)

The plugin runs as a `wasmi` guest with a hard 16 MB linear-memory cap and a
single-threaded mutex that aborts the whole guest on a panic-during-panic. Any
feature that risks a guest trap is rejected. Concretely:

- **Never subscribe to `EventType::PaneUpdate`** (or any high-frequency event:
  `InputReceived`, `PaneRenderReport`). They flood the guest and crash it under
  load. A tab bar only needs `TabUpdate`, `ModeUpdate`, `Mouse`, `Timer`,
  `PermissionRequestResult`.
- **Rendering is char/width-safe.** No byte-slicing; truncate via `char`
  iteration + `unicode-width`. Emoji must never panic the renderer.
- **All index/scroll arithmetic uses `saturating_*` + `.min()` clamps.** No
  unchecked indexing or subtraction.

## II. Minimal surface

Single-file (`src/main.rs`), no heavy dependencies beyond `zellij-tile`,
`unicode-width`, `chrono`(+`chrono-tz`). Every added widget must justify its
event subscription against Principle I.

## III. Reproducible builds

`nix build` produces the wasm reproducibly (fenix toolchain + pinned target).
`zellij-tile` is pinned to the target runtime's zellij version. No vendored
binaries in git; `target/` and `*.wasm` are gitignored.

## IV. Theme-correct, not slop

Catppuccin Mocha palette via explicit 24-bit ANSI constants. Clear visual
hierarchy (weight/color), no rainbow gradients, no decorative noise. Active
state is unambiguous.

## V. Release engineering (yolo-labz standard)

Conventional commits + DCO sign-off. Signed releases, SBOM, and build
provenance attestations per the org standard. Never re-tag a release; cut a new
patch on a botched publish. `zellij-tile` version bumps must be tested against
the matching zellij runtime before release.

## Governance

Amendments require a note here + a version bump. Specs live under
`.specify/specs/NNN-slug/`. The `CLAUDE.md` invariants mirror Principle I and
must stay in lockstep.

**Version:** 1.0.0 · **Ratified:** 2026-05-31
