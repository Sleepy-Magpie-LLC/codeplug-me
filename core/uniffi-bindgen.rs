//! uniffi-bindgen CLI entry point (library mode).
//!
//! Built only with `--features cli`; `scripts/gen-bindings.sh` runs it against
//! the compiled cdylib to emit the Kotlin bindings the Android app uses. The
//! macOS app does not go through UniFFI at all: it keeps the hand-written C ABI
//! in `src/ffi.rs` and `include/codeplugme_core.h`.

/// Delegates straight to uniffi's bundled bindgen CLI.
fn main() {
    uniffi::uniffi_bindgen_main()
}
