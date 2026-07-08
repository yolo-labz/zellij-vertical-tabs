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

Three small files: `src/lib.rs` (pure rendering core — host-buildable),
`src/plugin.rs` (the plugin, wasm-only), `src/main.rs` (entrypoint glue). No
heavy dependencies beyond `zellij-tile`, `unicode-width`, and `chrono`
(+ `chrono-tz`) — all zellij/chrono deps are wasm-target-gated so the host
build stays tiny. Every added widget must justify its event subscription
against the stability rule above.

## Rescue instrument

The visible instance mirrors the live tab set to `/data/tab-manifest.txt`
(atomic tmp+rename; TabUpdate-driven + ~1/min freshness touch) and the footer
border carries a `version@rev` badge (`PLUGIN_REV` baked by the flake). Born
from the 04/07 double-kill restore: manifests written only from live sessions
cannot be poisoned by dead-session dumps, and mixed-wasm fleets (resurrected
tabs pin old store paths) become visible at a glance. Write failures are
swallowed — a rescue aid must never break rendering. Full rationale:
Brain Storm `Ideas/zellij-vtabs-as-rescue-instrument.md` (P1 + P3).

**Rescue marks (P2).** A restore script can paint a per-tab status glyph on the
sidebar so an operator watches convergence in place instead of polling `ps`:

```sh
zellij pipe --name rescue-mark -- '🔨 Rescue:wip'    # ⏳ in progress
zellij pipe --name rescue-mark -- '🔨 Rescue:ok'     # ✅ converged
zellij pipe --name rescue-mark -- '💳 Buy:fail'      # ❌ failed
zellij pipe --name rescue-mark -- '🔨 Rescue:clear'  # remove one mark
zellij pipe --name rescue-mark -- '*:clear'          # remove every mark
```

Payload is `<tab-name>:<state>` (split on the **last** colon, so names may
contain colons; state is case-insensitive with aliases — `ok|done|ready`,
`wip|pending|run|running`, `fail|err|error`, `clear|none`). The glyph renders
in the existing bell/sync flair slot (width-2, self-coloured — no ANSI, no
active-band reset). `pipe()` is a distinct trait method, **not** a subscribed
event, so it is operator-driven and low-frequency — invariant 1 holds. The mark
map is bounded three ways so a broadcast pipe can never grow the 16 MB guest:
only names of **live** tabs are stored (map ≤ tab count), a hard `MAX_MARKS`
backstop caps new keys, and each mark self-expires after `MARK_TTL_MS`
(15 min), swept in both `render()` and `pipe()` (hidden instances never render,
so the pipe path is their only sweep). Parsing lives in the pure core
(`parse_rescue_mark`) and is proptested + fuzzed like `trunc`. See
`examples/rescue-marks.sh` for a worked emit loop.

## Tested invariants

The two functions that carry invariants 2 and 3 (`trunc`, `scroll_window`)
live in the pure core and are **property-tested** (`cargo test`, proptest:
width budget, char-boundary prefix, viewport bounds/containment) and
**fuzzed** (`cargo fuzz run render_core`; a 60 s smoke runs on every PR,
see ci.yml). A renderer change that byte-slices or un-clamps the viewport
fails CI before it can trap the guest.

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
