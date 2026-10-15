//! Data model for Adobe DNG Camera Profiles (DCP).
//!
//! Normative shape from `COBALT_DCP_SPEC.md` §5.1. These types are produced by
//! `parser.rs` and consumed by the render chain (W2), the registry/pairing
//! logic (W5) and the UI (W6).

use std::path::PathBuf;

/// A 3x3 colour matrix. Stored row-major matching the file order: the DCP
/// `ColorMatrix`/`ForwardMatrix` tags carry 9 rationals laid out row-by-row
/// (`[r0c0, r0c1, r0c2, r1c0, ...]`).
pub type Mat3 = nalgebra::Matrix3<f32>;

/// Stable content-derived identity for a profile: the blake3 hash of the raw
/// file bytes. Two paths that point at identical bytes share one `ProfileId`.
/// Wire format is always lowercase 64-character hex. Legacy 32-byte JSON
/// arrays remain readable so existing registry indexes and sidecars migrate.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ProfileId(pub [u8; 32]);

impl ProfileId {
    pub fn to_hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl serde::Serialize for ProfileId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for ProfileId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdVisitor;
        impl<'de> serde::de::Visitor<'de> for IdVisitor {
            type Value = ProfileId;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(
                    "exactly 64 ASCII hex characters or a legacy array of exactly 32 u8 integers",
                )
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<ProfileId, E> {
                if value.len() != 64 {
                    return Err(E::custom(
                        "profile ID must contain exactly 64 hex characters",
                    ));
                }
                let nibble = |b: u8| match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    b'A'..=b'F' => Some(b - b'A' + 10),
                    _ => None,
                };
                let mut bytes = [0u8; 32];
                for (out, pair) in bytes.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
                    let hi = nibble(pair[0])
                        .ok_or_else(|| E::custom("profile ID contains non-hex characters"))?;
                    let lo = nibble(pair[1])
                        .ok_or_else(|| E::custom("profile ID contains non-hex characters"))?;
                    *out = (hi << 4) | lo;
                }
                Ok(ProfileId(bytes))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<ProfileId, A::Error> {
                use serde::de::Error;
                let mut bytes = [0u8; 32];
                for (i, out) in bytes.iter_mut().enumerate() {
                    *out = seq
                        .next_element::<u8>()?
                        .ok_or_else(|| A::Error::invalid_length(i, &self))?;
                }
                if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    return Err(A::Error::invalid_length(33, &self));
                }
                Ok(ProfileId(bytes))
            }
        }
        deserializer.deserialize_any(IdVisitor)
    }
}

/// The illuminant a profile's calibration matrices were computed for.
///
/// `CalibrationIlluminant1`/`2` map to these. The correlated colour
/// temperatures follow `COBALT_DCP_SPEC.md` §4.2.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Illuminant {
    Daylight,
    Fluorescent,
    Tungsten,
    Flash,
    StdA,
    StdB,
    StdC,
    D55,
    D65,
    D75,
    D50,
    IsoStudioTungsten,
    Unknown(u16),
}

impl Illuminant {
    pub fn from_code(code: u16) -> Self {
        match code {
            1 => Self::Daylight,
            2 => Self::Fluorescent,
            3 => Self::Tungsten,
            4 => Self::Flash,
            10 => Self::Flash,
            17 => Self::StdA,
            18 => Self::StdB,
            19 => Self::StdC,
            20 => Self::D55,
            21 => Self::D65,
            22 => Self::D75,
            23 => Self::D50,
            24 => Self::IsoStudioTungsten,
            other => Self::Unknown(other),
        }
    }

    pub fn code(&self) -> u16 {
        match self {
            Self::Daylight => 1,
            Self::Fluorescent => 2,
            Self::Tungsten => 3,
            Self::Flash => 4,
            Self::StdA => 17,
            Self::StdB => 18,
            Self::StdC => 19,
            Self::D55 => 20,
            Self::D65 => 21,
            Self::D75 => 22,
            Self::D50 => 23,
            Self::IsoStudioTungsten => 24,
            Self::Unknown(code) => *code,
        }
    }

    /// Correlated colour temperature in Kelvin per §4.2. Returns `None` for
    /// codes outside the supported set.
    pub fn correlated_temp(&self) -> Option<f64> {
        match self {
            Self::Daylight => Some(5500.0),
            Self::Fluorescent => Some(4200.0),
            Self::Tungsten => Some(2856.0),
            Self::Flash => Some(5500.0),
            Self::StdA => Some(2856.0),
            Self::StdB => Some(4874.0),
            Self::StdC => Some(6774.0),
            Self::D55 => Some(5500.0),
            Self::D65 => Some(6504.0),
            Self::D75 => Some(7504.0),
            Self::D50 => Some(5003.0),
            Self::IsoStudioTungsten => Some(3200.0),
            Self::Unknown(_) => None,
        }
    }
}

/// The colour-space encoding a rendering table's RGB values are stored in.
///
/// `ProfileHueSatMapEncoding` and `ProfileLookTableEncoding`. Per §1.3 the
/// HueSatMap defaults to `Linear` when the tag is absent, while the LookTable
/// here is `Srgb` — the two tables in one pipeline use different encodings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TableEncoding {
    Linear,
    Srgb,
}

impl TableEncoding {
    pub fn from_code(code: u32) -> Self {
        match code {
            0 => Self::Linear,
            1 => Self::Srgb,
            _ => Self::Linear,
        }
    }
}

/// `ProfileEmbedPolicy` (tag 50941). `EmbedNever` must be honoured by any
/// DNG/TIFF export path (§8.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EmbedPolicy {
    AllowCopying,
    EmbedIfUsed,
    EmbedNever,
    NoRestrictions,
    Unknown(u32),
}

