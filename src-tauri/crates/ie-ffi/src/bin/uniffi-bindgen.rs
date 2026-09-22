//! The binding generator, as a binary of this crate.
//!
//! UniFFI's generator has to be the same version as the runtime it generates
//! for, and building it here is what guarantees that: `cargo run --bin
//! uniffi-bindgen` can only ever use the `uniffi` in this crate's lockfile.
//!
//! It is a pure-Rust program that emits Swift as text, so it runs on any host —
//! which is how the bindings stay reviewable without a Mac.
fn main() {
    uniffi::uniffi_bindgen_main()
}
