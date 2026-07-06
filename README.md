# zellij-vertical-tabs

A robust, good-looking **vertical tab sidebar** for [zellij](https://zellij.dev).

Horizontal tab bars truncate badly once you have many tabs. This plugin renders
the tab list vertically down the side of the screen, with a framed header
(session · date · time · mode), per-tab bell/sync indicators, an active-tab
highlight, and a scrolling viewport that always keeps the focused tab visible.
Themed in Catppuccin Mocha.

> Built because `cfal/zellij-vertical-tabs` crashes under heavy real load
> (subscribing to `PaneUpdate` exhausts the wasm guest's 16 MB memory cap when
> many TUIs redraw). This implementation subscribes only to the events a tab bar
> actually needs. See `docs/design-principles.md` for the load-bearing invariants.

## Features

- Vertical tab list with index, emoji, name (char/width-safe truncation)
- Framed panel: 🏠 session · 📅 date · 🕐 live clock · 🔒 mode · footer tab count
- Active tab: mauve bar + bold + highlight band
- Scrolling viewport with `↑/↓` overflow counts — focused tab always visible
- Mouse: click to switch, scroll wheel to cycle tabs
- Per-tab `🔔` bell + `🔗` sync-panes indicators
- Version badge (`0.1.0@rev`) in the footer border — spot stale/mixed plugin
  builds across resurrected tabs at a glance
- **Live tab manifest** for session-rescue tooling (see below)

## Tab manifest (rescue instrument)

The visible instance mirrors the tab set to `/data/tab-manifest.txt` on every
tab change, plus a ~1/min freshness touch. Because it is written only from a
*live* session, a stale mtime reliably means "session gone" — it cannot be
poisoned by dumps against dead sessions the way `dump-layout`-based
checkpoints can.

Format (tab-separated; `*` marks the active tab):

```text
# zellij-vertical-tabs tab manifest v1
plugin: 0.1.0@abc1234
session: HOME
written_ms: 1780000000000
tabs: 35
1	-	🔨 Rescue
2	*	🧙 Merlin
```

Host-side discovery (the `/data` mount is per session + plugin instance):

```bash
grep -l '^session: HOME$' \
  ~/.cache/zellij/*/*zellij-vertical-tabs*/*/tab-manifest.txt 2>/dev/null \
  | xargs ls -t | head -1   # newest = the live writer
```

## Build

```bash
nix build      # -> result/lib/zellij-vertical-tabs.wasm
# or
cargo build --release --target wasm32-wasip1
```

## Use

```kdl
pane size=24 borderless=true {
    plugin location="file:/path/to/zellij-vertical-tabs.wasm" {
        timezone "America/Recife"
    }
}
```

Put it in your layout's `default_tab_template` next to `children` (children
first → sidebar on the right). Grant `ReadApplicationState` +
`ChangeApplicationState`.

## License

MIT © Pedro H S Balbino