impl EmbedPolicy {
    pub fn from_code(code: u32) -> Self {
        match code {
            0 => Self::AllowCopying,
            1 => Self::EmbedIfUsed,
            2 => Self::EmbedNever,
            3 => Self::NoRestrictions,
            other => Self::Unknown(other),
        }
    }
}

/// `DefaultBlackRender` (tag 50710). `Auto` is the DNG default when the tag is
/// absent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefaultBlackRender {
    Auto,
    None,
}

impl DefaultBlackRender {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::None,
            _ => Self::Auto,
        }
    }
}

/// A single rendering table (HueSatMap or LookTable) with its divisions and
/// data. `data` holds one `[hue_shift, sat_scale, val_scale]` triplet per
/// `hue_div * sat_div * val_div` cell.
#[derive(Clone, PartialEq, Debug)]
pub struct HsvTable {
    pub hue_div: u32,
    pub sat_div: u32,
    pub val_div: u32,
    pub data: Vec<[f32; 3]>,
}

impl HsvTable {
    pub fn cell_count(&self) -> u64 {
        self.hue_div as u64 * self.sat_div as u64 * self.val_div as u64
    }
}

/// A dual-illuminant HueSatMap: the interpolated map for illuminant 1 plus an
/// optional second map for illuminant 2.
#[derive(Clone, PartialEq, Debug)]
pub struct DualHueSatMap {
    pub map_1: HsvTable,
    pub map_2: Option<HsvTable>,
}

/// The profile tone curve: a list of `(x, y)` points in [0, 1] domain.
#[derive(Clone, PartialEq, Debug)]
pub struct ToneCurve {
    pub points: Vec<[f32; 2]>,
}

/// A fully parsed Adobe DNG Camera Profile.
///
/// Required tags (a usable profile cannot exist without them) are plain
/// fields; everything else is `Option` with a documented default.
#[derive(Clone, PartialEq, Debug)]
pub struct DcpProfile {
    pub id: ProfileId,
    pub file_path: PathBuf,
    pub profile_name: String,
    pub unique_camera_model: String,
    pub copyright: Option<String>,
    pub embed_policy: EmbedPolicy,
    pub calibration_illuminant_1: Illuminant,
    pub calibration_illuminant_2: Option<Illuminant>,
    pub color_matrix_1: Mat3,
    pub color_matrix_2: Option<Mat3>,
    pub forward_matrix_1: Option<Mat3>,
    pub forward_matrix_2: Option<Mat3>,
    pub camera_calibration_1: Option<Mat3>,
    pub camera_calibration_2: Option<Mat3>,
    pub analog_balance: Option<[f32; 3]>,
    pub baseline_exposure_offset: Option<f32>,
    pub default_black_render: DefaultBlackRender,
    pub hue_sat_map: Option<DualHueSatMap>,
    pub look_table: Option<HsvTable>,
    pub look_table_encoding: TableEncoding,
    pub hue_sat_map_encoding: TableEncoding,
    pub tone_curve: Option<ToneCurve>,
}

#[cfg(test)]
mod profile_id_wire_tests {
    use super::ProfileId;
    use crate::dcp::registry::{PairingState, ProfileEntry, ProfileSource};
    use serde_json::json;

    #[test]
    fn profile_id_hex_and_legacy_round_trip_to_canonical_hex() {
        let id = ProfileId(std::array::from_fn(|i| (i * 7 + 3) as u8));
        let canonical = json!(id.to_hex());
        assert_eq!(serde_json::to_value(id).unwrap(), canonical);
        for value in [
            canonical.clone(),
            json!(id.to_hex().to_uppercase()),
            json!(id.0),
        ] {
            let parsed: ProfileId = serde_json::from_value(value).unwrap();
            assert_eq!(parsed, id);
            assert_eq!(serde_json::to_value(parsed).unwrap(), canonical);
        }
    }

    #[test]
    fn profile_id_rejects_malformed_wire_values() {
        let mut bad = vec![
            json!(null),
            json!(42),
            json!(true),
            json!({}),
            json!(""),
            json!("a".repeat(63)),
            json!("a".repeat(65)),
            json!("z".repeat(64)),
            json!("é".repeat(32)),
            json!(format!(" {}", "a".repeat(64))),
            json!(vec![0u8; 31]),
            json!(vec![0u8; 33]),
        ];
        for value in [
            json!(-1),
            json!(256),
            json!(1.5),
            json!(1.0),
            json!("1"),
            json!(null),
        ] {
            let mut array = vec![json!(0); 32];
            array[7] = value;
            bad.push(json!(array));
        }
        for value in bad {
            assert!(
                serde_json::from_value::<ProfileId>(value.clone()).is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn actual_registry_entry_and_pairing_ipc_emit_hex_strings() {
        let id = ProfileId([74; 32]);
        let entry = ProfileEntry {
            id,
            file_path: "synthetic.dcp".into(),
            profile_name: "Synthetic".into(),
            unique_camera_model: "Synthetic Camera".into(),
            source: ProfileSource::Managed,
            size: 0,
            mtime_ms: 0,
        };
        let mut encoded = serde_json::to_value(&entry).unwrap();
        assert_eq!(encoded["id"], json!(id.to_hex()));
        // The actual pairing type is PairingState, not a stand-in test shape.
        assert_eq!(
            serde_json::to_value(PairingState::Satisfied { base: id }).unwrap(),
            json!({"state":"satisfied","base":id.to_hex()})
        );
        encoded["id"] = json!(id.0);
        let migrated: ProfileEntry = serde_json::from_value(encoded).unwrap();
        assert_eq!(migrated, entry);
        assert_eq!(
            serde_json::to_value(migrated).unwrap()["id"],
            json!(id.to_hex())
        );
    }
}
