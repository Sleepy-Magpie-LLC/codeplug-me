//! The UniFFI surface the Android app calls.
//!
//! This is the sibling of [`crate::ffi`]. Both are thin wrappers over the same
//! [`crate::driver`] and [`crate::document`] layers, and neither holds any
//! protocol or codeplug logic of its own. They differ only in how the host
//! reaches them and in how bulk data crosses:
//!
//! - `ffi` is a hand-written C ABI for the macOS Swift app. It takes filesystem
//!   paths and does its own `fs::read`/`fs::write`, and it opens the serial
//!   port itself through [`crate::transport::SerialTransport`].
//! - `api` (here) takes and returns byte vectors, because Android reaches files
//!   through the Storage Access Framework as content URIs rather than paths,
//!   and because the transport lives in Kotlin: `serialport` needs libudev,
//!   which Android does not have, so the USB link is driven by the platform's
//!   USB host API and passed in as a [`ByteTransport`].
//!
//! The largest supported image is 1.79 MB, so moving whole codeplugs by value
//! across the boundary is cheap enough to be the simpler choice.
//!
//! When a radio is added, both surfaces need the same new entry point. That is
//! the known cost of keeping the shipping macOS app on its existing ABI.

use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

use crate::document::Document;
use crate::driver::{driver_for, write_verified_image};
use crate::model::{Capabilities, Model};
use crate::transport::Transport;

/// Every failure crossing this boundary, reduced to a message.
///
/// The core's own [`crate::error::Error`] is richer, but the app only ever
/// displays these, and one variant keeps the generated Kotlin to a single type
/// whose message is always safe to show. This matches what the C ABI already
/// does with its `err_out` strings.
///
/// **This must not be a `#[uniffi(flat_error)]`.** A flat error can only travel
/// Rust to host. UniFFI generates its `Lift` the other way as a `panic!("Can't
/// lift flat errors")`, and [`ByteTransport`] is a foreign trait that returns
/// this type, so the first failure the Kotlin transport reported panicked
/// inside the lift and replaced the real message with that one. Found on
/// hardware 2026-09-04, where every KT-5000 read surfaced as "Can't lift flat
/// errors" and the actual cause was invisible. A named field costs nothing and
/// crosses in both directions.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ApiError {
    /// Something went wrong; the message is user-facing.
    #[error("{msg}")]
    Failed {
        /// Human-readable description of the failure.
        msg: String,
    },
}

impl ApiError {
    /// Build a failure from anything displayable.
    fn new(msg: impl std::fmt::Display) -> Self {
        ApiError::Failed {
            msg: msg.to_string(),
        }
    }
}

/// The byte link to a radio, implemented by the host.
///
/// On Android this wraps a `UsbSerialPort` from the USB host API. The contract
/// is exactly [`crate::transport::Transport`]'s, restated for a foreign
/// implementor: `write_all` must flush everything, and `read_exact` must block
/// until it has `len` bytes or fail. Returning short from `read_exact` is a
/// protocol error, not a partial success, so the implementation loops.
///
/// `Send + Sync` is required by UniFFI for foreign implementations. The core's
/// own `Transport` deliberately does not carry those bounds; [`ForeignTransport`]
/// bridges the two.
#[uniffi::export(with_foreign)]
pub trait ByteTransport: Send + Sync {
    /// Push the whole buffer to the radio, blocking until it is flushed.
    fn write_all(&self, bytes: Vec<u8>) -> Result<(), ApiError>;

    /// Return exactly `len` bytes, or fail (e.g. on timeout).
    fn read_exact(&self, len: u32) -> Result<Vec<u8>, ApiError>;
}

/// Block-level progress for a read or a write.
///
/// Called synchronously on the thread running the transfer, which is never the
/// Android main thread: a full transfer takes minutes.
#[uniffi::export(with_foreign)]
pub trait ProgressListener: Send + Sync {
    /// Report that `done` of `total` blocks have moved.
    fn on_progress(&self, done: u32, total: u32);
}

