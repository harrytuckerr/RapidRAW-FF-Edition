//! Native evidence for a possible Modular default. No load/UI activation.
//!
//! The caller must supply classification and camera metadata from an actual RAW
//! decode, never EXIF, an extension, display rank or a restored null selection.
//! This synchronous helper reads installed assets and prepares existing CPU
//! evaluators; a future caller must run it off the UI thread. An eligible result
//! is evidence for that snapshot, not authority to overwrite edits. M2/M3 must
//! still enforce missing-sidecar, source-generation and untouched-edit guards.
use super::{
    look_xmp::{CobaltLook, parse_look_xmp_bytes},
    model::ProfileId,
    parser::parse_bytes,
    pipeline::{matches_camera, neutral_from_wb, validate_pairing},
    registry::{DefaultProfileSnapshot, ProfileRegistry},
    render::DcpRenderer,
    rgb_look::RgbLook,
};
use crate::raw_processing::RawImageMeta;
use std::fs::File;
use std::io::Read;
use std::path::Path;

// Same DCP file limit as the existing disk parser, also bounding XMP input here.
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;
const MODULAR: &str = "Cobalt Modular";

/// Trusted native input boundary; this enum does not itself decode/authenticate
/// a source. There is deliberately no serde/IPC representation or EXIF adapter.
pub enum RawDefaultFacts<'a> {
    CameraNative(&'a RawImageMeta),
    LinearRaw,
    Rendered,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultSkip {
    NotCameraNative,
    UnknownCamera,
    InvalidWhiteBalance,
    CatalogUnavailable,
    CatalogChanged,
    MissingModular,
    AmbiguousModular,
    AssetUnavailable,
    AssetTooLarge,
    StaleAsset,
    InvalidBase,
    MultipleProfiles,
    UnsupportedBase,
    NoCompatibleLook,
}

/// A witness is not a selected Look. Later application selects only `base`,
/// leaves Look null and uses the fresh default amount 100.
#[derive(Debug, PartialEq, Eq)]
pub struct ModularEvidence {
    pub base: ProfileId,
    pub look_witness_uuid: String,
    look_witness_content_id: String,
    look_witness_bytes: u64,
    /// Ephemeral catalog metadata digest, not an asset/RAW identity or revision.
    pub catalog_fingerprint: [u8; 32],
}

impl ModularEvidence {
    /// Full BLAKE3 digest of the exact owned Look bytes parsed for this witness.
    /// This is snapshot evidence, not a guarantee the installed file is unchanged.
    pub fn look_witness_content_id(&self) -> &str {
        &self.look_witness_content_id
    }

    /// Length of those same bounded owned bytes, independent of the Look UUID.
    pub fn look_witness_bytes(&self) -> u64 {
        self.look_witness_bytes
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ModularDecision {
    Eligible(ModularEvidence),
    Skip(DefaultSkip),
}

fn catalog_fingerprint(snapshot: &DefaultProfileSnapshot) -> Option<[u8; 32]> {
    let bytes = serde_json::to_vec(&(&snapshot.profiles, &snapshot.looks)).ok()?;
    Some(*blake3::hash(&bytes).as_bytes())
}

fn read_asset(path: &Path) -> Result<Vec<u8>, DefaultSkip> {
    let mut file = File::open(path).map_err(|_| DefaultSkip::AssetUnavailable)?;
    let metadata = file.metadata().map_err(|_| DefaultSkip::AssetUnavailable)?;
    if !metadata.is_file() {
        return Err(DefaultSkip::AssetUnavailable);
    }
    if metadata.len() > MAX_ASSET_BYTES {
        return Err(DefaultSkip::AssetTooLarge);
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_ASSET_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| DefaultSkip::AssetUnavailable)?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(DefaultSkip::AssetTooLarge);
    }
    Ok(bytes)
}

// The existing parser intentionally skips unreadable nested profiles. After
// parsing the primary, reject the *presence* of ExtraCameraProfiles as well as
// multiple returned profiles, so that skipped nested data cannot certify a
// single-profile default. This reads only the bounded primary tag directory;
// it does not interpret tags, tables, profile names or nested offsets.
fn declares_extra_profiles(bytes: &[u8]) -> Result<bool, DefaultSkip> {
    let header = bytes.get(4..8).ok_or(DefaultSkip::InvalidBase)?;
    let offset =
        u32::from_le_bytes(header.try_into().map_err(|_| DefaultSkip::InvalidBase)?) as usize;
    let count_bytes = bytes
        .get(offset..offset.checked_add(2).ok_or(DefaultSkip::InvalidBase)?)
        .ok_or(DefaultSkip::InvalidBase)?;
    let count = u16::from_le_bytes(
        count_bytes
            .try_into()
            .map_err(|_| DefaultSkip::InvalidBase)?,
    ) as usize;
    let start = offset.checked_add(2).ok_or(DefaultSkip::InvalidBase)?;
    let end = start
        .checked_add(count.checked_mul(12).ok_or(DefaultSkip::InvalidBase)?)
        .ok_or(DefaultSkip::InvalidBase)?;
    let directory = bytes.get(start..end).ok_or(DefaultSkip::InvalidBase)?;
    Ok(directory
        .as_chunks::<12>()
        .0
        .iter()
        .any(|entry| u16::from_le_bytes([entry[0], entry[1]]) == 50933))
}

fn supported_look(look: &CobaltLook) -> bool {
    // Use precisely the renderer's acceptance boundary, including grid/amount
    // validation. Captured and unavailable tables are rejected by this API.
    RgbLook::new(&look.table, 100.0, look.supports_amount).is_ok()
}

/// Inspect current catalog assets once. Filesystem failures or stale candidate
/// base IDs never cause a fallback to another base. No file or catalog is changed.
/// The Look's content ID is computed from the same owned bytes that are parsed;
/// the legacy catalog stores only its metadata, so this is a fresh witness,
/// not proof of equality with previously imported Look bytes.
pub fn decide_modular_default(
    facts: RawDefaultFacts<'_>,
    registry: &ProfileRegistry,
) -> ModularDecision {
    match decide(facts, registry) {
        Ok(evidence) => ModularDecision::Eligible(evidence),
        Err(reason) => ModularDecision::Skip(reason),
    }
}

fn decide(
    facts: RawDefaultFacts<'_>,
    registry: &ProfileRegistry,
) -> Result<ModularEvidence, DefaultSkip> {
    let meta = match facts {
        RawDefaultFacts::CameraNative(meta) => meta,
        RawDefaultFacts::LinearRaw | RawDefaultFacts::Rendered | RawDefaultFacts::Unknown => {
            return Err(DefaultSkip::NotCameraNative);
        }
    };
    if meta.model.trim().is_empty() && meta.clean_model.trim().is_empty() {
        return Err(DefaultSkip::UnknownCamera);
    }
    let neutral = neutral_from_wb(meta.wb_coeffs).map_err(|_| DefaultSkip::InvalidWhiteBalance)?;
    let snapshot = registry
        .default_snapshot()
        .ok_or(DefaultSkip::CatalogUnavailable)?;
    let fingerprint = catalog_fingerprint(&snapshot).ok_or(DefaultSkip::CatalogUnavailable)?;
    let mut selected = None;
    for entry in &snapshot.profiles {
        if entry.profile_name != MODULAR
            || !matches_camera(
                &entry.unique_camera_model,
                &meta.make,
                &meta.model,
                &meta.clean_model,
            )
        {
            continue;
        }
        let bytes = read_asset(&entry.file_path)?;
        let id = ProfileId(*blake3::hash(&bytes).as_bytes());
        if id != entry.id {
            return Err(DefaultSkip::StaleAsset);
        }
        // No path: missing ProfileName stays empty, never a filename fallback.
        let mut profiles = parse_bytes(&bytes, id, None).map_err(|_| DefaultSkip::InvalidBase)?;
        if profiles.len() != 1 || declares_extra_profiles(&bytes)? {
            // Parsed nested profiles share a whole-file ID; the renderer picks
            // the first ID. Do not make an ambiguous automatic selection.
            return Err(DefaultSkip::MultipleProfiles);
        }
        let profile = profiles.remove(0);
        if profile.profile_name != MODULAR
            || profile.unique_camera_model != entry.unique_camera_model
            || !matches_camera(
                &profile.unique_camera_model,
                &meta.make,
                &meta.model,
                &meta.clean_model,
            )
        {
            return Err(DefaultSkip::StaleAsset);
        }
        if DcpRenderer::new(&profile, neutral).is_err() {
            return Err(DefaultSkip::UnsupportedBase);
        }
        if selected
            .as_ref()
            .is_some_and(|p: &super::model::DcpProfile| p.id != profile.id)
        {
            return Err(DefaultSkip::AmbiguousModular);
        }
        selected = Some(profile);
    }
    let profile = selected.ok_or(DefaultSkip::MissingModular)?;
    let mut witness = None;
    for entry in &snapshot.looks {
        if entry.required_base_profile != MODULAR {
            continue;
        }
        if validate_pairing(
            &profile,
            &entry.required_base_profile,
            entry.camera_model_restriction.as_deref(),
            &meta.make,
            &meta.model,
            &meta.clean_model,
        )
        .is_err()
        {
            continue;
        }
        let bytes = match read_asset(&entry.file_path) {
            Ok(bytes) => bytes,
            Err(_) => continue, // This Look is not a usable witness.
        };
        let id = blake3::hash(&bytes).to_hex().to_string();
        let look = match parse_look_xmp_bytes(&bytes, entry.file_path.clone(), id) {
            Ok(look) => look,
            Err(_) => continue,
        };
        if look.uuid != entry.uuid
            || look.required_base_profile != entry.required_base_profile
            || look.camera_model_restriction != entry.camera_model_restriction
            || validate_pairing(
                &profile,
                &look.required_base_profile,
                look.camera_model_restriction.as_deref(),
                &meta.make,
                &meta.model,
                &meta.clean_model,
            )
            .is_err()
            || !supported_look(&look)
        {
            continue;
        }
        witness = Some((look.uuid, look.id, bytes.len() as u64));
        break;
    }
    let (look_witness_uuid, look_witness_content_id, look_witness_bytes) =
        witness.ok_or(DefaultSkip::NoCompatibleLook)?;
    let current = registry
        .default_snapshot()
        .ok_or(DefaultSkip::CatalogUnavailable)?;
    if catalog_fingerprint(&current) != Some(fingerprint) {
        return Err(DefaultSkip::CatalogChanged);
    }
    Ok(ModularEvidence {
        base: profile.id,
        look_witness_uuid,
        look_witness_content_id,
        look_witness_bytes,
        catalog_fingerprint: fingerprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcp::look_xmp::LookTableSource;
    use crate::dcp::registry::{LookMetadata, ProfileEntry, ProfileSource};
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn raw(model: &str) -> RawImageMeta {
        RawImageMeta {
            make: "FUJIFILM".into(),
            model: model.into(),
            clean_model: model.into(),
            wb_coeffs: [1.0, 1.0, 1.0, f32::NAN], // RGBE: E is unused.
        }
    }

    // Standalone synthetic IIRC, with required tags and optional real name.
    fn dcp(camera: &str, name: Option<&str>, extra_profile: bool) -> Vec<u8> {
        let ascii = |s: &str| [s.as_bytes(), &[0]].concat();
        let matrix: Vec<u8> = [1_i32, 0, 0, 0, 1, 0, 0, 0, 1]
            .iter()
            .flat_map(|n| [n.to_le_bytes(), 1_i32.to_le_bytes()].concat())
            .collect();
        let mut tags = vec![
            (50708_u16, 2_u16, ascii(camera)),
            (50721, 10, matrix),
            (50778, 3, 21_u16.to_le_bytes().to_vec()),
        ];
        if let Some(name) = name {
            tags.push((50936, 2, ascii(name)));
        }
        if extra_profile {
            // Both parsed profiles share the file's ID, matching parser behavior.
            tags.push((50933, 4, 8_u32.to_le_bytes().to_vec()));
        }
        tags.sort_by_key(|t| t.0);
        let data_start = 8 + 2 + tags.len() * 12 + 4;
        let mut bytes = vec![b'I', b'I', 0x52, 0x43, 8, 0, 0, 0];
        bytes.extend_from_slice(&(tags.len() as u16).to_le_bytes());
        bytes.resize(data_start, 0);
        for (index, (tag, kind, data)) in tags.iter().enumerate() {
            let offset = 10 + index * 12;
            bytes[offset..offset + 2].copy_from_slice(&tag.to_le_bytes());
            bytes[offset + 2..offset + 4].copy_from_slice(&kind.to_le_bytes());
            let size = match kind {
                2 => 1,
                3 => 2,
                4 => 4,
                10 => 8,
                _ => unreachable!(),
            };
            bytes[offset + 4..offset + 8]
                .copy_from_slice(&((data.len() / size) as u32).to_le_bytes());
            if data.len() <= 4 {
                bytes[offset + 8..offset + 8 + data.len()].copy_from_slice(data);
            } else {
                let at = bytes.len() as u32;
                bytes[offset + 8..offset + 12].copy_from_slice(&at.to_le_bytes());
                bytes.extend_from_slice(data);
            }
        }
        bytes
    }

    fn encoded_table(primaries: u32, max_amount: f64) -> String {
        let mut bytes = Vec::new();
        for value in [1_u32, 1, 3, 2] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 8 * 3 * 2]); // zero deltas: identity ramp
        for value in [primaries, 2, 0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0.0_f64.to_le_bytes());
        bytes.extend_from_slice(&max_amount.to_le_bytes());
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&bytes).unwrap();
        let mut block = (bytes.len() as u32).to_le_bytes().to_vec();
        block.extend_from_slice(&encoder.finish().unwrap());
        // Independent fixed Adobe alphabet; no private parser fixture access.
        let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?`'|()[]{}@%$#";
        let mut text = String::new();
        for chunk in block.chunks(4) {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            let mut value = u32::from_le_bytes(word) as u64;
            for _ in 0..chunk.len() + 1 {
                text.push(alphabet[(value % 85) as usize] as char);
                value /= 85;
            }
        }
        text
    }

    fn xmp(uuid: &str, required: &str, restriction: &str, table: Option<&str>) -> Vec<u8> {
        let table_attribute = table
            .map(|t| format!("crs:Table_TABLE=\"{t}\" "))
            .unwrap_or_default();
        format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF>
<rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
crs:PresetType="Look" crs:Cluster="Cobalt-Image" crs:UUID="{uuid}"
crs:SupportsAmount="True" crs:CameraModelRestriction="{restriction}"
crs:CameraProfile="{required}" crs:RGBTable="TABLE" crs:ProcessVersion="15.4" {table_attribute}>
<crs:Name><rdf:Alt><rdf:li>Fictional synthetic Look</rdf:li></rdf:Alt></crs:Name>
<crs:Group><rdf:Alt><rdf:li>Test only</rdf:li></rdf:Alt></crs:Group>
</rdf:Description></rdf:RDF></x:xmpmeta>"#
        )
        .into_bytes()
    }

    struct Assets {
        directory: TempDir,
        registry: ProfileRegistry,
    }
    impl Assets {
        fn new() -> Self {
            Self {
                directory: tempfile::tempdir().unwrap(),
                registry: ProfileRegistry::new(),
            }
        }
        fn base(&self, filename: &str, bytes: &[u8]) -> ProfileId {
            let path = self.directory.path().join(filename);
            std::fs::write(&path, bytes).unwrap();
            // Use the existing disk parser exactly as actual registry ingestion does.
            let profile = crate::dcp::parser::parse_dcp(&path).unwrap().remove(0);
            self.registry.insert_dcp(ProfileEntry {
                id: profile.id,
                file_path: path,
                profile_name: profile.profile_name,
                unique_camera_model: profile.unique_camera_model,
                source: ProfileSource::Managed,
                size: bytes.len() as u64,
                mtime_ms: 0,
            });
            profile.id
        }
        fn look(&self, filename: &str, bytes: &[u8]) -> PathBuf {
            let path = self.directory.path().join(filename);
            std::fs::write(&path, bytes).unwrap();
            let look = crate::dcp::look_xmp::parse_look_xmp(&path).unwrap();
            self.registry.register_look(LookMetadata {
                uuid: look.uuid,
                name: look.name,
                group: look.group,
                required_base_profile: look.required_base_profile,
                camera_model_restriction: look.camera_model_restriction,
                file_path: path.clone(),
            });
            path
        }
        fn supported(&self) -> PathBuf {
            self.look(
                "look.xmp",
                &xmp("synthetic-uuid", MODULAR, "", Some(&encoded_table(2, 1.5))),
            )
        }
        fn decision(&self, camera: &str) -> ModularDecision {
            decide_modular_default(RawDefaultFacts::CameraNative(&raw(camera)), &self.registry)
        }
    }

    #[test]
    fn decoded_two_camera_identity_and_existing_aliases_choose_their_own_base() {
        let assets = Assets::new();
        let xpro = assets.base(
            "unrelated-name.dcp",
            &dcp("Fuji X-Pro2", Some(MODULAR), false),
        );
        let xt = assets.base(
            "also-unrelated.dcp",
            &dcp("FUJIFILM X-T5", Some(MODULAR), false),
        );
        let path = assets.supported();
        for (camera, expected) in [("X-Pro2", xpro), ("X-T5", xt)] {
            let ModularDecision::Eligible(evidence) = assets.decision(camera) else {
                panic!("expected eligible synthetic base")
            };
            assert_eq!(evidence.base, expected);
            assert_eq!(evidence.look_witness_uuid, "synthetic-uuid");
            assert_eq!(
                evidence.look_witness_content_id(),
                blake3::hash(&std::fs::read(&path).unwrap())
                    .to_hex()
                    .to_string()
            );
            assert_eq!(
                evidence.look_witness_bytes(),
                std::fs::read(&path).unwrap().len() as u64
            );
        }
        assert_eq!(
            assets.decision("X-T4"),
            ModularDecision::Skip(DefaultSkip::MissingModular)
        );
    }

    #[test]
    fn duplicate_identical_bytes_are_one_id_but_distinct_contents_are_ambiguous() {
        let assets = Assets::new();
        let bytes = dcp("Fujifilm X-Pro2", Some(MODULAR), false);
        let first = assets.base("a.dcp", &bytes);
        assert_eq!(first, assets.base("b.dcp", &bytes));
        assets.supported();
        assert!(
            matches!(assets.decision("X-Pro2"), ModularDecision::Eligible(e) if e.base == first)
        );
        let mut different = bytes;
        different.push(7); // Different content ID, same valid parsed profile.
        assert_ne!(first, assets.base("c.dcp", &different));
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::AmbiguousModular)
        );
    }

    #[test]
    fn flat_missing_and_filename_only_modular_never_supply_a_default() {
        let assets = Assets::new();
        assets.supported();
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::MissingModular)
        );
        assets.base(
            "Cobalt Modular.dcp",
            &dcp("Fujifilm X-Pro2", Some("Cobalt Flat"), false),
        );
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::MissingModular)
        );
        let absent = Assets::new();
        absent.base("Cobalt Modular.dcp", &dcp("Fujifilm X-Pro2", None, false));
        absent.supported();
        assert_eq!(
            absent.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::StaleAsset)
        );
    }

    #[test]
    fn source_type_unknown_camera_and_invalid_rgb_wb_fail_before_eligibility() {
        let assets = Assets::new();
        assets.base("base.dcp", &dcp("Fujifilm X-Pro2", Some(MODULAR), false));
        assets.supported();
        for facts in [
            RawDefaultFacts::Rendered,
            RawDefaultFacts::LinearRaw,
            RawDefaultFacts::Unknown,
        ] {
            assert_eq!(
                decide_modular_default(facts, &assets.registry),
                ModularDecision::Skip(DefaultSkip::NotCameraNative)
            );
        }
        assert_eq!(
            assets.decision(""),
            ModularDecision::Skip(DefaultSkip::UnknownCamera)
        );
        for value in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut meta = raw("X-Pro2");
            meta.wb_coeffs[0] = value;
            assert_eq!(
                decide_modular_default(RawDefaultFacts::CameraNative(&meta), &assets.registry),
                ModularDecision::Skip(DefaultSkip::InvalidWhiteBalance)
            );
        }
    }

    #[test]
    fn corrupted_or_changed_base_bytes_do_not_fall_back() {
        let assets = Assets::new();
        let id = assets.base("base.dcp", &dcp("Fujifilm X-Pro2", Some(MODULAR), false));
        assets.supported();
        let entry = assets.registry.get(id).unwrap();
        std::fs::write(&entry.file_path, b"not a DCP").unwrap();
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::StaleAsset)
        );
        let mut entry = entry;
        entry.id = ProfileId(*blake3::hash(b"not a DCP").as_bytes());
        assets.registry.remove_dcp(id);
        assets.registry.insert_dcp(entry);
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::InvalidBase)
        );
    }

    #[test]
    fn wrong_registered_content_id_and_camera_metadata_are_stale() {
        let assets = Assets::new();
        let id = assets.base("base.dcp", &dcp("Fujifilm X-Pro2", Some(MODULAR), false));
        assets.supported();
        let mut entry = assets.registry.get(id).unwrap();
        assets.registry.remove_dcp(id);
        entry.id = ProfileId([0xff; 32]);
        assets.registry.insert_dcp(entry.clone());
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::StaleAsset)
        );
        assets.registry.remove_dcp(entry.id);
        entry.id = id;
        entry.unique_camera_model = "FUJI X-Pro2".into(); // Alias matches, actual stored metadata differs.
        assets.registry.insert_dcp(entry);
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::StaleAsset)
        );
    }

    #[test]
    fn parsed_multiple_profiles_do_not_choose_the_renderers_first_id() {
        let assets = Assets::new();
        assets.base("multi.dcp", &dcp("Fujifilm X-Pro2", Some(MODULAR), true));
        assets.supported();
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::MultipleProfiles)
        );
    }

    #[test]
    fn unreadable_nested_profile_is_not_mistaken_for_a_single_profile() {
        let assets = Assets::new();
        let mut bytes = dcp("Fujifilm X-Pro2", Some(MODULAR), true);
        let count = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
        for index in 0..count {
            let at = 10 + 12 * index;
            if u16::from_le_bytes([bytes[at], bytes[at + 1]]) == 50933 {
                bytes[at + 8..at + 12].copy_from_slice(&u32::MAX.to_le_bytes());
            }
        }
        // The production parser keeps the primary and skips the invalid child.
        assert_eq!(
            parse_bytes(&bytes, ProfileId(*blake3::hash(&bytes).as_bytes()), None)
                .unwrap()
                .len(),
            1
        );
        assets.base("invalid-nested.dcp", &bytes);
        assets.supported();
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::MultipleProfiles)
        );
    }

    #[test]
    fn missing_unavailable_malformed_and_unsupported_tables_are_not_witnesses() {
        let assets = Assets::new();
        assets.base("base.dcp", &dcp("Fujifilm X-Pro2", Some(MODULAR), false));
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::NoCompatibleLook)
        );
        for table in [
            None,
            Some("invalid".to_string()),
            Some(encoded_table(99, 1.5)),
            Some(encoded_table(2, 0.5)),
        ] {
            assets.look(
                "look.xmp",
                &xmp("synthetic-uuid", MODULAR, "", table.as_deref()),
            );
            assert_eq!(
                assets.decision("X-Pro2"),
                ModularDecision::Skip(DefaultSkip::NoCompatibleLook)
            );
        }
        assets.supported();
        assert!(matches!(
            assets.decision("X-Pro2"),
            ModularDecision::Eligible(_)
        ));
    }

    #[test]
    fn captured_table_is_rejected_by_the_actual_evaluator_boundary() {
        let bytes = xmp("synthetic-uuid", MODULAR, "", Some(&encoded_table(2, 1.5)));
        let mut look = parse_look_xmp_bytes(
            &bytes,
            PathBuf::new(),
            blake3::hash(&bytes).to_hex().to_string(),
        )
        .unwrap();
        assert!(supported_look(&look));
        look.table = LookTableSource::Captured {
            cube_path: PathBuf::from("synthetic-only.cube"),
        };
        assert!(!supported_look(&look));
        look.table = LookTableSource::Unavailable;
        assert!(!supported_look(&look));
    }

    #[test]
    fn exact_base_and_real_camera_restriction_are_required_for_a_witness() {
        let assets = Assets::new();
        assets.base("base.dcp", &dcp("Fujifilm X-Pro2", Some(MODULAR), false));
        let table = encoded_table(2, 1.5);
        for (required, restriction) in [("Cobalt Flat", ""), (MODULAR, "Fujifilm X-T5")] {
            assets.look(
                "look.xmp",
                &xmp("synthetic-uuid", required, restriction, Some(&table)),
            );
            assert_eq!(
                assets.decision("X-Pro2"),
                ModularDecision::Skip(DefaultSkip::NoCompatibleLook)
            );
        }
        assets.look(
            "look.xmp",
            &xmp("synthetic-uuid", MODULAR, "Fuji X-Pro2", Some(&table)),
        );
        assert!(matches!(
            assets.decision("X-Pro2"),
            ModularDecision::Eligible(_)
        ));
    }

    #[test]
    fn missing_or_changed_look_identity_and_restriction_do_not_use_cached_pairing() {
        let assets = Assets::new();
        assets.base("base.dcp", &dcp("Fujifilm X-Pro2", Some(MODULAR), false));
        let path = assets.supported();
        for bytes in [
            b"broken XML".to_vec(),
            xmp("different-uuid", MODULAR, "", Some(&encoded_table(2, 1.5))),
            xmp(
                "synthetic-uuid",
                MODULAR,
                "Fuji X-T5",
                Some(&encoded_table(2, 1.5)),
            ),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(
                assets.decision("X-Pro2"),
                ModularDecision::Skip(DefaultSkip::NoCompatibleLook)
            );
        }
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::NoCompatibleLook)
        );
    }

    #[test]
    fn file_size_cap_and_missing_base_are_explicit_failures() {
        let assets = Assets::new();
        let id = assets.base("base.dcp", &dcp("Fujifilm X-Pro2", Some(MODULAR), false));
        assets.supported();
        let path = assets.registry.get(id).unwrap().file_path;
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_ASSET_BYTES + 1)
            .unwrap();
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::AssetTooLarge)
        );
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            assets.decision("X-Pro2"),
            ModularDecision::Skip(DefaultSkip::AssetUnavailable)
        );
    }

    #[test]
    fn coherent_snapshot_is_order_independent_and_changes_on_catalog_mutation() {
        let a = Assets::new();
        let first = a.base("one.dcp", &dcp("Fuji X-Pro2", Some(MODULAR), false));
        a.base("two.dcp", &dcp("Fuji X-T5", Some(MODULAR), false));
        a.supported();
        let b = ProfileRegistry::new();
        for entry in a.registry.all_profiles().into_iter().rev() {
            b.insert_dcp(entry);
        }
        for look in a.registry.looks().into_iter().rev() {
            b.register_look(look);
        }
        let initial = catalog_fingerprint(&a.registry.default_snapshot().unwrap()).unwrap();
        assert_eq!(
            Some(initial),
            catalog_fingerprint(&b.default_snapshot().unwrap())
        );
        b.remove_dcp(first);
        assert_ne!(
            Some(initial),
            catalog_fingerprint(&b.default_snapshot().unwrap())
        );
    }

    #[test]
    fn another_supported_look_can_witness_after_an_unavailable_corrupt_or_missing_one() {
        for failure in ["unavailable", "corrupt", "missing"] {
            let assets = Assets::new();
            let base = assets.base("base.dcp", &dcp("Fuji X-Pro2", Some(MODULAR), false));
            let bad = assets.look("bad.xmp", &xmp("000-bad", MODULAR, "", None));
            match failure {
                "corrupt" => std::fs::write(&bad, b"invalid XML").unwrap(),
                "missing" => std::fs::remove_file(&bad).unwrap(),
                _ => {}
            }
            let good = assets.look(
                "good.xmp",
                &xmp("zzz-good", MODULAR, "", Some(&encoded_table(2, 1.5))),
            );
            let snapshot = assets.registry.default_snapshot().unwrap();
            assert_eq!(snapshot.looks[0].uuid, "000-bad");
            let ModularDecision::Eligible(evidence) = assets.decision("X-Pro2") else {
                panic!("the second real supported Look must satisfy the prerequisite")
            };
            assert_eq!(evidence.base, base);
            assert_eq!(evidence.look_witness_uuid, "zzz-good");
            assert_eq!(
                evidence.look_witness_content_id(),
                blake3::hash(&std::fs::read(good).unwrap())
                    .to_hex()
                    .to_string()
            );
        }
    }

    #[test]
    fn discovered_adobe_profiles_are_eligible_without_reimporting_or_substitution() {
        let assets = Assets::new();
        let base = assets.base("base.dcp", &dcp("Fuji X-Pro2", Some(MODULAR), false));
        let mut entry = assets.registry.get(base).unwrap();
        entry.source = ProfileSource::AdobeAutoDiscovered;
        assets.registry.insert_dcp(entry);
        assets.supported();
        assert!(
            matches!(assets.decision("X-Pro2"), ModularDecision::Eligible(e) if e.base == base)
        );
    }
}
