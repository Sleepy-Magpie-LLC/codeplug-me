//! C ABI for the macOS SwiftUI app.
//!
//! Every entry point carries the `codeplugme_` prefix. They are model-agnostic
//! and take or report a [`Model`] id where the model matters.
//!
//! Conventions (mirrored by `core/include/codeplugme_core.h`):
//! - Strings cross the boundary as UTF-8 C strings. Every `char *` returned by
//!   this module is heap-allocated and must be released with
//!   [`codeplugme_string_free`]. Never free them any other way.
//! - Fallible functions either return a string (null on failure) or an `i32`
//!   status (`0` = success, `-1` = failure). On failure `err_out`, when
//!   non-null, receives a heap-allocated error message the caller must free
//!   with [`codeplugme_string_free`].
//! - No panic ever crosses the boundary: every entry point runs its body under
//!   `catch_unwind` and converts a panic into an error status.
//!
//! The device functions keep the same safety gates as the CLI: `restore`
//! requires an explicit `force` flag, refuses to write unless the connected
//! radio identifies as the model the FILE is for, and the core verifies every
//! written block by reading it back.

use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use serde::Serialize;

use crate::anytone::device::codeplug_size;
use crate::document::Document;
use crate::driver::{driver_for, write_verified_image};
use crate::model::{Capabilities, Model};
use crate::ports::list_ports;
use crate::transport::{SerialTransport, Transport};

/// Per-read timeout for the serial link.
const TIMEOUT: Duration = Duration::from_secs(2);

/// C progress callback: `(blocks_done, blocks_total, user_context)`. Called
/// synchronously on the thread that invoked the FFI function.
pub type CodeplugmeProgress = extern "C" fn(done: usize, total: usize, user: *mut c_void);

/// One serial port, serialized to JSON for [`codeplugme_ports_json`].
#[derive(Serialize)]
struct PortJson {
    name: String,
    vid: Option<u16>,
    pid: Option<u16>,
    product: Option<String>,
    likely_radio: bool,
    /// The model this port's USB identity names, or null when unrecognized.
    model: Option<&'static str>,
}

/// One supported radio, serialized to JSON for [`codeplugme_models_json`].
#[derive(Serialize)]
struct ModelJson {
    id: &'static str,
    display_name: &'static str,
    image_size: usize,
    capabilities: Capabilities,
}

/// The identity of a connected radio, serialized for [`codeplugme_identify_json`].
#[derive(Serialize)]
struct IdentityJson {
    model: &'static str,
    identity: String,
}

/// Convert a Rust string into a heap-allocated C string the caller frees with
/// [`codeplugme_string_free`]. Interior NULs (impossible in practice) are replaced
/// so this can never fail.
fn into_c_string(s: String) -> *mut c_char {
    let sanitized: String = s.chars().map(|c| if c == '\0' { ' ' } else { c }).collect();
    CString::new(sanitized)
        .expect("sanitized string has no interior NUL")
        .into_raw()
}

/// Store `msg` into `err_out` (when non-null) as a heap-allocated C string.
///
/// Safety: `err_out` must be null or a valid pointer to a `char *` slot.
unsafe fn set_err(err_out: *mut *mut c_char, msg: &str) {
    if !err_out.is_null() {
        *err_out = into_c_string(msg.to_string());
    }
}

/// Borrow a required UTF-8 C string argument, with a clear error on null or
/// invalid UTF-8.
///
/// Safety: `p` must be null or a valid NUL-terminated C string.
unsafe fn arg_str<'a>(p: *const c_char, name: &str) -> Result<&'a str, String> {
    if p.is_null() {
        return Err(format!("{name} must not be null"));
    }
    CStr::from_ptr(p)
        .to_str()
        .map_err(|_| format!("{name} is not valid UTF-8"))
}

/// Run a string-producing body under `catch_unwind`; on error set `err_out`
/// and return null.
///
/// Safety: `err_out` must be null or a valid pointer to a `char *` slot.
unsafe fn ffi_string(
    err_out: *mut *mut c_char,
    body: impl FnOnce() -> Result<String, String>,
) -> *mut c_char {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(s)) => into_c_string(s),
        Ok(Err(e)) => {
            set_err(err_out, &e);
            std::ptr::null_mut()
        }
        Err(_) => {
            set_err(err_out, "internal error: panic in codeplugme-core");
            std::ptr::null_mut()
        }
    }
}

/// Run a status-producing body under `catch_unwind`; on error set `err_out`
/// and return -1, else 0.
///
/// Safety: `err_out` must be null or a valid pointer to a `char *` slot.
unsafe fn ffi_status(err_out: *mut *mut c_char, body: impl FnOnce() -> Result<(), String>) -> i32 {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            set_err(err_out, &e);
            -1
        }
        Err(_) => {
            set_err(err_out, "internal error: panic in codeplugme-core");
            -1
        }
    }
}

/// Open the serial port as a bare transport for a [`RadioDriver`] to drive.
///
/// The line rate comes from the model, because it is a property of the radio.
/// See [`Model::baud`] for why that matters more than it looks like it should.
fn open_transport(port: &str, model: Model) -> Result<SerialTransport, String> {
    SerialTransport::open(port, model.baud(), TIMEOUT)
        .map_err(|e| format!("failed to open {port}: {e}"))
}

/// Resolve a model id argument that may be null.
///
/// A null `model` means "infer from the port's USB identity". Inference can
/// fail: not every supported radio's USB bridge is in
/// [`crate::ports::KNOWN_USB_IDS`], and a generic USB-serial adapter names no
/// model at all. With more than one supported model there is no safe default to
/// fall back to, so that case is an error naming the models the caller may pass
/// instead. Speaking the wrong protocol at a radio is exactly what this refuses
/// to do by accident.
///
/// Safety: `model` must be null or a valid NUL-terminated C string.
unsafe fn resolve_model(model: *const c_char, port: &str) -> Result<Model, String> {
    if !model.is_null() {
        let id = arg_str(model, "model")?;
        return Model::from_id(id).ok_or_else(|| format!("unknown radio model {id:?}"));
    }
    model_for_port(port).ok_or_else(|| {
        let ids: Vec<&str> = Model::ALL.iter().map(|m| m.id()).collect();
        format!(
            "cannot tell which radio is on {port}: its USB identity matches no \
             known radio. Choose the model explicitly (one of: {}).",
            ids.join(", ")
        )
    })
}

