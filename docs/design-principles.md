# Design principles

The non-obvious constraints behind this plugin — the parts the code alone does
not explain.

## Stability over features

The plugin runs as a `wasmi` guest with a hard 16 MB linear-memory cap and a
single-threaded mutex that aborts the whole guest on a panic-during-panic. Any
feature that risks a guest trap is rejected.

- **Never subscribe to `EventType::PaneUpdate`** (or other high-frequency
  events: `InputReceived`, `PaneRenderReport`). They flood the guest and crash
  it under load. A tab bar only needs `TabUpdate`, `ModeUpdate`, `Mouse`,
  `Timer`, `Visible`, `PermissionRequestResult`.
- **Rendering is char/width-safe.** No byte-slicing; truncate via `char`
  iteration + `unicode-width`. Emoji must never panic the renderer.
- **All index/scroll arithmetic uses `saturating_*` + `.min()` clamps.** No
  unchecked indexing or subtraction.
- **One instance per tab → gate the tick on visibility.** The layout's
  `default_tab_template` instantiates this plugin in every tab; N tabs = N wasm
  instances. `Event::Visible` (zellij sends it to tiled plugin panes on tab
  focus transitions) parks the 1 Hz clock in hidden instances — steady-state is
  ~1 armed timer per session instead of N. The tick chain also **self-heals**
  (arm-timestamp dead-chain detection on other events, duplicate chains
  collapsed by a sub-second guard): a single dropped Timer event previously
  froze that instance's clock forever, visible as sidebars disagreeing across
  tabs.
- **Per-tab is NOT a choice — zellij has no cross-tab pane (researched
  04/07/2026, don't re-litigate).** Panes are owned by a single tab
  (`Screen → Tab → Pane`); the built-in tab-bar/status-bar and zjstatus all run
  one instance per tab; background plugins (`load_plugins`) cannot render a
  pane; pinned floating panes are per-tab too. Only the wasm *bytecode* is
  cached across instances — each pane gets its own linear memory. The
  author-once/appears-everywhere UI is an unshipped upstream proposal (the
  `viewport` KDL node, [zellij#4646](https://github.com/zellij-org/zellij/issues/4646));
  revisit this architecture if/when that lands. Until then, the design above
  (cheap parked instances) is the correct shape.

## Minimal surface

Single file (`src/main.rs`), no heavy dependencies beyond `zellij-tile`,
`unicode-width`, and `chrono` (+ `chrono-tz`). Every added widget must justify
its event subscription against the stability rule above.

## Reproducible builds

`nix build` produces the wasm reproducibly (fenix toolchain + pinned target).
`zellij-tile` is pinned to the target runtime's zellij version. No vendored
binaries in git; `target/` and `*.wasm` are gitignored.

## Theme

Catppuccin Mocha palette via explicit 24-bit ANSI constants. Visual hierarchy
comes from weight and colour — no rainbow gradients, no decorative noise. The
active tab is unambiguous.

## Release engineering

Conventional commits + DCO sign-off. Signed releases, SBOM, and build-provenance
attestations per the yolo-labz standard. Never re-tag a release; cut a new patch
on a botched publish. `zellij-tile` version bumps are tested against the matching
zellij runtime before release.
