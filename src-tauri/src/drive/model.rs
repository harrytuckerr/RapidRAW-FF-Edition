//! Strict received-revision envelopes, not a portable-payload codec.
//!
//! `decode_revision` is the raw-byte entry point: it rejects duplicate JSON keys
//! before typed decoding and retains the exact received bytes and their SHA-256.
//! Passing these checks establishes neither account ownership nor payload safety.
//! No type in this module represents a payload ready to apply, render or publish.

use std::collections::HashMap;
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const MAX_JSON_BYTES: usize = 1024 * 1024;
/// Counts object/array containers, including the root object; scalars add no depth.
pub const MAX_JSON_DEPTH: usize = 32;
pub const MAX_REVISION_REFS: usize = 32;
pub const MAX_DEPENDENCIES: usize = 256;
pub const MAX_TEXT_SCALARS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeError {
    SizeLimit,
    DepthLimit,
    DuplicateJsonKey,
    InvalidJson,
    InvalidEnvelope,
    UnsupportedVersion,
    WrongSpace,
    ObjectKindMismatch,
    ReferenceLimit,
    DependencyLimit,
    DuplicateRevisionRef,
    ConflictingRevisionRef,
    SelfReference,
    DuplicateBlobRef,
    ConflictingBlobLength,
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Deliberately exclude received keys, payloads, paths and other raw input.
        write!(f, "invalid revision envelope: {self:?}")
    }
}
impl std::error::Error for EnvelopeError {}

/// Lowercase SHA-256 of bytes; not the existing DCP BLAKE3 identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256Hex(String);
impl TryFrom<String> for Sha256Hex {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(value))
        } else {
            Err("expected lowercase 64-hex SHA-256")
        }
    }
}
impl From<Sha256Hex> for String {
    fn from(value: Sha256Hex) -> Self {
        value.0
    }
}
impl Sha256Hex {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(64);
        for byte in digest.iter().copied() {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 15)]));
        }
        Self(text)
    }
}

/// Hyphenated lowercase UUID. No UUID version or authenticated identity is inferred.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CanonicalUuid(String);
impl TryFrom<String> for CanonicalUuid {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() != 36 {
            return Err("expected canonical UUID");
        }
        let parsed = Uuid::parse_str(&value).map_err(|_| "expected canonical UUID")?;
        if parsed.hyphenated().to_string() != value {
            return Err("expected canonical UUID");
        }
        Ok(Self(value))
    }
}
impl From<CanonicalUuid> for String {
    fn from(value: CanonicalUuid) -> Self {
        value.0
    }
}
impl CanonicalUuid {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical unsigned decimal *string* on the wire, with the full u64 range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnsignedDecimal(u64);
impl UnsignedDecimal {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn value(self) -> u64 {
        self.0
    }
}
impl Serialize for UnsignedDecimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for UnsignedDecimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.is_empty()
            || text.len() > 20
            || !text.bytes().all(|b| b.is_ascii_digit())
            || (text.len() > 1 && text.starts_with('0'))
        {
            return Err(de::Error::custom(
                "expected canonical unsigned decimal string",
            ));
        }
        text.parse::<u64>()
            .map(Self)
            .map_err(|_| de::Error::custom("unsigned decimal overflow"))
    }
}