/// The model named by `port`'s USB identity, if the OS reports the port at all
/// and its VID/PID is one we know.
fn model_for_port(port: &str) -> Option<Model> {
    let ports = list_ports().ok()?;
    let info = ports.into_iter().find(|p| p.name == port)?;
    info.model
}

/// Build an `FnMut(done, total)` progress closure that forwards to the
/// optional C callback with its user context pointer.
fn progress_closure(
    progress: Option<CodeplugmeProgress>,
    user: *mut c_void,
) -> impl FnMut(usize, usize) {
    move |done, total| {
        if let Some(cb) = progress {
            cb(done, total, user);
        }
    }
}

/// Free a string previously returned by any function in this module. Passing
/// null is a no-op.
///
/// # Safety
/// `s` must be null or a pointer previously returned by this module and not
/// yet freed.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// The exact byte length of a full codeplug image for the supported radio
/// (the sum of every modeled region). A `.bin` of any other length is from a
/// different model or firmware version and cannot be parsed or written by this
/// build — the app uses this to reject an incompatible file up front with a
/// clear message rather than a low-level size error.
#[no_mangle]
pub extern "C" fn codeplugme_codeplug_size() -> usize {
    codeplug_size()
}

/// Every radio this build supports, as a JSON array:
/// `[{"id","display_name","image_size","capabilities":{...}}, ...]`.
/// Returns null and sets `err_out` on failure.
///
/// # Safety
/// `err_out` must be null or a valid pointer to a `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_models_json(err_out: *mut *mut c_char) -> *mut c_char {
    ffi_string(err_out, || {
        let json: Vec<ModelJson> = Model::ALL
            .iter()
            .map(|m| ModelJson {
                id: m.id(),
                display_name: m.display_name(),
                image_size: m.image_size(),
                capabilities: m.capabilities(),
            })
            .collect();
        serde_json::to_string(&json).map_err(|e| format!("failed to encode models JSON: {e}"))
    })
}

/// Identify which radio the image file `bin_path` belongs to, returning its
/// model id. Returns null and sets `err_out` when the file cannot be read or
/// matches no supported radio.
///
/// # Safety
/// `bin_path` must be a valid C string; `err_out` null or a valid `char *`
/// slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_detect_model(
    bin_path: *const c_char,
    err_out: *mut *mut c_char,
) -> *mut c_char {
    ffi_string(err_out, || {
        let bin_path = arg_str(bin_path, "bin_path")?;
        let data = fs::read(bin_path).map_err(|e| format!("failed to read {bin_path}: {e}"))?;
        Model::detect(&data)
            .map(|m| m.id().to_string())
            .ok_or_else(|| {
                format!(
                    "{bin_path} is {} bytes, which matches no supported radio image",
                    data.len()
                )
            })
    })
}

/// The exact byte length of a full codeplug image for `model`. Returns 0 and
/// sets `err_out` when the model id is unknown.
///
/// # Safety
/// `model` must be a valid C string; `err_out` null or a valid `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_codeplug_size_for(
    model: *const c_char,
    err_out: *mut *mut c_char,
) -> usize {
    let body = || -> Result<usize, String> {
        let id = arg_str(model, "model")?;
        Model::from_id(id)
            .map(Model::image_size)
            .ok_or_else(|| format!("unknown radio model {id:?}"))
    };
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            set_err(err_out, &e);
            0
        }
        Err(_) => {
            set_err(err_out, "internal error: panic in codeplugme-core");
            0
        }
    }
}

/// Enumerate serial ports as a JSON array:
/// `[{"name","vid","pid","product","likely_radio","model"}, ...]`, where
/// `model` is a model id or null and `likely_radio` is simply `model != null`.
/// Returns null and sets `err_out` on failure.
///
/// # Safety
/// `err_out` must be null or a valid pointer to a `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_ports_json(err_out: *mut *mut c_char) -> *mut c_char {
    ffi_string(err_out, || {
        let ports = list_ports().map_err(|e| e.to_string())?;
        let json: Vec<PortJson> = ports
            .into_iter()
            .map(|p| PortJson {
                name: p.name,
                vid: p.vid,
                pid: p.pid,
                product: p.product,
                likely_radio: p.likely_radio,
                model: p.model.map(Model::id),
            })
            .collect();
        serde_json::to_string(&json).map_err(|e| format!("failed to encode ports JSON: {e}"))
    })
}

/// Handshake with the radio on `port` and return `{"model","identity"}`.
/// `model` names which protocol to speak and may be null to infer it from the
/// port's USB identity. Returns null and sets `err_out` on failure.
///
/// # Safety
/// `port` must be a valid C string; `model` null or a valid C string;
/// `err_out` null or a valid `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_identify_json(
    port: *const c_char,
    model: *const c_char,
    err_out: *mut *mut c_char,
) -> *mut c_char {
    ffi_string(err_out, || {
        let port = arg_str(port, "port")?;
        let model = resolve_model(model, port)?;
        let identity = identify_with(port, model)?;
        serde_json::to_string(&IdentityJson {
            model: model.id(),
            identity,
        })
        .map_err(|e| format!("failed to encode identity JSON: {e}"))
    })
}

/// Handshake with the radio on `port` and return only its identity string.
///
/// Retained unchanged so existing C consumers keep working;
/// [`codeplugme_identify_json`] is the one that also reports the model.
///
/// # Safety
/// `port` must be a valid C string; `err_out` null or a valid `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_identify(
    port: *const c_char,
    err_out: *mut *mut c_char,
) -> *mut c_char {
    ffi_string(err_out, || {
        let port = arg_str(port, "port")?;
        let model = resolve_model(std::ptr::null(), port)?;
        identify_with(port, model)
    })
}

