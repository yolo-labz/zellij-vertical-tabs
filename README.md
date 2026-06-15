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
- **Fuzzy go-to picker** (`mode "picker"`): a focusable floating surface that
  fuzzy-filters tab names; on no match, `⏎` creates a new tab tagged with an
  emoji not already in use

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

### Fuzzy picker

Bind a key to launch the *same* wasm in picker mode as a floating pane:

```kdl
bind "Alt g" {
    LaunchOrFocusPlugin "file:/path/to/zellij-vertical-tabs.wasm" {
        floating true
        mode "picker"
    }
}
```

In the picker: type to fuzzy-filter, `↑/↓` or `Ctrl+n`/`Ctrl+p` to move, `⏎` to
go to the selected tab. If the query matches no tab, `⏎` creates a new tab named
`<emoji> <query>` (emoji picked from a curated pool, avoiding ones already on a
tab). `Esc` cancels.

## License

MIT © Pedro H S Balbino