/// Bounded display metadata only. This does not parse timestamps or certify a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BoundedText(String);
impl TryFrom<String> for BoundedText {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty()
            || value.chars().count() > MAX_TEXT_SCALARS
            || value.chars().any(char::is_control)
        {
            Err("expected nonblank display text of at most 256 scalars without controls")
        } else {
            Ok(Self(value))
        }
    }
}
impl From<BoundedText> for String {
    fn from(value: BoundedText) -> Self {
        value.0
    }
}
impl BoundedText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Conservative syntax for an opaque provider ID, not proof of a Google account.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProviderId(String);
impl TryFrom<String> for ProviderId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if !value.is_empty()
            && value.len() <= 256
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            Ok(Self(value))
        } else {
            Err("invalid opaque provider ID syntax")
        }
    }
}
impl From<ProviderId> for String {
    fn from(value: ProviderId) -> Self {
        value.0
    }
}
impl ProviderId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Syntax only: authenticating this key requires a separate native provider step.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountKey {
    pub project_number: UnsignedDecimal,
    pub drive_permission_id: ProviderId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageSpace {
    Drive,
    AppDataFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlobKind {
    ProfileDcp,
    ProfileLook,
    Lut,
    Patch,
    Mask,
    Watermark,
    State,
    OriginalPhoto,
    RenderedExport,
}
impl BlobKind {
    pub fn required_space(self) -> StorageSpace {
        match self {
            Self::OriginalPhoto | Self::RenderedExport => StorageSpace::Drive,
            _ => StorageSpace::AppDataFolder,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlobRef {
    pub sha256: Sha256Hex,
    pub bytes: UnsignedDecimal,
    pub kind: BlobKind,
}
impl BlobRef {
    pub fn validate_space(&self, space: StorageSpace) -> Result<(), EnvelopeError> {
        if self.kind.required_space() == space {
            Ok(())
        } else {
            Err(EnvelopeError::WrongSpace)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionRef {
    pub revision_id: CanonicalUuid,
    pub sha256: Sha256Hex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RevisionKind {
    Settings,
    WorkspaceBinding,
    Edit,
    Media,
    Checkpoint,
    Preset,
    PresetFolder,
    Profile,
    ExportPreset,
    Tombstone,
}

/// Workspace-scoped object identity; paths and arbitrary namespace strings are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ObjectId(String);
fn object_kind(value: &str) -> Option<RevisionKind> {
    if value == "settings" {
        return Some(RevisionKind::Settings);
    }
    if value == "workspaceBinding" {
        return Some(RevisionKind::WorkspaceBinding);
    }
    let mut parts = value.split(':');
    let kind = match parts.next()? {
        "edit" => RevisionKind::Edit,
        "media" => RevisionKind::Media,
        "checkpoint" => RevisionKind::Checkpoint,
        "preset" => RevisionKind::Preset,
        "presetFolder" => RevisionKind::PresetFolder,
        "profile" => RevisionKind::Profile,
        "exportPreset" => RevisionKind::ExportPreset,
        _ => return None,
    };
    CanonicalUuid::try_from(parts.next()?.to_owned()).ok()?;
    if kind == RevisionKind::Edit {
        CanonicalUuid::try_from(parts.next()?.to_owned()).ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(kind)
}
impl TryFrom<String> for ObjectId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() <= 80 && object_kind(&value).is_some() {
            Ok(Self(value))
        } else {
            Err("invalid object identity")
        }
    }
}
impl From<ObjectId> for String {
    fn from(value: ObjectId) -> Self {
        value.0
    }
}
impl ObjectId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadApplicability {
    Unvalidated,
}

/// Opaque content may contain unsafe paths, URLs, unknown effects or invalid values.
/// Envelope validity grants no permission to apply or publish this content.
#[derive(Serialize)]
#[serde(transparent)]
pub struct UnvalidatedPayload(Value);
impl UnvalidatedPayload {
    pub fn applicability(&self) -> PayloadApplicability {
        PayloadApplicability::Unvalidated
    }
    pub fn json_for_inspection(&self) -> &Value {
        &self.0
    }
}

/// Only constructed by the bounded raw decoder; fields exposed through shared borrows.
/// Serialization is for inspection/testing, not a publication approval or raw identity.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionV1 {
    schema_version: u32,
    workspace_id: CanonicalUuid,
    object_id: ObjectId,
    kind: RevisionKind,
    revision_id: CanonicalUuid,
    parents: Vec<RevisionRef>,
    device_id: CanonicalUuid,
    device_sequence: UnsignedDecimal,
    created_at: BoundedText,
    engine_contract: BoundedText,
    payload: UnvalidatedPayload,
    dependencies: Vec<BlobRef>,
    required_revisions: Vec<RevisionRef>,
}
impl RevisionV1 {
    pub fn workspace_id(&self) -> &CanonicalUuid {
        &self.workspace_id
    }
    pub fn object_id(&self) -> &ObjectId {
        &self.object_id
    }
    pub fn kind(&self) -> RevisionKind {
        self.kind
    }
    pub fn revision_id(&self) -> &CanonicalUuid {
        &self.revision_id
    }
    pub fn parents(&self) -> &[RevisionRef] {
        &self.parents
    }
    pub fn device_id(&self) -> &CanonicalUuid {
        &self.device_id
    }
    pub fn device_sequence(&self) -> UnsignedDecimal {
        self.device_sequence
    }
    pub fn created_at(&self) -> &BoundedText {
        &self.created_at
    }
    pub fn engine_contract(&self) -> &BoundedText {
        &self.engine_contract
    }
    pub fn payload(&self) -> &UnvalidatedPayload {
        &self.payload
    }
    pub fn dependencies(&self) -> &[BlobRef] {
        &self.dependencies
    }
    pub fn required_revisions(&self) -> &[RevisionRef] {
        &self.required_revisions
    }
}

// Private: direct serde decoding must not bypass the duplicate-key/depth boundary.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevisionWire {
    schema_version: u32,
    workspace_id: CanonicalUuid,
    object_id: ObjectId,
    kind: RevisionKind,
    revision_id: CanonicalUuid,
    parents: Vec<RevisionRef>,
    device_id: CanonicalUuid,
    device_sequence: UnsignedDecimal,
    created_at: BoundedText,
    engine_contract: BoundedText,
    payload: Value,
    dependencies: Vec<BlobRef>,
    required_revisions: Vec<RevisionRef>,
}

impl RevisionWire {
    fn validate(self) -> Result<RevisionV1, EnvelopeError> {
        if self.schema_version != 1 {
            return Err(EnvelopeError::UnsupportedVersion);
        }
        if self.kind != RevisionKind::Tombstone
            && object_kind(self.object_id.as_str()) != Some(self.kind)
        {
            return Err(EnvelopeError::ObjectKindMismatch);
        }
        if self.parents.len() > MAX_REVISION_REFS
            || self.required_revisions.len() > MAX_REVISION_REFS
        {
            return Err(EnvelopeError::ReferenceLimit);
        }
        if self.dependencies.len() > MAX_DEPENDENCIES {
            return Err(EnvelopeError::DependencyLimit);
        }
        let mut refs: HashMap<&CanonicalUuid, &Sha256Hex> = HashMap::new();
        for reference in self.parents.iter().chain(&self.required_revisions) {
            if reference.revision_id == self.revision_id {
                return Err(EnvelopeError::SelfReference);
            }
            if let Some(previous) = refs.insert(&reference.revision_id, &reference.sha256) {
                return Err(if previous == &reference.sha256 {
                    EnvelopeError::DuplicateRevisionRef
                } else {
                    EnvelopeError::ConflictingRevisionRef
                });
            }
        }
        let mut lengths: HashMap<&Sha256Hex, UnsignedDecimal> = HashMap::new();
        let mut roles = std::collections::HashSet::new();
        for blob in &self.dependencies {
            blob.validate_space(StorageSpace::AppDataFolder)?;
            if let Some(previous) = lengths.insert(&blob.sha256, blob.bytes)
                && previous != blob.bytes
            {
                return Err(EnvelopeError::ConflictingBlobLength);
            }
            // Equal bytes may legitimately serve two roles. Same hash + role is duplicate.
            if !roles.insert((&blob.sha256, blob.kind)) {
                return Err(EnvelopeError::DuplicateBlobRef);
            }
        }
        Ok(RevisionV1 {
            schema_version: self.schema_version,
            workspace_id: self.workspace_id,
            object_id: self.object_id,
            kind: self.kind,
            revision_id: self.revision_id,
            parents: self.parents,
            device_id: self.device_id,
            device_sequence: self.device_sequence,
            created_at: self.created_at,
            engine_contract: self.engine_contract,
            payload: UnvalidatedPayload(self.payload),
            dependencies: self.dependencies,
            required_revisions: self.required_revisions,
        })
    }
}

/// Immutable received bytes and envelope. The hash is of `raw_bytes`, not reserialization.
pub struct ReceivedRevision {
    raw_bytes: Box<[u8]>,
    raw_sha256: Sha256Hex,
    revision: RevisionV1,
}
impl ReceivedRevision {
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw_bytes
    }
    pub fn raw_sha256(&self) -> &Sha256Hex {
        &self.raw_sha256
    }
    pub fn revision(&self) -> &RevisionV1 {
        &self.revision
    }
}
impl fmt::Debug for ReceivedRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReceivedRevision")
            .field("raw_bytes_len", &self.raw_bytes.len())
            .field("applicability", &PayloadApplicability::Unvalidated)
            .finish_non_exhaustive()
    }
}

/// Checks syntax and envelope structure only. Payload schema, graph ancestry,
/// account ownership, dependency contents and render compatibility are unvalidated.
pub fn decode_revision(raw: &[u8], space: StorageSpace) -> Result<ReceivedRevision, EnvelopeError> {
    if raw.len() > MAX_JSON_BYTES {
        return Err(EnvelopeError::SizeLimit);
    }
    if space != StorageSpace::AppDataFolder {
        return Err(EnvelopeError::WrongSpace);
    }
    let mut fault = None;
    let mut deserializer = serde_json::Deserializer::from_slice(raw);
    let value = StrictValue {
        depth: 0,
        fault: &mut fault,
    }
    .deserialize(&mut deserializer)
    .map_err(|_| fault.take().unwrap_or(EnvelopeError::InvalidJson))?;
    deserializer.end().map_err(|_| EnvelopeError::InvalidJson)?;
    let wire: RevisionWire =
        serde_json::from_value(value).map_err(|_| EnvelopeError::InvalidEnvelope)?;
    let revision = wire.validate()?;
    Ok(ReceivedRevision {
        raw_sha256: Sha256Hex::of_bytes(raw),
        raw_bytes: raw.into(),
        revision,
    })
}

// A single bounded JSON pass retains decoded keys before serde_json::Value could
// discard duplicates. No raw_value feature or handwritten JSON tokenizer needed.
struct StrictValue<'a> {
    depth: usize,
    fault: &'a mut Option<EnvelopeError>,
}
impl StrictValue<'_> {
    fn container<E: de::Error>(&mut self) -> Result<(), E> {
        if self.depth >= MAX_JSON_DEPTH {
            *self.fault = Some(EnvelopeError::DepthLimit);
            Err(E::custom("JSON depth limit"))
        } else {
            Ok(())
        }
    }
}
impl<'de> DeserializeSeed<'de> for StrictValue<'_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for StrictValue<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON value")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }
    fn visit_seq<A: SeqAccess<'de>>(mut self, mut sequence: A) -> Result<Value, A::Error> {
        self.container::<A::Error>()?;
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(StrictValue {
            depth: self.depth + 1,
            fault: &mut *self.fault,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Value, A::Error> {
        self.container::<A::Error>()?;
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                *self.fault = Some(EnvelopeError::DuplicateJsonKey);
                return Err(de::Error::custom("duplicate JSON key"));
            }
            let value = map.next_value_seed(StrictValue {
                depth: self.depth + 1,
                fault: &mut *self.fault,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const WORKSPACE: &str = "11111111-1111-4111-8111-111111111111";
    const REVISION: &str = "22222222-2222-4222-8222-222222222222";
    const DEVICE: &str = "33333333-3333-4333-8333-333333333333";
    fn fixture() -> Value {
        json!({"schemaVersion":1,"workspaceId":WORKSPACE,"objectId":"settings","kind":"settings",
            "revisionId":REVISION,"parents":[],"deviceId":DEVICE,"deviceSequence":"9007199254740993",
            "createdAt":"2026-10-15T00:00:00Z","engineContract":"rapidraw.v1",
            "payload":{"theme":"dark"},"dependencies":[],"requiredRevisions":[]})
    }
    fn decode(value: &Value) -> Result<ReceivedRevision, EnvelopeError> {
        decode_revision(
            &serde_json::to_vec(value).unwrap(),
            StorageSpace::AppDataFolder,
        )
    }
    fn reference(n: u32) -> Value {
        json!({"revisionId":format!("00000000-0000-4000-8000-{n:012x}"),"sha256":"a".repeat(64)})
    }
    fn blob(n: u32, kind: &str) -> Value {
        json!({"sha256":format!("{n:064x}"),"bytes":"123","kind":kind})
    }

    #[test]
    fn round_trip_preserves_wire_keys_and_string_counts() {
        let input = fixture();
        let received = decode(&input).unwrap();
        assert_eq!(serde_json::to_value(received.revision()).unwrap(), input);
        assert_eq!(
            received.revision().device_sequence().value(),
            9_007_199_254_740_993
        );
        assert_eq!(
            received.revision().payload().applicability(),
            PayloadApplicability::Unvalidated
        );
        assert_eq!(
            serde_json::to_string(&UnsignedDecimal::new(u64::MAX)).unwrap(),
            "\"18446744073709551615\""
        );
        let mut v = fixture();
        v["dependencies"] = json!([blob(1, "profileDcp")]);
        assert_eq!(
            serde_json::to_value(decode(&v).unwrap().revision()).unwrap(),
            v
        );
    }

    #[test]
    fn raw_identity_is_not_reserialized_identity() {
        assert_eq!(
            Sha256Hex::of_bytes(b"abc").as_str(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let compact = serde_json::to_vec(&fixture()).unwrap();
        let pretty = serde_json::to_vec_pretty(&fixture()).unwrap();
        let a = decode_revision(&compact, StorageSpace::AppDataFolder).unwrap();
        let b = decode_revision(&pretty, StorageSpace::AppDataFolder).unwrap();
        assert_eq!(a.raw_bytes(), compact);
        assert_eq!(b.raw_bytes(), pretty);
        assert_ne!(a.raw_sha256(), b.raw_sha256());
        assert_eq!(
            serde_json::to_value(a.revision()).unwrap(),
            serde_json::to_value(b.revision()).unwrap()
        );
    }

    #[test]
    fn version_unknown_fields_and_unknown_enums_fail() {
        let mut v = fixture();
        v["schemaVersion"] = json!(2);
        assert_eq!(decode(&v).unwrap_err(), EnvelopeError::UnsupportedVersion);
        for (key, value) in [
            ("schemaVersion", json!("1")),
            ("schemaVersion", json!(-1)),
            ("kind", json!("future")),
            ("selfHash", json!("a".repeat(64))),
        ] {
            let mut v = fixture();
            v[key] = value;
            assert!(decode(&v).is_err(), "{key}");
        }
        assert!(serde_json::from_value::<StorageSpace>(json!("unknown")).is_err());
        let mut v = fixture();
        v["dependencies"] = json!([blob(1, "unknown")]);
        assert!(decode(&v).is_err());
        let mut v = fixture();
        let mut r = reference(1);
        r["path"] = json!("/private/example");
        v["parents"] = json!([r]);
        assert!(decode(&v).is_err());
        let mut v = fixture();
        let mut b = blob(1, "state");
        b["url"] = json!("https://example.invalid");
        v["dependencies"] = json!([b]);
        assert!(decode(&v).is_err());
        let mut v = fixture();
        v.as_object_mut().unwrap().remove("payload");
        assert!(decode(&v).is_err());
    }

    #[test]
    fn canonical_identifiers_and_decimal_strings_are_strict() {
        for text in [
            "",
            "0",
            &"A".repeat(64),
            &"g".repeat(64),
            &"a".repeat(63),
            &"a".repeat(65),
        ] {
            assert!(Sha256Hex::try_from(text.to_owned()).is_err());
        }
        for text in [
            "",
            "11111111111141118111111111111111",
            "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA",
            "urn:uuid:11111111-1111-4111-8111-111111111111",
        ] {
            assert!(CanonicalUuid::try_from(text.to_owned()).is_err());
            let mut v = fixture();
            v["workspaceId"] = json!(text);
            assert!(decode(&v).is_err());
        }
        for value in [
            json!(0),
            json!(null),
            json!(true),
            json!(""),
            json!("01"),
            json!("+1"),
            json!("-1"),
            json!("1.0"),
            json!("1e2"),
            json!(" 1"),
            json!("18446744073709551616"),
        ] {
            let mut v = fixture();
            v["deviceSequence"] = value;
            assert!(decode(&v).is_err());
        }
        for value in [
            json!(123),
            json!("-1"),
            json!("01"),
            json!("18446744073709551616"),
        ] {
            let mut v = fixture();
            let mut b = blob(1, "state");
            b["bytes"] = value;
            v["dependencies"] = json!([b]);
            assert!(decode(&v).is_err());
        }
        for text in ["0", "18446744073709551615"] {
            let mut v = fixture();
            v["deviceSequence"] = json!(text);
            assert!(decode(&v).is_ok());
        }
        let mut v = fixture();
        v["dependencies"] = json!([{"sha256":"A".repeat(64),"bytes":"1","kind":"state"}]);
        assert!(decode(&v).is_err());
        let mut v = fixture();
        v["parents"] = json!([{"revisionId":"bad","sha256":"a".repeat(64)}]);
        assert!(decode(&v).is_err());
    }

    #[test]
    fn object_kind_and_tombstone_target_are_checked() {
        let mut v = fixture();
        v["kind"] = json!("edit");
        assert_eq!(decode(&v).unwrap_err(), EnvelopeError::ObjectKindMismatch);
        v["objectId"] = json!(format!("edit:{WORKSPACE}:{DEVICE}"));
        assert!(decode(&v).is_ok());
        v["kind"] = json!("tombstone");
        assert!(decode(&v).is_ok());
        for id in [
            "/tmp/settings",
            "https://example.invalid",
            "tombstone:abc",
            "settings:extra",
        ] {
            v["objectId"] = json!(id);
            assert!(decode(&v).is_err());
        }
    }

    #[test]
    fn duplicate_keys_are_rejected_before_value_can_discard_them() {
        let text = serde_json::to_string(&fixture()).unwrap();
        let root = text.replacen("{", "{\"schemaVersion\":1,", 1);
        assert_eq!(
            decode_revision(root.as_bytes(), StorageSpace::AppDataFolder).unwrap_err(),
            EnvelopeError::DuplicateJsonKey
        );
        for replacement in [
            r#"{"theme":"dark","theme":"light"}"#,
            r#"{"a":1,"\u0061":2}"#,
            r#"[{"nested":{"x":1,"x":2}}]"#,
        ] {
            let nested = text.replace(r#"{"theme":"dark"}"#, replacement);
            assert_ne!(nested, text);
            assert_eq!(
                decode_revision(nested.as_bytes(), StorageSpace::AppDataFolder).unwrap_err(),
                EnvelopeError::DuplicateJsonKey
            );
        }
    }

    #[test]
    fn nonfinite_invalid_utf8_trailing_and_empty_json_fail() {
        let text = serde_json::to_string(&fixture()).unwrap();
        for literal in ["NaN", "Infinity", "-Infinity", "1e400"] {
            let raw = text.replace(r#"{"theme":"dark"}"#, literal);
            assert!(decode_revision(raw.as_bytes(), StorageSpace::AppDataFolder).is_err());
        }
        for raw in [b"".as_slice(), b"\xff".as_slice(), b"{} {}".as_slice()] {
            assert!(decode_revision(raw, StorageSpace::AppDataFolder).is_err());
        }
        assert!(
            decode_revision(
                format!("{text} null").as_bytes(),
                StorageSpace::AppDataFolder
            )
            .is_err()
        );
    }

    #[test]
    fn exact_depth_and_byte_limits_are_enforced() {
        let mut v = fixture();
        let mut payload = Value::Null;
        for _ in 0..31 {
            payload = json!([payload]);
        } // root + 31 arrays = 32
        v["payload"] = payload;
        assert!(decode(&v).is_ok());
        v["payload"] = json!([v["payload"].clone()]);
        assert_eq!(decode(&v).unwrap_err(), EnvelopeError::DepthLimit);
        let mut raw = serde_json::to_vec(&fixture()).unwrap();
        raw.resize(MAX_JSON_BYTES, b' ');
        assert!(decode_revision(&raw, StorageSpace::AppDataFolder).is_ok());
        raw.push(b' ');
        assert_eq!(
            decode_revision(&raw, StorageSpace::AppDataFolder).unwrap_err(),
            EnvelopeError::SizeLimit
        );
    }

    #[test]
    fn bounded_text_counts_unicode_scalars_without_normalizing() {
        let mut v = fixture();
        v["engineContract"] = json!("界".repeat(256));
        assert!(decode(&v).is_ok());
        v["engineContract"] = json!("界".repeat(257));
        assert!(decode(&v).is_err());
        for text in ["", " \t", "name\n", "x\u{0}"] {
            let mut v = fixture();
            v["createdAt"] = json!(text);
            assert!(decode(&v).is_err());
        }
    }

    #[test]
    fn reference_limits_duplicate_and_corruption_are_distinct() {
        for field in ["parents", "requiredRevisions"] {
            let mut v = fixture();
            v[field] = json!((1..=32).map(reference).collect::<Vec<_>>());
            assert!(decode(&v).is_ok());
            v[field] = json!((1..=33).map(reference).collect::<Vec<_>>());
            assert_eq!(decode(&v).unwrap_err(), EnvelopeError::ReferenceLimit);
            v[field] = json!([reference(1), reference(1)]);
            assert_eq!(decode(&v).unwrap_err(), EnvelopeError::DuplicateRevisionRef);
            let mut other = reference(1);
            other["sha256"] = json!("b".repeat(64));
            v[field] = json!([reference(1), other]);
            assert_eq!(
                decode(&v).unwrap_err(),
                EnvelopeError::ConflictingRevisionRef
            );
        }
        let mut full = fixture();
        full["parents"] = json!((1..=32).map(reference).collect::<Vec<_>>());
        full["requiredRevisions"] = json!((33..=64).map(reference).collect::<Vec<_>>());
        assert!(decode(&full).is_ok());
        let mut v = fixture();
        v["parents"] = json!([reference(1)]);
        v["requiredRevisions"] = json!([reference(1)]);
        assert_eq!(decode(&v).unwrap_err(), EnvelopeError::DuplicateRevisionRef);
        v["requiredRevisions"][0]["sha256"] = json!("b".repeat(64));
        assert_eq!(
            decode(&v).unwrap_err(),
            EnvelopeError::ConflictingRevisionRef
        );
        let mut v = fixture();
        v["parents"] = json!([{"revisionId":REVISION,"sha256":"a".repeat(64)}]);
        assert_eq!(decode(&v).unwrap_err(), EnvelopeError::SelfReference);
    }

    #[test]
    fn dependencies_are_bounded_with_hash_length_and_role_checks() {
        let mut v = fixture();
        v["dependencies"] = json!((0..256).map(|n| blob(n, "state")).collect::<Vec<_>>());
        assert!(decode(&v).is_ok());
        v["dependencies"] = json!((0..257).map(|n| blob(n, "state")).collect::<Vec<_>>());
        assert_eq!(decode(&v).unwrap_err(), EnvelopeError::DependencyLimit);
        v["dependencies"] = json!([blob(1, "mask"), blob(1, "mask")]);
        assert_eq!(decode(&v).unwrap_err(), EnvelopeError::DuplicateBlobRef);
        let mut other = blob(1, "patch");
        other["bytes"] = json!("124");
        v["dependencies"] = json!([blob(1, "mask"), other]);
        assert_eq!(
            decode(&v).unwrap_err(),
            EnvelopeError::ConflictingBlobLength
        );
        v["dependencies"] = json!([blob(1, "mask"), blob(1, "patch")]);
        assert!(decode(&v).is_ok());
    }

    #[test]
    fn space_rules_are_exhaustive_and_revision_dependencies_are_internal() {
        for kind in [
            BlobKind::ProfileDcp,
            BlobKind::ProfileLook,
            BlobKind::Lut,
            BlobKind::Patch,
            BlobKind::Mask,
            BlobKind::Watermark,
            BlobKind::State,
            BlobKind::OriginalPhoto,
            BlobKind::RenderedExport,
        ] {
            let b = BlobRef {
                sha256: Sha256Hex::of_bytes(b"test"),
                bytes: UnsignedDecimal::new(4),
                kind,
            };
            let expected = if matches!(kind, BlobKind::OriginalPhoto | BlobKind::RenderedExport) {
                StorageSpace::Drive
            } else {
                StorageSpace::AppDataFolder
            };
            assert_eq!(b.validate_space(expected), Ok(()));
            let wrong = if expected == StorageSpace::Drive {
                StorageSpace::AppDataFolder
            } else {
                StorageSpace::Drive
            };
            assert_eq!(b.validate_space(wrong), Err(EnvelopeError::WrongSpace));
        }
        assert_eq!(
            decode_revision(
                &serde_json::to_vec(&fixture()).unwrap(),
                StorageSpace::Drive
            )
            .unwrap_err(),
            EnvelopeError::WrongSpace
        );
        for kind in ["originalPhoto", "renderedExport"] {
            let mut v = fixture();
            v["dependencies"] = json!([blob(1, kind)]);
            assert_eq!(decode(&v).unwrap_err(), EnvelopeError::WrongSpace);
        }
    }

    #[test]
    fn opaque_malformed_payloads_never_acquire_applicability() {
        for payload in [
            Value::Null,
            json!(42),
            json!([]),
            json!({"profile":{"amount":"wrong"},"path":"/private/local","token":"synthetic"}),
        ] {
            let mut v = fixture();
            v["payload"] = payload.clone();
            let received = decode(&v).unwrap();
            assert_eq!(
                received.revision().payload().json_for_inspection(),
                &payload
            );
            assert_eq!(
                received.revision().payload().applicability(),
                PayloadApplicability::Unvalidated
            );
            assert!(!format!("{received:?}").contains("synthetic"));
        }
    }

    #[test]
    fn account_key_is_only_syntax_and_uses_string_project_number() {
        let v = json!({"projectNumber":"123456","drivePermissionId":"opaque_123-ABC"});
        let key: AccountKey = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(serde_json::to_value(key).unwrap(), v);
        for v in [
            json!({"projectNumber":123456,"drivePermissionId":"id"}),
            json!({"projectNumber":"1","drivePermissionId":""}),
            json!({"projectNumber":"1","drivePermissionId":"https://example.invalid"}),
            json!({"projectNumber":"1","drivePermissionId":"id","token":"synthetic"}),
        ] {
            assert!(serde_json::from_value::<AccountKey>(v).is_err());
        }
    }
}