/// Adapts a host-supplied [`ByteTransport`] to the core's [`Transport`].
///
/// The only real work is turning `read_exact(&mut buf)` into
/// `read_exact(len) -> Vec<u8>`, since a foreign call cannot fill a Rust
/// buffer in place. A reply shorter than requested is treated as a failure
/// rather than silently accepted, matching `SerialTransport`.
struct ForeignTransport(Arc<dyn ByteTransport>);

impl Transport for ForeignTransport {
    /// Forward to the host, mapping its error into the core's error type.
    fn write_all(&mut self, buf: &[u8]) -> crate::error::Result<()> {
        self.0.write_all(buf.to_vec()).map_err(io_err)
    }

    /// Ask the host for exactly `buf.len()` bytes and copy them in.
    fn read_exact(&mut self, buf: &mut [u8]) -> crate::error::Result<()> {
        let got = self.0.read_exact(buf.len() as u32).map_err(io_err)?;
        if got.len() != buf.len() {
            return Err(crate::error::Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "transport returned {} bytes for a {}-byte read",
                    got.len(),
                    buf.len()
                ),
            )));
        }
        buf.copy_from_slice(&got);
        Ok(())
    }
}

/// Wrap a host transport error as an I/O error. The Android build has no
/// `serialport`, so [`crate::error::Error::Io`] is where every transport
/// failure lands.
fn io_err(e: ApiError) -> crate::error::Error {
    crate::error::Error::Io(std::io::Error::other(e.to_string()))
}

/// Build the `FnMut(done, total)` the driver layer expects, forwarding to the
/// optional listener.
fn progress_closure(listener: Option<Arc<dyn ProgressListener>>) -> impl FnMut(usize, usize) {
    move |done, total| {
        if let Some(l) = &listener {
            l.on_progress(done as u32, total as u32);
        }
    }
}

/// Look up a model by its id, with an error naming the valid ids.
fn model_by_id(id: &str) -> Result<Model, ApiError> {
    Model::from_id(id).ok_or_else(|| {
        let ids: Vec<&str> = Model::ALL.iter().map(|m| m.id()).collect();
        ApiError::new(format!(
            "unknown radio model {id:?} (expected one of: {})",
            ids.join(", ")
        ))
    })
}

/// One supported radio, as it appears in [`models_json`].
#[derive(serde::Serialize)]
struct ModelJson {
    id: &'static str,
    display_name: &'static str,
    image_size: usize,
    capabilities: Capabilities,
}

/// Every supported radio as JSON: `[{"id","display_name","image_size",
/// "capabilities":{...}}]`.
///
/// Same payload as the C ABI's `codeplugme_models_json`. The app reads its section
/// list, channel capacity, name length, and picker options from here rather
/// than carrying a parallel table.
#[uniffi::export]
pub fn models_json() -> Result<String, ApiError> {
    let models: Vec<ModelJson> = Model::ALL
        .iter()
        .map(|m| ModelJson {
            id: m.id(),
            display_name: m.display_name(),
            image_size: m.image_size(),
            capabilities: m.capabilities(),
        })
        .collect();
    serde_json::to_string(&models).map_err(ApiError::new)
}

/// The model id a USB VID/PID names, or `None` when we cannot attribute it.
///
/// This is the one source of truth for USB attribution, and the Android app
/// must call it rather than carrying its own table. Note that a `None` here is
/// routine, not an error: the KT-5000's programming cable is a stock Prolific
/// PL2303 shared with thousands of unrelated adapters, so it can never be
/// attributed from its descriptors and always needs the user to name the model.
#[uniffi::export]
pub fn model_for_usb_id(vid: u16, pid: u16) -> Option<String> {
    crate::usb_ids::model_for_usb_id(vid, pid).map(|m| m.id().to_string())
}

