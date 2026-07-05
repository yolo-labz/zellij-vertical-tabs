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

## Accepted advisories (transitive, unreachable — revisit on zellij-tile bump)

These RustSec advisories sit in the lockfile via `zellij-tile 0.44.3 →
zellij-utils → clap 3`, which is pinned to the runtime zellij version by
design (see `renovate.json`); they are not fixable here without unpinning:

| Advisory | Crate | Why accepted |
|---|---|---|
| RUSTSEC-2024-0375 | `atty` (unmaintained) | clap-3 CLI machinery; a zellij wasm plugin never runs argument parsing, so the code path is dead in the shipped artifact |
| RUSTSEC-2021-0145 | `atty` (unaligned read) | Windows-only unsound path, doubly unreachable under `wasm32-wasip1` |
| RUSTSEC-2024-0370 | `proc-macro-error` (unmaintained) | proc-macro — compile-time only, never part of the artifact |

Trigger to revisit: any `zellij-tile` version bump (upstream zellij moved to
clap 4 after 0.44, which drops `atty`/`proc-macro-error`).
