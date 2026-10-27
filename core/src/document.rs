//! The model-tagged codeplug document and its JSON envelope.
//!
//! A `.bin` on disk is a raw radio image with no header, so [`Document::parse`]
//! identifies the model from the bytes ([`Model::detect`]) and then parses with
//! that model's codeplug layer.
//!
//! Everything crossing into the app is wrapped in a tagged envelope:
//!
//! ```json
//! { "model": "anytone-d878uv2",
//!   "display_name": "AnyTone AT-D878UVII Plus",
//!   "capabilities": { ... },
//!   "anytone": { "channels": [...], "zones": [...], ... } }
//! ```
//!
//! The per-model payload is nested rather than flattened on purpose. It lets
//! each model's Swift decode type keep every field non-optional, so a core bug
//! that drops a whole family fails loudly instead of silently rendering an
//! empty pane.

use serde::Deserialize;

use crate::anytone::codeplug::Codeplug;
use crate::error::{Error, Result};
use crate::model::Model;
use crate::qyt::codeplug::QytCodeplug;

/// A parsed codeplug, tagged with the model it came from.
pub enum Document {
    /// An AnyTone AT-D878UVII Plus codeplug.
    AnyTone(Codeplug),
    /// A QYT KT-5000 codeplug.
    Qyt(QytCodeplug),
}

/// Envelope for an incoming edit batch. Absent keys mean "no edits for that
/// model", which is what makes a legacy flat spec detectable.
#[derive(Deserialize)]
struct EditEnvelope {
    #[serde(default)]
    anytone: Option<serde_json::Value>,
    #[serde(default)]
    qyt: Option<serde_json::Value>,
}

impl Document {
    /// Detect the model from `data` and parse it with that model's layer.
    pub fn parse(data: &[u8]) -> Result<Document> {
        match model_of(data)? {
            Model::AnyToneD878UV2 => Ok(Document::AnyTone(Codeplug::parse(data)?)),
            Model::QytKt5000 => Ok(Document::Qyt(QytCodeplug::parse(data)?)),
        }
    }

    /// The model this document belongs to.
    pub fn model(&self) -> Model {
        match self {
            Document::AnyTone(_) => Model::AnyToneD878UV2,
            Document::Qyt(_) => Model::QytKt5000,
        }
    }

    /// Build the tagged JSON envelope the app consumes.
    pub fn to_json(&self) -> serde_json::Value {
        let model = self.model();
        let payload = match self {
            Document::AnyTone(cp) => serde_json::to_value(cp.to_json()),
            Document::Qyt(cp) => serde_json::to_value(cp.to_json()),
        }
        .expect("codeplug projection is always serializable");
        let mut env = serde_json::Map::new();
        env.insert("model".into(), model.id().into());
        env.insert("display_name".into(), model.display_name().into());
        env.insert(
            "capabilities".into(),
            serde_json::to_value(model.capabilities())
                .expect("capabilities are always serializable"),
        );
        env.insert(model.payload_key().to_string(), payload);
        serde_json::Value::Object(env)
    }

    /// Re-encode the document back to a full radio image.
    pub fn serialize(&self) -> Vec<u8> {
        match self {
            Document::AnyTone(cp) => cp.serialize(),
            Document::Qyt(cp) => cp.serialize(),
        }
    }
}

/// Detect `data`'s model, with an error message that names the length mismatch
/// rather than leaving the caller to guess.
fn model_of(data: &[u8]) -> Result<Model> {
    Model::detect(data).ok_or_else(|| {
        Error::Parse(format!(
            "{} bytes does not match any supported radio image",
            data.len()
        ))
    })
}