/// The model id an image's own bytes name, or `None` if it matches no radio.
///
/// Used to label a codeplug on import and, more importantly, to decide what a
/// write is allowed to target. See [`write_image`].
#[uniffi::export]
pub fn detect_model(image: Vec<u8>) -> Option<String> {
    Model::detect(&image).map(|m| m.id().to_string())
}

/// The exact image size in bytes for a model, for pre-checking a file before
/// it is imported.
#[uniffi::export]
pub fn image_size_for(model_id: String) -> Result<u64, ApiError> {
    Ok(model_by_id(&model_id)?.image_size() as u64)
}

/// The line rate this model's programming link runs at.
///
/// Android configures the USB bridge itself, so unlike the desktop it has to be
/// told. Getting this wrong is silent: the link opens, commands go out, and the
/// radio never answers. See [`crate::model::Model::baud`].
#[uniffi::export]
pub fn baud_for(model_id: String) -> Result<u32, ApiError> {
    Ok(model_by_id(&model_id)?.baud())
}

/// Handshake with the radio on `transport` and return its identity string.
///
/// Runs a complete session: the driver enters and leaves program mode itself.
/// The caller must not have opened the link for anything else, and must close
/// it afterwards. See the note on [`write_image`] about why identity checks
/// cannot be chained ahead of a transfer.
#[uniffi::export]
pub fn identify(
    transport: Arc<dyn ByteTransport>,
    model_id: String,
) -> Result<String, ApiError> {
    let model = model_by_id(&model_id)?;
    let driver = driver_for(model);
    let mut t = ForeignTransport(transport);
    driver
        .identify(&mut t as &mut dyn Transport)
        .map_err(ApiError::new)
}

/// Read the complete codeplug image off the radio, reporting block progress.
#[uniffi::export]
pub fn read_image(
    transport: Arc<dyn ByteTransport>,
    model_id: String,
    progress: Option<Arc<dyn ProgressListener>>,
) -> Result<Vec<u8>, ApiError> {
    let model = model_by_id(&model_id)?;
    let driver = driver_for(model);
    let mut t = ForeignTransport(transport);
    let mut cb = progress_closure(progress);
    driver
        .read_image(&mut t as &mut dyn Transport, &mut cb)
        .map_err(ApiError::new)
}

/// Write a complete codeplug image back to the radio.
///
/// Two gates, both deliberate and both matching the C ABI:
///
/// 1. **The model comes from the image's bytes, never from the caller.** A
///    QYT codeplug can therefore never reach an AnyTone or the reverse, no
///    matter what the UI believes is connected. The connected radio's identity
///    is checked against that model before the first write block goes out,
///    inside the same session as the transfer (see the comment on
///    [`crate::driver::write_verified_image`]: the KT-5000 rejects a second
///    handshake on an open link, so identify-then-write breaks on hardware).
/// 2. **`force` must be true.** Writing replaces the radio's entire
///    configuration and cannot be undone from here, so the caller has to say so
///    explicitly rather than reach it by a default argument.
///
/// `device_label` appears in the refusal message so the user can tell which
/// radio was refused; pass something they will recognise.
#[uniffi::export]
pub fn write_image(
    transport: Arc<dyn ByteTransport>,
    image: Vec<u8>,
    force: bool,
    device_label: String,
    progress: Option<Arc<dyn ProgressListener>>,
) -> Result<(), ApiError> {
    let model = Model::detect(&image).ok_or_else(|| {
        ApiError::new(format!(
            "codeplug is {} bytes, which matches no supported radio image",
            image.len()
        ))
    })?;
    if !force {
        return Err(ApiError::new(
            "writing replaces the radio's configuration; take a fresh backup \
             and confirm (force) to proceed",
        ));
    }
    let mut t = ForeignTransport(transport);
    let mut cb = progress_closure(progress);
    write_verified_image(
        model,
        &mut t as &mut dyn Transport,
        &image,
        &device_label,
        &mut cb,
    )
    .map_err(ApiError::new)
}