/// Open `port`, run `model`'s identify handshake, and return the identity
/// string the radio reported.
fn identify_with(port: &str, model: Model) -> Result<String, String> {
    let driver = driver_for(model);
    let mut transport = open_transport(port, model)?;
    let mut t: &mut dyn Transport = &mut transport;
    driver.identify(&mut t).map_err(|e| e.to_string())
}

/// Read the full codeplug from the radio on `port` into the file `out_path`.
/// `model` names which protocol to speak and may be null to infer it from the
/// port's USB identity. `progress` (nullable) is called after each block with
/// `(done, total)`.
///
/// # Safety
/// `port`/`out_path` must be valid C strings; `model` null or a valid C string;
/// `err_out` null or a valid `char *` slot; `user` is passed through to
/// `progress` unchanged.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_backup(
    port: *const c_char,
    model: *const c_char,
    out_path: *const c_char,
    progress: Option<CodeplugmeProgress>,
    user: *mut c_void,
    err_out: *mut *mut c_char,
) -> i32 {
    ffi_status(err_out, || {
        let port = arg_str(port, "port")?;
        let out_path = arg_str(out_path, "out_path")?;
        let model = resolve_model(model, port)?;
        let driver = driver_for(model);
        let mut transport = open_transport(port, model)?;
        let mut t: &mut dyn Transport = &mut transport;
        let mut cb = progress_closure(progress, user);
        let data = driver
            .read_image(&mut t, &mut cb)
            .map_err(|e| e.to_string())?;
        fs::write(out_path, &data).map_err(|e| format!("failed to write {out_path}: {e}"))?;
        Ok(())
    })
}

/// Write the codeplug file `in_path` back to the radio on `port`.
///
/// The model comes from the FILE, never from the caller: the file's bytes are
/// what determines which protocol could possibly write them. The connected
/// radio is then handshaked with that model's driver and its identity checked,
/// so pointing a file at the wrong radio fails before a single block is sent.
/// The other gates are unchanged: `force` must be true, and every written block
/// is read back and verified by the core.
///
/// # Safety
/// `port`/`in_path` must be valid C strings; `err_out` null or a valid
/// `char *` slot; `user` is passed through to `progress` unchanged.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_restore(
    port: *const c_char,
    in_path: *const c_char,
    force: bool,
    progress: Option<CodeplugmeProgress>,
    user: *mut c_void,
    err_out: *mut *mut c_char,
) -> i32 {
    ffi_status(err_out, || {
        let port = arg_str(port, "port")?;
        let in_path = arg_str(in_path, "in_path")?;
        let data = fs::read(in_path).map_err(|e| format!("failed to read {in_path}: {e}"))?;
        let model = Model::detect(&data).ok_or_else(|| {
            format!(
                "codeplug file is {} bytes, which matches no supported radio image",
                data.len()
            )
        })?;
        if !force {
            return Err(
                "restore overwrites the radio's configuration; take a fresh backup \
                 and confirm (force) to proceed"
                    .to_string(),
            );
        }
        let mut transport = open_transport(port, model)?;
        let mut t: &mut dyn Transport = &mut transport;
        let mut cb = progress_closure(progress, user);
        // The model came from the file above; the guard confirms the radio on
        // the other end agrees before a single write block goes out.
        write_verified_image(model, &mut t, &data, port, &mut cb)
    })
}

/// Parse the codeplug file `bin_path` offline and return the tagged envelope
/// `{"model","display_name","capabilities",<model key>:{...}}`. Returns null
/// and sets `err_out` on failure.
///
/// # Safety
/// `bin_path` must be a valid C string; `err_out` null or a valid `char *`
/// slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_dump_json(
    bin_path: *const c_char,
    err_out: *mut *mut c_char,
) -> *mut c_char {
    ffi_string(err_out, || {
        let bin_path = arg_str(bin_path, "bin_path")?;
        let data = fs::read(bin_path).map_err(|e| format!("failed to read {bin_path}: {e}"))?;
        let document = Document::parse(&data).map_err(|e| e.to_string())?;
        serde_json::to_string(&document.to_json())
            .map_err(|e| format!("failed to encode codeplug JSON: {e}"))
    })
}

/// Apply a batch of edits to the codeplug file `bin_in` and write the result to
/// `bin_out` (the two paths may be equal). `edits_json` is the tagged envelope
/// (`{"anytone":{"channels":[...],"zones":[...]}}`) or, for backward
/// compatibility, a bare spec with no model key. Every edit round-trips through
/// the typed model and is verified by re-parsing the output, so only the
/// intended bytes change.
///
/// # Safety
/// `bin_in`/`edits_json`/`bin_out` must be valid C strings; `err_out` null or
/// a valid `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_apply_edits(
    bin_in: *const c_char,
    edits_json: *const c_char,
    bin_out: *const c_char,
    err_out: *mut *mut c_char,
) -> i32 {
    ffi_status(err_out, || {
        let bin_in = arg_str(bin_in, "bin_in")?;
        let edits_json = arg_str(edits_json, "edits_json")?;
        let bin_out = arg_str(bin_out, "bin_out")?;
        apply_edits_impl(bin_in, edits_json, bin_out)
    })
}

/// Load `bin_in`, apply the edits described by `edits_json` through
/// [`crate::document::apply_edits`], and write the verified result to
/// `bin_out`.
fn apply_edits_impl(bin_in: &str, edits_json: &str, bin_out: &str) -> Result<(), String> {
    let data = fs::read(bin_in).map_err(|e| format!("failed to read {bin_in}: {e}"))?;
    let out = crate::document::apply_edits(&data, edits_json)?;
    fs::write(bin_out, &out).map_err(|e| format!("failed to write {bin_out}: {e}"))?;
    Ok(())
}