/// Apply an edit batch to a raw image and return the new image.
///
/// `edits_json` is either the tagged envelope (`{"anytone": {...}}`) or, for
/// backward compatibility, a bare per-model edit spec with no model key at all.
/// The bare form is what the CLI's `edit` command and every pre-envelope caller
/// still send, so it stays supported rather than being migrated.
pub fn apply_edits(data: &[u8], edits_json: &str) -> std::result::Result<Vec<u8>, String> {
    let model = model_of(data).map_err(|e| e.to_string())?;
    let envelope: EditEnvelope =
        serde_json::from_str(edits_json).map_err(|e| format!("invalid edits JSON: {e}"))?;

    // An edit batch carrying another radio's payload key is a mis-addressed
    // batch, not an empty one. Reject it before anything is applied: the
    // alternative is a silent no-op, because each model's spec ignores fields
    // it does not know.
    let addressed_elsewhere = match model {
        Model::AnyToneD878UV2 => envelope.qyt.is_some(),
        Model::QytKt5000 => envelope.anytone.is_some(),
    };
    if addressed_elsewhere {
        return Err(format!(
            "these edits are addressed to a different radio than this {} image",
            model.display_name()
        ));
    }

    match model {
        Model::AnyToneD878UV2 => {
            let spec = match envelope.anytone {
                Some(v) => {
                    serde_json::to_string(&v).map_err(|e| format!("invalid anytone edits: {e}"))?
                }
                // No model key: the whole object is the spec (legacy shape).
                None => edits_json.to_string(),
            };
            crate::anytone::codeplug::apply_edits(data, &spec)
        }
        Model::QytKt5000 => {
            let spec = match envelope.qyt {
                Some(v) => {
                    serde_json::to_string(&v).map_err(|e| format!("invalid qyt edits: {e}"))?
                }
                // The QYT has no legacy flat form to stay compatible with: it
                // never shipped before the envelope existed.
                None => edits_json.to_string(),
            };
            crate::qyt::codeplug::apply_edits(data, &spec)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anytone::codeplug::{global_offset, CONTACT_BITMAP};

    /// An image with no active records. The contact bitmap is inverted, so a
    /// bare zero-fill would read back as ten thousand live contacts.
    fn empty_anytone_image() -> Vec<u8> {
        let mut raw = vec![0u8; Model::AnyToneD878UV2.image_size()];
        let off = global_offset(CONTACT_BITMAP).unwrap();
        for b in raw[off..off + 0x500].iter_mut() {
            *b = 0xff;
        }
        raw
    }

    /// The same image with one named channel, so edits have something to target.
    fn image_with_a_channel() -> Vec<u8> {
        let mut cp = Codeplug::parse(&empty_anytone_image()).unwrap();
        let i = cp.add_channel().unwrap();
        cp.channel_mut(i).unwrap().set_name("ORIGINAL");
        cp.serialize()
    }

    #[test]
    fn parse_detects_the_anytone_model() {
        let doc = Document::parse(&empty_anytone_image()).unwrap();
        assert_eq!(doc.model(), Model::AnyToneD878UV2);
    }

    #[test]
    fn parse_rejects_an_unknown_image_length() {
        let err = match Document::parse(&[0u8; 128]) {
            Ok(_) => panic!("a 128-byte blob must not parse as any radio image"),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("does not match any supported radio image"),
            "{err}"
        );
    }

    #[test]
    fn dump_json_reports_model_and_capabilities() {
        let doc = Document::parse(&image_with_a_channel()).unwrap();
        let v = doc.to_json();
        assert_eq!(v["model"], "anytone-d878uv2");
        assert_eq!(v["display_name"], "AnyTone AT-D878UVII Plus");
        assert_eq!(v["capabilities"]["supports_dmr"], true);
        assert_eq!(v["capabilities"]["sections"][0], "channels");
        // The payload is exactly the pre-envelope dump shape, just nested.
        assert_eq!(v["anytone"]["channels"][0]["name"], "ORIGINAL");
        assert!(v["anytone"]["zones"].is_array());
        assert!(v["anytone"].get("aprs").is_some());
    }

    #[test]
    fn apply_edits_accepts_a_legacy_flat_spec() {
        // No model key anywhere: exactly what the CLI's `edit` command sends.
        let out = apply_edits(
            &image_with_a_channel(),
            r#"{"channels":[{"index":0,"name":"RENAMED"}]}"#,
        )
        .unwrap();
        let cp = Codeplug::parse(&out).unwrap();
        assert_eq!(cp.channels().next().unwrap().name, "RENAMED");
    }

    #[test]
    fn apply_edits_accepts_the_tagged_envelope() {
        let out = apply_edits(
            &image_with_a_channel(),
            r#"{"anytone":{"channels":[{"index":0,"name":"ENVELOPE"}]}}"#,
        )
        .unwrap();
        let cp = Codeplug::parse(&out).unwrap();
        assert_eq!(cp.channels().next().unwrap().name, "ENVELOPE");
    }

    #[test]
    fn parse_detects_the_qyt_model_and_dumps_it_under_its_own_key() {
        let img = crate::qyt::codeplug::test_support::synthetic_qyt_image();
        let doc = Document::parse(&img).unwrap();
        assert_eq!(doc.model(), Model::QytKt5000);

        let v = doc.to_json();
        assert_eq!(v["model"], "qyt-kt5000");
        assert_eq!(v["display_name"], "QYT KT-5000");
        assert_eq!(v["capabilities"]["supports_dmr"], false);
        assert_eq!(v["capabilities"]["sections"][0], "qyt_channels");
        assert_eq!(v["qyt"]["channels"][0]["name"], "FRS-01");
        assert_eq!(v["qyt"]["static_message"], "Model:KT-5000");
        // The AnyTone key must not appear on a QYT document, or vice versa.
        assert!(v.get("anytone").is_none());
        assert_eq!(doc.serialize(), img);
    }

    #[test]
    fn apply_edits_routes_the_qyt_payload_to_the_qyt_layer() {
        let img = crate::qyt::codeplug::test_support::synthetic_qyt_image();
        let out = apply_edits(&img, r#"{"qyt":{"channels":[{"index":0,"name":"ENV"}]}}"#).unwrap();
        let cp = crate::qyt::codeplug::QytCodeplug::parse(&out).unwrap();
        assert_eq!(cp.channels().next().unwrap().name, "ENV");
    }

    #[test]
    fn apply_edits_rejects_a_qyt_spec_on_an_anytone_file() {
        // Each model's spec silently ignores fields it does not know, so a
        // mis-addressed batch would otherwise apply as a no-op and look like it
        // had worked.
        let err = apply_edits(
            &image_with_a_channel(),
            r#"{"qyt":{"channels":[{"index":0,"name":"WRONG"}]}}"#,
        )
        .unwrap_err();
        assert!(err.contains("addressed to a different radio"), "{err}");

        let img = crate::qyt::codeplug::test_support::synthetic_qyt_image();
        let err = apply_edits(&img, r#"{"anytone":{"channels":[{"index":0,"name":"X"}]}}"#)
            .unwrap_err();
        assert!(err.contains("addressed to a different radio"), "{err}");
    }

    #[test]
    fn apply_edits_round_trips_through_serialize() {
        let doc = Document::parse(&image_with_a_channel()).unwrap();
        assert_eq!(doc.serialize().len(), Model::AnyToneD878UV2.image_size());
    }
}
