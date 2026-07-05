# Security Policy

A published zellij plugin: the built `.wasm` runs inside every user's zellij
session, in each tab, as a `wasmi` guest. The threat model is proportionate to
that surface.

## Threat model

| # | Threat | Mitigation |
|---|---|---|
| T1 | Malicious/hostile input via tab names (emoji, control bytes, width tricks) crashing the renderer — a guest panic aborts the whole plugin | Char/width-safe rendering only (`unicode-width`, no byte slicing); all index/scroll arithmetic saturating + clamped. See `docs/design-principles.md`. |
| T2 | Event-flood memory exhaustion (16 MB wasmi cap) | Low-frequency event subscriptions only — never `PaneUpdate`; visibility-gated timer. Load-bearing invariant, enforced at review. |
| T3 | Supply-chain compromise of a dependency | Locked builds (`--locked`, `Cargo.lock` committed); OSV-Scanner on PR + weekly; Renovate with 3-day `minimumReleaseAge`; reproducible `nix build`. |
| T4 | Compromised CI publishing a tampered artifact | Actions SHA-pinned with version comments; `permissions: {}` deny-all + per-job re-grant; harden-runner; gitleaks; OpenSSF Scorecard published. |
| T5 | Excessive plugin permissions | Requests only `ReadApplicationState` + `ChangeApplicationState` — no filesystem, no command execution, no web access. |

## Reporting a vulnerability

Open a [GitHub security advisory](https://github.com/yolo-labz/zellij-vertical-tabs/security/advisories/new)
(private) or email `pedrobalbino@proton.me`. Expect an acknowledgement within
7 days. Please include the zellij version, the plugin rev, and a reproduction
(a layout + tab set is usually enough).

## Scope

The only supported artifact is the `.wasm` built from this repo's `main` via
`nix build` / `cargo build --release --target wasm32-wasip1`. Forks and
locally-patched builds are out of scope.