/// One run of bytes that came back different from what was sent.
///
/// `region` is the map label from `docs/qyt-kt5000-memory-map.md`, so a failure
/// reads as "12 bytes in channel names" rather than as a hex offset.
#[derive(Debug, uniffi::Record)]
pub struct ImageDiff {
    /// First differing address.
    pub addr: u32,
    /// How many consecutive bytes differ.
    pub len: u32,
    /// The map region `addr` falls in.
    pub region: String,
}

/// Whether [`verify_image`] can check a write to this model.
///
/// True for the KT-5000, whose write sends the image verbatim, so a read-back
/// is directly comparable with what was sent. False for the AnyTone, whose
/// write is a sector read-modify-write that deliberately leaves unmodelled
/// bytes as the radio had them: a whole-image comparison there would report
/// every such byte as a failure. That radio is also structurally safe from the
/// fault this exists for, and a full read of it takes minutes.
#[uniffi::export]
pub fn supports_write_verify(model_id: String) -> bool {
    matches!(model_by_id(&model_id), Ok(Model::QytKt5000))
}

/// Read the radio back after a write and report what does not match `sent`.
///
/// **An empty result is the only proof a write landed.** The KT-5000
/// acknowledges each block on receipt and commits it afterwards, so an ack is
/// not proof: a reset arriving in between loses a block the host was told it
/// had (measured 2026-09-09). Differences that say nothing about the transfer
/// are filtered out: the preserved window, everything above the write limit,
/// and the two addresses the firmware maintains itself.
///
/// This runs a complete session of its own, so the caller must open the link
/// fresh, after the radio has rebooted from the write, and close it afterwards.
#[uniffi::export]
pub fn verify_image(
    transport: Arc<dyn ByteTransport>,
    sent: Vec<u8>,
    model_id: String,
    progress: Option<Arc<dyn ProgressListener>>,
) -> Result<Vec<ImageDiff>, ApiError> {
    let model = model_by_id(&model_id)?;
    if !matches!(model, Model::QytKt5000) {
        return Err(ApiError::new(format!(
            "verifying a write is not supported for the {}",
            model.display_name()
        )));
    }
    if sent.len() != model.image_size() {
        return Err(ApiError::new(format!(
            "codeplug is {} bytes, expected {} for the {}",
            sent.len(),
            model.image_size(),
            model.display_name()
        )));
    }

    let driver = driver_for(model);
    let mut t = ForeignTransport(transport);
    let mut cb = progress_closure(progress);
    let back = driver
        .read_image(&mut t as &mut dyn Transport, &mut cb)
        .map_err(ApiError::new)?;
    let runs = crate::qyt::rescue::verify_diff(&sent, &back).map_err(ApiError::new)?;
    Ok(runs
        .into_iter()
        .map(|r| ImageDiff {
            addr: r.addr as u32,
            len: r.len as u32,
            region: r.region.to_string(),
        })
        .collect())
}