/// Fetch repeaters near a location for the codeplug file `bin_path`, which
/// decides the radio (no DMR for a KT-5000) and which results are flagged
/// `duplicate`. `query_json` is a `geo::GeoQuery`; `cache_dir` holds the free
/// sources' downloads for a day. Returns a JSON array of repeaters, nearest
/// first, or null with `err_out` set. Blocks on the network: call it off the
/// main thread.
///
/// # Safety
/// The string arguments must be valid C strings; `err_out` null or a valid
/// `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_geo_fetch_json(
    query_json: *const c_char,
    bin_path: *const c_char,
    cache_dir: *const c_char,
    err_out: *mut *mut c_char,
) -> *mut c_char {
    ffi_string(err_out, || {
        let query_json = arg_str(query_json, "query_json")?;
        let bin_path = arg_str(bin_path, "bin_path")?;
        let cache_dir = arg_str(cache_dir, "cache_dir")?;
        let data = fs::read(bin_path).map_err(|e| format!("failed to read {bin_path}: {e}"))?;
        crate::geo::fetch_json(query_json, &data, cache_dir)
    })
}

/// How many repeaters may be picked for `bin_path` in `mode` (`"clean"` or
/// `"merge"`), as JSON `{"available","max_selectable","zone_limit",
/// "scan_list_limit","blocked_reason"}`. Null with `err_out` set on failure.
///
/// # Safety
/// The string arguments must be valid C strings; `err_out` null or a valid
/// `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_geo_capacity_json(
    bin_path: *const c_char,
    mode: *const c_char,
    err_out: *mut *mut c_char,
) -> *mut c_char {
    ffi_string(err_out, || {
        let bin_path = arg_str(bin_path, "bin_path")?;
        let mode = arg_str(mode, "mode")?;
        let data = fs::read(bin_path).map_err(|e| format!("failed to read {bin_path}: {e}"))?;
        crate::geo::capacity_json(&data, mode)
    })
}

