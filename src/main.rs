// Plugin entrypoint. The plugin proper (src/plugin.rs) and the zellij glue
// only exist for wasm32 — on the host this crate compiles down to the pure
// core in src/lib.rs plus this stub, so `cargo test` (proptest) and
// cargo-fuzz run natively without pulling zellij-utils' host-only deps.
#[cfg(target_arch = "wasm32")]
use zellij_tile::prelude::*;

#[cfg(target_arch = "wasm32")]
mod plugin;

#[cfg(target_arch = "wasm32")]
register_plugin!(plugin::State);

// register_plugin! generates fn main() for the wasm bin; this one is the
// host stand-in so the bin target still links for `cargo test`.
#[cfg(not(target_arch = "wasm32"))]
fn main() {}