/// Map regions this image would blank if it were written to a radio.
///
/// Empty for an image read off the radio being written, and for the AnyTone,
/// whose write leaves unmodelled bytes alone. A non-empty answer means the
/// image is older or narrower than what the radio currently holds, which is
/// worth a sentence in front of the user before the write, not a diff after it.
#[uniffi::export]
pub fn erased_regions(image: Vec<u8>) -> Vec<String> {
    match Model::detect(&image) {
        Some(Model::QytKt5000) => crate::qyt::rescue::erased_regions(&image)
            .into_iter()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Parse an image offline and return the tagged envelope
/// `{"model","display_name","capabilities",<model key>:{...}}`.
#[uniffi::export]
pub fn dump_json(image: Vec<u8>) -> Result<String, ApiError> {
    let document = Document::parse(&image).map_err(ApiError::new)?;
    serde_json::to_string(&document.to_json()).map_err(ApiError::new)
}

/// Apply a batch of edits to an image and return the new image.
///
/// `edits_json` is the tagged envelope (`{"anytone":{...}}` or `{"qyt":{...}}`)
/// or, for backward compatibility, a bare AnyTone spec with no model key. Every
/// edit round-trips through the typed model and the result is verified by
/// re-parsing, so only the intended bytes change and unmodelled bytes survive.
#[uniffi::export]
pub fn apply_edits(image: Vec<u8>, edits_json: String) -> Result<Vec<u8>, ApiError> {
    crate::document::apply_edits(&image, &edits_json).map_err(ApiError::new)
}

/// Fetch repeaters near a location for `image`, which decides the radio (no
/// DMR for a KT-5000) and which results are flagged `duplicate`.
///
/// `query_json` is `{"lat","lon","radius_km","country","state","sources",
/// "repeaterbook":{"token","contact"}}`; `cache_dir` holds the free sources'
/// downloads for a day. Returns a JSON array, nearest first. Blocks on the
/// network, so call it from a background dispatcher.
#[uniffi::export]
pub fn geo_fetch(query_json: String, image: Vec<u8>, cache_dir: String) -> Result<String, ApiError> {
    crate::geo::fetch_json(&query_json, &image, &cache_dir).map_err(ApiError::new)
}

/// How many repeaters may be picked for `image` in `mode` (`"clean"` or
/// `"merge"`), as JSON `{"available","max_selectable","zone_limit",
/// "scan_list_limit","blocked_reason"}`.
#[uniffi::export]
pub fn geo_capacity(image: Vec<u8>, mode: String) -> Result<String, ApiError> {
    crate::geo::capacity_json(&image, &mode).map_err(ApiError::new)
}

/// Build a new image from `image` plus exactly the repeaters in
/// `repeaters_json` (a subset of a [`geo_fetch`] result). `options_json` is
/// `{"mode":"clean"|"merge","zone_name"}`. A selection larger than the radio
/// can take is an error, never truncated.
#[uniffi::export]
pub fn geo_build(
    image: Vec<u8>,
    repeaters_json: String,
    options_json: String,
) -> Result<Vec<u8>, ApiError> {
    crate::geo::build_json(&image, &repeaters_json, &options_json).map_err(ApiError::new)
}

/// Adapts a `Transport` test double to [`ByteTransport`].
///
/// Exposed to the crate's own tests only. The `Mutex` is what lets a `&mut`
/// core transport satisfy `ByteTransport`'s `&self` + `Send + Sync` contract,
/// which is the same shape the Kotlin implementation ends up with.
#[cfg(test)]
pub(crate) struct SharedTransport<T: Transport + Send>(Mutex<T>);

#[cfg(test)]
impl<T: Transport + Send> SharedTransport<T> {
    /// Wrap a transport so it can cross the UniFFI boundary.
    pub(crate) fn new(inner: T) -> Self {
        Self(Mutex::new(inner))
    }

    /// Take the wrapped transport back for assertions.
    pub(crate) fn into_inner(self) -> T {
        self.0.into_inner().expect("transport mutex not poisoned")
    }
}

#[cfg(test)]
impl<T: Transport + Send> ByteTransport for SharedTransport<T> {
    fn write_all(&self, bytes: Vec<u8>) -> Result<(), ApiError> {
        self.0
            .lock()
            .expect("transport mutex not poisoned")
            .write_all(&bytes)
            .map_err(ApiError::new)
    }

    fn read_exact(&self, len: u32) -> Result<Vec<u8>, ApiError> {
        let mut buf = vec![0u8; len as usize];
        self.0
            .lock()
            .expect("transport mutex not poisoned")
            .read_exact(&mut buf)
            .map_err(ApiError::new)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anytone::mock::MockTransport;
    use crate::qyt::mock::QytMock;

    /// A QYT image with the model tag in place, so `Model::detect` names it.
    fn qyt_image() -> Vec<u8> {
        crate::qyt::codeplug::test_support::synthetic_qyt_image()
    }

    /// Counts progress callbacks so a test can assert one actually fired.
    struct CountingProgress(Mutex<Vec<(u32, u32)>>);

    impl ProgressListener for CountingProgress {
        fn on_progress(&self, done: u32, total: u32) {
            self.0
                .lock()
                .expect("progress mutex not poisoned")
                .push((done, total));
        }
    }

    /// The error type must cross **host to Rust**, not just Rust to host.
    ///
    /// [`ByteTransport`] is implemented in Kotlin and returns `ApiError`, so
    /// every failure the phone's USB layer reports has to be lifted back into
    /// Rust. A `#[uniffi(flat_error)]` generates that lift as
    /// `panic!("Can't lift flat errors")`, which is exactly what happened on
    /// hardware 2026-09-04: the first read timeout panicked inside the lift and
    /// the user saw that panic instead of the real cause. This round trip is
    /// the guard; it fails the moment the attribute comes back.
    #[test]
    fn an_api_error_survives_a_round_trip_from_the_host() {
        use uniffi::{Lift, Lower};

        let sent = "radio stopped responding: got 0 of 50 bytes before timing out";
        let buf = <ApiError as Lower<crate::UniFfiTag>>::lower(ApiError::new(sent));
        let back = <ApiError as Lift<crate::UniFfiTag>>::try_lift(buf)
            .expect("a host-reported error must lift back into Rust");
        assert_eq!(back.to_string(), sent);
    }

    #[test]
    fn models_json_lists_every_supported_radio_with_its_capabilities() {
        let v: serde_json::Value = serde_json::from_str(&models_json().unwrap()).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), Model::ALL.len());
        let ids: Vec<&str> = arr.iter().map(|m| m["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"anytone-d878uv2"));
        assert!(ids.contains(&"qyt-kt5000"));
        // Capabilities must be present, since the app renders its panes from them.
        assert!(arr[0]["capabilities"]["sections"].is_array());
    }

    #[test]
    fn usb_ids_resolve_through_the_cores_table_not_a_second_one() {
        assert_eq!(
            model_for_usb_id(0x28e9, 0x018a).as_deref(),
            Some("anytone-d878uv2")
        );
        assert_eq!(
            model_for_usb_id(0x2e3c, 0x5740).as_deref(),
            Some("anytone-d878uv2")
        );
        // The KT-5000's PL2303 cable is deliberately unattributable.
        assert_eq!(model_for_usb_id(0x067b, 0x2303), None);
    }

    #[test]
    fn detect_model_names_the_radio_an_image_belongs_to() {
        assert_eq!(detect_model(qyt_image()).as_deref(), Some("qyt-kt5000"));
        assert_eq!(detect_model(vec![0u8; 7]), None);
    }

    /// Android has no OS driver to fall back on, so a wrong answer here is a
    /// radio that never replies. It must come from the core's table.
    #[test]
    fn baud_for_reports_each_radios_own_link_rate() {
        assert_eq!(baud_for("qyt-kt5000".into()).unwrap(), 9_600);
        assert_eq!(baud_for("anytone-d878uv2".into()).unwrap(), 115_200);
        let e = baud_for("nope".into()).unwrap_err().to_string();
        assert!(e.contains("nope"), "{e}");
    }

    #[test]
    fn image_size_for_rejects_an_unknown_model_by_name() {
        assert_eq!(image_size_for("qyt-kt5000".into()).unwrap(), 0x3200);
        let e = image_size_for("nope".into()).unwrap_err().to_string();
        assert!(e.contains("nope"), "{e}");
        assert!(e.contains("qyt-kt5000"), "expected valid ids listed: {e}");
    }

    #[test]
    fn identify_runs_a_complete_session_against_a_fake_radio() {
        let t = Arc::new(SharedTransport::new(QytMock::new()));
        let id = identify(t, "qyt-kt5000".into()).unwrap();
        assert!(id.contains("KT-500"), "{id}");
    }

    #[test]
    fn read_image_returns_the_full_image_and_reports_progress() {
        let t = Arc::new(SharedTransport::new(QytMock::new()));
        let p = Arc::new(CountingProgress(Mutex::new(Vec::new())));
        let img = read_image(t, "qyt-kt5000".into(), Some(p.clone())).unwrap();
        assert_eq!(img.len(), 0x3200);
        let seen = p.0.lock().unwrap();
        assert!(!seen.is_empty(), "progress listener was never called");
        let (done, total) = *seen.last().unwrap();
        assert_eq!(done, total, "final progress must be complete");
    }

    #[test]
    fn read_image_works_with_no_progress_listener() {
        let t = Arc::new(SharedTransport::new(QytMock::new()));
        assert_eq!(
            read_image(t, "qyt-kt5000".into(), None).unwrap().len(),
            0x3200
        );
    }

    #[test]
    fn write_image_refuses_without_force_before_touching_the_radio() {
        let inner = SharedTransport::new(QytMock::new());
        let t = Arc::new(inner);
        let e = write_image(t.clone(), qyt_image(), false, "test".into(), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("force"), "{e}");
        // Nothing may have gone out on the wire.
        let mock = Arc::try_unwrap(t).ok().unwrap().into_inner();
        assert!(
            mock.written.is_empty(),
            "refused write still emitted {} bytes",
            mock.written.len()
        );
    }

    #[test]
    fn write_image_refuses_an_image_that_matches_no_radio() {
        let t = Arc::new(SharedTransport::new(QytMock::new()));
        let e = write_image(t, vec![0u8; 99], true, "test".into(), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("99 bytes"), "{e}");
    }

    #[test]
    fn write_image_refuses_a_qyt_codeplug_aimed_at_an_anytone() {
        // The image says QYT; the radio on the link is an AnyTone. The guard
        // must fire before any write block is emitted.
        let inner = SharedTransport::new(MockTransport::new(crate::codeplug_size(), "ID878UV"));
        let t = Arc::new(inner);
        let e = write_image(t.clone(), qyt_image(), true, "usb-1".into(), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("usb-1"), "device label should name the target: {e}");
        let mock = Arc::try_unwrap(t).ok().unwrap().into_inner();
        assert!(
            !mock.written.windows(1).any(|w| w == [b'W']),
            "a refused cross-model write emitted a write block"
        );
    }

    #[test]
    fn dump_json_returns_the_tagged_envelope() {
        let v: serde_json::Value = serde_json::from_str(&dump_json(qyt_image()).unwrap()).unwrap();
        assert_eq!(v["model"], "qyt-kt5000");
        assert!(v["qyt"].is_object(), "payload must be nested under its model key");
        assert!(v["capabilities"]["sections"].is_array());
    }

    #[test]
    fn dump_json_rejects_bytes_that_are_not_a_codeplug() {
        assert!(dump_json(vec![0u8; 10]).is_err());
    }

    #[test]
    fn verify_image_is_clean_after_a_write_the_radio_really_took() {
        let image = qyt_image();
        let mock = QytMock::new();
        let t = Arc::new(SharedTransport::new(mock));
        write_image(t.clone(), image.clone(), true, "usb-1".into(), None).unwrap();

        // The radio reboots when the port closes; verification is its own
        // session on a fresh link, which is what the mock's reboot models.
        let mock = Arc::try_unwrap(t).ok().unwrap().into_inner();
        let mut mock = mock;
        mock.reboot();
        let t = Arc::new(SharedTransport::new(mock));
        let diffs = verify_image(t, image, "qyt-kt5000".into(), None).unwrap();
        assert!(
            diffs.is_empty(),
            "a write the radio took must verify clean, got {:?}",
            diffs.iter().map(|d| (d.addr, d.len)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn verify_image_names_the_region_a_dropped_block_landed_in() {
        // A radio that kept everything except one block of channel names, which
        // is what a reset between the ack and the commit leaves behind.
        let held = qyt_image();
        let mut image = held.clone();
        for b in &mut image[0x1A50..0x1A60] {
            *b = 0x41;
        }
        let mut mock = QytMock::new();
        mock.seed(0, &held);
        let t = Arc::new(SharedTransport::new(mock));

        let diffs = verify_image(t, image, "qyt-kt5000".into(), None).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].addr, 0x1A50);
        assert_eq!(diffs[0].region, "channel names");
    }

    #[test]
    fn verify_is_offered_only_where_it_would_mean_something() {
        assert!(supports_write_verify("qyt-kt5000".into()));
        assert!(!supports_write_verify("anytone-d878uv2".into()));
        assert!(!supports_write_verify("nope".into()));

        let t = Arc::new(SharedTransport::new(QytMock::new()));
        let e = verify_image(t, vec![0u8; 8], "qyt-kt5000".into(), None)
            .unwrap_err()
            .to_string();
        assert!(e.contains("8 bytes"), "{e}");
    }

    #[test]
    fn erased_regions_reports_only_what_a_write_would_blank() {
        // A synthetic image carries no autodial memories and no factory blob,
        // which is exactly the shape this warning exists to catch.
        let synthetic = qyt_image();
        assert_eq!(
            erased_regions(synthetic.clone()),
            vec!["factory blob".to_string(), "DTMF / autodial".to_string()]
        );

        // Fill them the way a real read off a radio would, and nothing is left
        // to warn about.
        let mut populated = synthetic.clone();
        for b in &mut populated[0x2AF0..0x2B00] {
            *b = 0x11;
        }
        for b in &mut populated[0x3000..0x3100] {
            *b = 0x22;
        }
        assert!(erased_regions(populated.clone()).is_empty());

        for b in &mut populated[0x3000..0x3100] {
            *b = 0xFF;
        }
        assert_eq!(
            erased_regions(populated),
            vec!["DTMF / autodial".to_string()]
        );

        // Not a codeplug at all, and the AnyTone, both answer empty.
        assert!(erased_regions(vec![0xFFu8; 64]).is_empty());
    }

    #[test]
    fn apply_edits_changes_only_what_the_spec_names() {
        let before = qyt_image();
        let spec = r#"{"qyt":{"channels":[{"index":0,"name":"TEST"}]}}"#;
        let after = apply_edits(before.clone(), spec.into()).unwrap();
        assert_eq!(after.len(), before.len());
        assert_ne!(after, before, "the edit should have changed something");

        let v: serde_json::Value = serde_json::from_str(&dump_json(after).unwrap()).unwrap();
        let ch0 = &v["qyt"]["channels"][0];
        assert_eq!(ch0["name"], "TEST");
    }

    #[test]
    fn apply_edits_refuses_a_batch_addressed_to_the_other_model() {
        let e = apply_edits(qyt_image(), r#"{"anytone":{"channels":[]}}"#.into())
            .unwrap_err()
            .to_string();
        assert!(!e.is_empty());
    }

    #[test]
    fn a_short_read_from_the_host_surfaces_as_a_failure_not_a_partial_success() {
        /// A transport whose `read_exact` under-delivers, which is what a
        /// buggy Kotlin implementation would do.
        struct ShortReader;
        impl ByteTransport for ShortReader {
            fn write_all(&self, _bytes: Vec<u8>) -> Result<(), ApiError> {
                Ok(())
            }
            fn read_exact(&self, len: u32) -> Result<Vec<u8>, ApiError> {
                Ok(vec![0u8; (len as usize).saturating_sub(1)])
            }
        }
        let e = identify(Arc::new(ShortReader), "qyt-kt5000".into())
            .unwrap_err()
            .to_string();
        assert!(!e.is_empty(), "a short read must not be accepted silently");
    }
}