/// Build a new codeplug at `bin_out` from `bin_in` plus exactly the repeaters
/// in `repeaters_json`. `options_json` is `{"mode":"clean"|"merge",
/// "zone_name"}`. `bin_in` is only read. A selection larger than the radio can
/// take is an error, never truncated.
///
/// # Safety
/// The string arguments must be valid C strings; `err_out` null or a valid
/// `char *` slot.
#[no_mangle]
pub unsafe extern "C" fn codeplugme_geo_build(
    bin_in: *const c_char,
    repeaters_json: *const c_char,
    options_json: *const c_char,
    bin_out: *const c_char,
    err_out: *mut *mut c_char,
) -> i32 {
    ffi_status(err_out, || {
        let bin_in = arg_str(bin_in, "bin_in")?;
        let repeaters_json = arg_str(repeaters_json, "repeaters_json")?;
        let options_json = arg_str(options_json, "options_json")?;
        let bin_out = arg_str(bin_out, "bin_out")?;
        let data = fs::read(bin_in).map_err(|e| format!("failed to read {bin_in}: {e}"))?;
        let out = crate::geo::build_json(&data, repeaters_json, options_json)?;
        fs::write(bin_out, &out).map_err(|e| format!("failed to write {bin_out}: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anytone::codeplug::{
        channel_addr, global_offset, zone_channels_addr, zone_name_addr, Codeplug, CHANNEL_BITMAP,
        CONTACT_BITMAP, ZONE_BITMAP, ZONE_CHANNELS_A, ZONE_CHANNELS_B, ZONE_CHANNELS_SLOT,
        ZONE_CHANNEL_LIST, ZONE_CHANNEL_LIST_SIZE,
    };
    use crate::bytes::{set_bcd8_be, write_ascii};
    use std::path::PathBuf;

    /// Build a minimal synthetic codeplug image: channel 0 (digital, 439 MHz
    /// simplex, CC1, TS2, "CALLING") and zone 0 ("Zone A", member [0]).
    fn synthetic_image() -> Vec<u8> {
        let mut raw = vec![0u8; codeplug_size()];

        // Inverted contact bitmap: all-set means no contacts active.
        let ctbm = global_offset(CONTACT_BITMAP).unwrap();
        for b in raw[ctbm..ctbm + 0x500].iter_mut() {
            *b = 0xff;
        }

        let cbm = global_offset(CHANNEL_BITMAP).unwrap();
        raw[cbm] |= 1;
        let rec = global_offset(channel_addr(0)).unwrap();
        set_bcd8_be(&mut raw[rec..rec + 4], 439_000_000 / 10);
        set_bcd8_be(&mut raw[rec + 4..rec + 8], 0);
        raw[rec + 8] = 0b0000_0001; // digital, simplex
        raw[rec + 0x20] = 1; // CC1
        raw[rec + 0x21] = 1; // TS2
        write_ascii(&mut raw[rec + 0x23..rec + 0x33], "CALLING", 0x00);

        let zbm = global_offset(ZONE_BITMAP).unwrap();
        raw[zbm] |= 1;
        let noff = global_offset(zone_name_addr(0)).unwrap();
        write_ascii(&mut raw[noff..noff + 16], "Zone A", 0x00);
        let coff = global_offset(zone_channels_addr(0)).unwrap();
        for b in raw[coff..coff + ZONE_CHANNELS_SLOT].iter_mut() {
            *b = 0xff;
        }
        raw[coff..coff + 2].copy_from_slice(&0u16.to_le_bytes());

        // Zone channel list: unset everywhere, with zone 0 parked on channel 0
        // for both VFOs — what a real radio holds for this zone layout.
        let zcl = global_offset(ZONE_CHANNEL_LIST).unwrap();
        for b in raw[zcl..zcl + ZONE_CHANNEL_LIST_SIZE].iter_mut() {
            *b = 0xff;
        }
        for vfo in [ZONE_CHANNELS_A, ZONE_CHANNELS_B] {
            raw[zcl + vfo..zcl + vfo + 2].copy_from_slice(&0u16.to_le_bytes());
        }

        raw
    }

    /// Unique temp file path for this test process.
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("anytone-ffi-{}-{name}", std::process::id()))
    }

    /// Convenience: build a CString from a str.
    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    /// Take ownership of an FFI string result, copy it, and free it.
    fn take(p: *mut c_char) -> String {
        assert!(!p.is_null());
        unsafe {
            let s = CStr::from_ptr(p).to_str().unwrap().to_string();
            codeplugme_string_free(p);
            s
        }
    }

    #[test]
    fn codeplug_size_matches_the_modeled_regions() {
        // The FFI must report the same length the parser enforces, or the app's
        // up-front version check would reject valid files (or admit bad ones).
        assert_eq!(codeplugme_codeplug_size(), codeplug_size());
        assert_eq!(codeplugme_codeplug_size(), synthetic_image().len());
    }

    #[test]
    fn ports_json_returns_valid_json_array() {
        let mut err: *mut c_char = std::ptr::null_mut();
        let out = unsafe { codeplugme_ports_json(&mut err) };
        let json = take(out);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.is_array());
    }

    #[test]
    fn dump_json_reads_synthetic_codeplug() {
        let path = temp_path("dump.bin");
        fs::write(&path, synthetic_image()).unwrap();
        let cpath = c(path.to_str().unwrap());

        let mut err: *mut c_char = std::ptr::null_mut();
        let out = unsafe { codeplugme_dump_json(cpath.as_ptr(), &mut err) };
        let json = take(out);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["model"], "anytone-d878uv2");
        let v = &v["anytone"];
        assert_eq!(v["channels"][0]["name"], "CALLING");
        assert_eq!(v["channels"][0]["rx_frequency_hz"], 439_000_000);
        assert_eq!(v["channels"][0]["color_code"], 1);
        assert_eq!(v["channels"][0]["time_slot"], 2);
        assert_eq!(v["zones"][0]["name"], "Zone A");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn dump_json_missing_file_sets_error() {
        let cpath = c("/nonexistent/anytone-test.bin");
        let mut err: *mut c_char = std::ptr::null_mut();
        let out = unsafe { codeplugme_dump_json(cpath.as_ptr(), &mut err) };
        assert!(out.is_null());
        let msg = take(err);
        assert!(msg.contains("failed to read"));
    }

    #[test]
    fn apply_edits_roundtrips_name_rx_tx_and_zone() {
        let input = synthetic_image();
        let pin = temp_path("edit-in.bin");
        let pout = temp_path("edit-out.bin");
        fs::write(&pin, &input).unwrap();

        let edits = r#"{
            "channels": [{"index": 0, "name": "REPEATER",
                          "rx_frequency_hz": 446000000,
                          "tx_frequency_hz": 441000000}],
            "zones": [{"index": 0, "name": "Zone B"}]
        }"#;
        let (cin, cedits, cout) = (
            c(pin.to_str().unwrap()),
            c(edits),
            c(pout.to_str().unwrap()),
        );
        let mut err: *mut c_char = std::ptr::null_mut();
        let status =
            unsafe { codeplugme_apply_edits(cin.as_ptr(), cedits.as_ptr(), cout.as_ptr(), &mut err) };
        assert_eq!(status, 0, "apply_edits failed: {}", take(err));

        let out = fs::read(&pout).unwrap();
        let cp = Codeplug::parse(&out).unwrap();
        let ch = cp.channels().next().unwrap();
        assert_eq!(ch.name, "REPEATER");
        assert_eq!(ch.rx_frequency_hz, 446_000_000);
        assert_eq!(ch.tx_frequency_hz, 441_000_000); // negative 5 MHz offset
        assert_eq!(cp.zones().next().unwrap().name, "Zone B");

        // Only the channel-0 record and zone-0 name slot may differ.
        let rec = global_offset(channel_addr(0)).unwrap();
        let zn = global_offset(zone_name_addr(0)).unwrap();
        for (i, (a, b)) in input.iter().zip(out.iter()).enumerate() {
            if (rec..rec + 0x40).contains(&i) || (zn..zn + 0x20).contains(&i) {
                continue;
            }
            assert_eq!(a, b, "byte {i} changed unexpectedly");
        }

        fs::remove_file(&pin).ok();
        fs::remove_file(&pout).ok();
    }

    #[test]
    fn models_json_lists_the_anytone() {
        let mut err: *mut c_char = std::ptr::null_mut();
        let json = take(unsafe { codeplugme_models_json(&mut err) });
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let models = v.as_array().unwrap();
        assert_eq!(models.len(), Model::ALL.len());
        assert_eq!(models[0]["id"], "anytone-d878uv2");
        assert_eq!(models[0]["display_name"], "AnyTone AT-D878UVII Plus");
        assert_eq!(models[0]["image_size"], codeplug_size());
        assert_eq!(models[0]["capabilities"]["supports_dmr"], true);
        assert_eq!(
            models[0]["capabilities"]["sections"]
                .as_array()
                .unwrap()
                .len(),
            7
        );
    }

    #[test]
    fn detect_model_recognizes_the_anytone_image() {
        let path = temp_path("detect.bin");
        fs::write(&path, synthetic_image()).unwrap();
        let cpath = c(path.to_str().unwrap());
        let mut err: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            take(unsafe { codeplugme_detect_model(cpath.as_ptr(), &mut err) }),
            "anytone-d878uv2"
        );

        // A file of any other length belongs to no radio this build knows.
        fs::write(&path, vec![0u8; 12_800]).unwrap();
        let mut err2: *mut c_char = std::ptr::null_mut();
        assert!(unsafe { codeplugme_detect_model(cpath.as_ptr(), &mut err2) }.is_null());
        assert!(take(err2).contains("matches no supported radio image"));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn codeplug_size_for_matches_the_model_and_rejects_unknown_ids() {
        let cid = c("anytone-d878uv2");
        let mut err: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { codeplugme_codeplug_size_for(cid.as_ptr(), &mut err) },
            codeplug_size()
        );

        let qid = c("qyt-kt5000");
        let mut errq: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { codeplugme_codeplug_size_for(qid.as_ptr(), &mut errq) },
            crate::qyt::device::IMAGE_SIZE
        );

        let bogus = c("no-such-radio");
        let mut err2: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { codeplugme_codeplug_size_for(bogus.as_ptr(), &mut err2) },
            0
        );
        assert!(take(err2).contains("unknown radio model"));
    }

    /// A hand-built QYT image, tag included so the detector recognizes it.
    fn synthetic_qyt_image() -> Vec<u8> {
        crate::qyt::codeplug::test_support::synthetic_qyt_image()
    }

    #[test]
    fn detect_model_names_the_qyt_image() {
        let path = temp_path("detect-qyt.bin");
        fs::write(&path, synthetic_qyt_image()).unwrap();
        let cpath = c(path.to_str().unwrap());
        let mut err: *mut c_char = std::ptr::null_mut();
        let id = take(unsafe { codeplugme_detect_model(cpath.as_ptr(), &mut err) });
        assert_eq!(id, "qyt-kt5000");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn models_json_lists_both_radios() {
        let mut err: *mut c_char = std::ptr::null_mut();
        let json = take(unsafe { codeplugme_models_json(&mut err) });
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let ids: Vec<&str> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["anytone-d878uv2", "qyt-kt5000"]);
        let qyt = &v[1];
        assert_eq!(qyt["display_name"], "QYT KT-5000");
        assert_eq!(qyt["image_size"], 12_800);
        assert_eq!(qyt["capabilities"]["channel_capacity"], 200);
        assert_eq!(qyt["capabilities"]["supports_dmr"], false);
    }

    #[test]
    fn dump_json_reports_the_qyt_envelope() {
        let path = temp_path("dump-qyt.bin");
        fs::write(&path, synthetic_qyt_image()).unwrap();
        let cpath = c(path.to_str().unwrap());
        let mut err: *mut c_char = std::ptr::null_mut();
        let json = take(unsafe { codeplugme_dump_json(cpath.as_ptr(), &mut err) });
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(v["model"], "qyt-kt5000");
        assert_eq!(v["display_name"], "QYT KT-5000");
        assert_eq!(v["capabilities"]["sections"][0], "qyt_channels");
        assert_eq!(v["qyt"]["channels"][0]["name"], "FRS-01");
        assert_eq!(v["qyt"]["channels"][2]["tx_tone"]["hz_tenths"], 1000);
        assert_eq!(v["qyt"]["settings"]["squelch"], 6);
        assert_eq!(v["qyt"]["bands"].as_array().unwrap().len(), 2);
        assert!(v.get("anytone").is_none());
        fs::remove_file(&path).ok();
    }

    #[test]
    fn apply_edits_roundtrips_a_qyt_envelope() {
        let input = synthetic_qyt_image();
        let pin = temp_path("qyt-edit-in.bin");
        let pout = temp_path("qyt-edit-out.bin");
        fs::write(&pin, &input).unwrap();

        let edits = r#"{"qyt":{
            "channels":[{"index":0,"name":"FFI","rx_tone":{"kind":"ctcss","hz_tenths":885}}],
            "settings":{"squelch":3}
        }}"#;
        let (cin, cedits, cout) = (
            c(pin.to_str().unwrap()),
            c(edits),
            c(pout.to_str().unwrap()),
        );
        let mut err: *mut c_char = std::ptr::null_mut();
        let status =
            unsafe { codeplugme_apply_edits(cin.as_ptr(), cedits.as_ptr(), cout.as_ptr(), &mut err) };
        assert_eq!(status, 0, "apply_edits failed: {}", take(err));

        let out = fs::read(&pout).unwrap();
        let cp = crate::qyt::codeplug::QytCodeplug::parse(&out).unwrap();
        let ch = cp.channels().next().unwrap();
        assert_eq!(ch.name, "FFI");
        assert_eq!(
            ch.rx_tone,
            crate::qyt::codeplug::Tone::Ctcss { hz_tenths: 885 }
        );
        assert_eq!(cp.settings().squelch, 3);
        // Channel 0's unnamed byte-15 bits survived the C ABI round trip.
        assert_eq!(out[15], 0x24);

        fs::remove_file(&pin).ok();
        fs::remove_file(&pout).ok();
    }

    #[test]
    fn apply_edits_rejects_a_batch_meant_for_the_other_radio() {
        let pin = temp_path("qyt-misaddressed-in.bin");
        let pout = temp_path("qyt-misaddressed-out.bin");
        fs::write(&pin, synthetic_qyt_image()).unwrap();
        let (cin, cedits, cout) = (
            c(pin.to_str().unwrap()),
            c(r#"{"anytone":{"channels":[{"index":0,"name":"NO"}]}}"#),
            c(pout.to_str().unwrap()),
        );
        let mut err: *mut c_char = std::ptr::null_mut();
        let status =
            unsafe { codeplugme_apply_edits(cin.as_ptr(), cedits.as_ptr(), cout.as_ptr(), &mut err) };
        assert_eq!(status, -1);
        assert!(take(err).contains("addressed to a different radio"));
        assert!(!pout.exists(), "a rejected edit must write no output file");
        fs::remove_file(&pin).ok();
    }

    #[test]
    fn ports_json_reports_a_model_alongside_likely_radio() {
        // Without hardware the list is usually empty, so this pins the contract
        // rather than the contents: the two fields must always agree.
        let mut err: *mut c_char = std::ptr::null_mut();
        let json = take(unsafe { codeplugme_ports_json(&mut err) });
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        for p in v.as_array().unwrap() {
            assert!(p.get("model").is_some(), "every port carries a model key");
            assert_eq!(p["likely_radio"], !p["model"].is_null());
        }
    }

    #[test]
    fn resolve_model_errors_on_an_unknown_port_with_no_explicit_model() {
        // A path no serial port has, so the USB lookup can name nothing. With
        // two supported models, guessing one would mean speaking the wrong
        // protocol at somebody's radio.
        let err = unsafe { resolve_model(std::ptr::null(), "/dev/cu.not-a-real-port") }
            .expect_err("an unattributable port must not resolve to a default model");
        assert!(err.contains("/dev/cu.not-a-real-port"), "{err}");
        assert!(
            err.contains("qyt-kt5000"),
            "the error must list what to pass: {err}"
        );
        assert!(err.contains("anytone-d878uv2"), "{err}");
    }

    #[test]
    fn resolve_model_honours_an_explicit_model_on_an_unknown_port() {
        // This is the whole QYT story until its VID/PID is known.
        let id = c("qyt-kt5000");
        let m = unsafe { resolve_model(id.as_ptr(), "/dev/cu.not-a-real-port") }.unwrap();
        assert_eq!(m, Model::QytKt5000);

        let bogus = c("no-such-radio");
        let err = unsafe { resolve_model(bogus.as_ptr(), "/dev/cu.not-a-real-port") }
            .expect_err("an unknown model id must be rejected, not inferred around");
        assert!(err.contains("no-such-radio"), "{err}");
    }

    #[test]
    fn dump_json_reports_model_and_capabilities() {
        let path = temp_path("envelope.bin");
        fs::write(&path, synthetic_image()).unwrap();
        let cpath = c(path.to_str().unwrap());
        let mut err: *mut c_char = std::ptr::null_mut();
        let json = take(unsafe { codeplugme_dump_json(cpath.as_ptr(), &mut err) });
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["model"], "anytone-d878uv2");
        assert_eq!(v["display_name"], "AnyTone AT-D878UVII Plus");
        assert_eq!(v["capabilities"]["channel_name_length"], 16);
        assert_eq!(v["capabilities"]["power_options"][0]["tag"], "Low");
        assert_eq!(v["anytone"]["channels"][0]["name"], "CALLING");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn apply_edits_accepts_a_legacy_flat_spec() {
        // The pre-envelope shape the CLI still sends. Keeping it working is why
        // the envelope treats a missing model key as "this whole object is the
        // spec" rather than as an empty edit batch.
        let pin = temp_path("legacy-in.bin");
        let pout = temp_path("legacy-out.bin");
        fs::write(&pin, synthetic_image()).unwrap();
        let (cin, ce, cout) = (
            c(pin.to_str().unwrap()),
            c(r#"{"channels":[{"index": 0, "name": "LEGACY"}]}"#),
            c(pout.to_str().unwrap()),
        );
        let mut err: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { codeplugme_apply_edits(cin.as_ptr(), ce.as_ptr(), cout.as_ptr(), &mut err) },
            0,
            "legacy flat spec was rejected"
        );
        let cp = Codeplug::parse(&fs::read(&pout).unwrap()).unwrap();
        assert_eq!(cp.channels().next().unwrap().name, "LEGACY");

        // And the tagged form reaches the same code path.
        let ce2 = c(r#"{"anytone":{"channels":[{"index": 0, "name": "TAGGED"}]}}"#);
        let mut err2: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { codeplugme_apply_edits(cin.as_ptr(), ce2.as_ptr(), cout.as_ptr(), &mut err2) },
            0,
            "tagged envelope was rejected"
        );
        let cp = Codeplug::parse(&fs::read(&pout).unwrap()).unwrap();
        assert_eq!(cp.channels().next().unwrap().name, "TAGGED");

        fs::remove_file(&pin).ok();
        fs::remove_file(&pout).ok();
    }

    #[test]
    fn apply_edits_rejects_inactive_channel() {
        let pin = temp_path("inactive.bin");
        fs::write(&pin, synthetic_image()).unwrap();
        let (cin, cedits) = (
            c(pin.to_str().unwrap()),
            c(r#"{"channels":[{"index": 7, "name": "X"}]}"#),
        );
        let mut err: *mut c_char = std::ptr::null_mut();
        let status =
            unsafe { codeplugme_apply_edits(cin.as_ptr(), cedits.as_ptr(), cin.as_ptr(), &mut err) };
        assert_eq!(status, -1);
        assert!(take(err).contains("not active"));
        fs::remove_file(&pin).ok();
    }

    #[test]
    fn apply_edits_adds_channel_contact_group_and_radio_id() {
        let pin = temp_path("add-dmr-in.bin");
        let pout = temp_path("add-dmr-out.bin");
        fs::write(&pin, synthetic_image()).unwrap();

        // Add a digital channel wired to a new talk group + group list + radio
        // ID, all in one apply.
        let edits = r#"{
            "add_channels": [{"name": "TG9 TS1", "rx_frequency_hz": 439000000,
                              "mode": "digital", "color_code": 1, "time_slot": 1,
                              "contact_index": 0, "group_list_index": 0,
                              "radio_id_index": 0, "power": "high", "bandwidth": "narrow"}],
            "add_contacts": [{"name": "LOCAL 9", "number": 9, "call_type": "group"}],
            "add_group_lists": [{"name": "LOCAL", "members": [0]}],
            "add_radio_ids": [{"name": "HOME", "number": 3141592}]
        }"#;
        let (cin, ce, cout) = (
            c(pin.to_str().unwrap()),
            c(edits),
            c(pout.to_str().unwrap()),
        );
        let mut err: *mut c_char = std::ptr::null_mut();
        let status =
            unsafe { codeplugme_apply_edits(cin.as_ptr(), ce.as_ptr(), cout.as_ptr(), &mut err) };
        assert_eq!(status, 0, "apply_edits failed: {}", take(err));

        // Re-dump and confirm every entity landed.
        let mut err2: *mut c_char = std::ptr::null_mut();
        let json = take(unsafe { codeplugme_dump_json(cout.as_ptr(), &mut err2) });
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let v = &v["anytone"];
        assert_eq!(v["channels"].as_array().unwrap().len(), 2);
        assert_eq!(v["contacts"][0]["name"], "LOCAL 9");
        assert_eq!(v["contacts"][0]["number"], 9);
        assert_eq!(v["contacts"][0]["call_type"], "Group");
        assert_eq!(v["group_lists"][0]["name"], "LOCAL");
        assert_eq!(v["group_lists"][0]["members"][0], 0);
        assert_eq!(v["radio_ids"][0]["number"], 3141592);
        let added = v["channels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "TG9 TS1")
            .unwrap();
        assert_eq!(added["mode"], "Digital");
        assert_eq!(added["time_slot"], 1);

        fs::remove_file(&pin).ok();
        fs::remove_file(&pout).ok();
    }

    #[test]
    fn apply_edits_removes_contact_and_scrubs_group_list() {
        // Seed a codeplug with two contacts and a group list referencing both,
        // then remove contact 0 and confirm the group list drops it.
        let pin = temp_path("rm-contact.bin");
        fs::write(&pin, synthetic_image()).unwrap();
        let cin = c(pin.to_str().unwrap());

        let seed = c(r#"{
            "add_contacts": [{"name": "A", "number": 1, "call_type": "group"},
                             {"name": "B", "number": 2, "call_type": "group"}],
            "add_group_lists": [{"name": "GL", "members": [0, 1]}]
        }"#);
        let mut err: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { codeplugme_apply_edits(cin.as_ptr(), seed.as_ptr(), cin.as_ptr(), &mut err) },
            0,
            "seed failed: {}",
            take(err)
        );

        let rm = c(r#"{"remove_contacts": [0]}"#);
        let mut err2: *mut c_char = std::ptr::null_mut();
        assert_eq!(
            unsafe { codeplugme_apply_edits(cin.as_ptr(), rm.as_ptr(), cin.as_ptr(), &mut err2) },
            0,
            "remove failed: {}",
            take(err2)
        );

        let mut err3: *mut c_char = std::ptr::null_mut();
        let json = take(unsafe { codeplugme_dump_json(cin.as_ptr(), &mut err3) });
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let v = &v["anytone"];
        assert_eq!(v["contacts"].as_array().unwrap().len(), 1);
        assert_eq!(v["contacts"][0]["index"], 1);
        assert_eq!(
            v["group_lists"][0]["members"].as_array().unwrap(),
            &vec![serde_json::json!(1)]
        );

        fs::remove_file(&pin).ok();
    }

    #[test]
    fn apply_edits_rejects_bad_frequency_and_long_name() {
        let pin = temp_path("badfreq.bin");
        fs::write(&pin, synthetic_image()).unwrap();
        let cin = c(pin.to_str().unwrap());

        let cedits = c(r#"{"channels":[{"index": 0, "rx_frequency_hz": 146520005}]}"#);
        let mut err: *mut c_char = std::ptr::null_mut();
        let status =
            unsafe { codeplugme_apply_edits(cin.as_ptr(), cedits.as_ptr(), cin.as_ptr(), &mut err) };
        assert_eq!(status, -1);
        assert!(take(err).contains("multiple of 10"));

        let cedits = c(r#"{"channels":[{"index": 0, "name": "THIS NAME IS WAY TOO LONG"}]}"#);
        let mut err: *mut c_char = std::ptr::null_mut();
        let status =
            unsafe { codeplugme_apply_edits(cin.as_ptr(), cedits.as_ptr(), cin.as_ptr(), &mut err) };
        assert_eq!(status, -1);
        assert!(take(err).contains("longer than"));
        fs::remove_file(&pin).ok();
    }

    #[test]
    fn restore_requires_force_before_touching_the_port() {
        let p = temp_path("restore.bin");
        fs::write(&p, vec![0u8; codeplug_size()]).unwrap();
        let (cport, cpath) = (c("/dev/nonexistent-port"), c(p.to_str().unwrap()));
        let mut err: *mut c_char = std::ptr::null_mut();
        let status = unsafe {
            codeplugme_restore(
                cport.as_ptr(),
                cpath.as_ptr(),
                false,
                None,
                std::ptr::null_mut(),
                &mut err,
            )
        };
        assert_eq!(status, -1);
        assert!(take(err).contains("backup"));
        fs::remove_file(&p).ok();
    }

    #[test]
    fn identify_bad_port_sets_error() {
        // With an explicit model there is nothing to infer, so the failure is
        // the port itself.
        let cport = c("/dev/nonexistent-anytone-port");
        let cmodel = c("anytone-d878uv2");
        let mut err: *mut c_char = std::ptr::null_mut();
        let out =
            unsafe { codeplugme_identify_json(cport.as_ptr(), cmodel.as_ptr(), &mut err) };
        assert!(out.is_null());
        assert!(take(err).contains("failed to open"));
    }

    #[test]
    fn an_unrecognized_port_with_no_model_is_ambiguous_now_that_there_are_two() {
        // The legacy shim carries no model parameter. With one supported radio
        // it could assume; with two it must say so rather than guess and then
        // talk the wrong protocol at whatever is plugged in.
        let cport = c("/dev/nonexistent-anytone-port");
        let mut err: *mut c_char = std::ptr::null_mut();
        let out = unsafe { codeplugme_identify(cport.as_ptr(), &mut err) };
        assert!(out.is_null());
        assert!(take(err).contains("cannot tell which radio"));
    }

    #[test]
    fn null_arguments_are_rejected_not_crashed() {
        let mut err: *mut c_char = std::ptr::null_mut();
        let out = unsafe { codeplugme_identify(std::ptr::null(), &mut err) };
        assert!(out.is_null());
        assert!(take(err).contains("must not be null"));
    }

    #[test]
    fn string_free_of_null_is_noop() {
        unsafe { codeplugme_string_free(std::ptr::null_mut()) };
    }
}
