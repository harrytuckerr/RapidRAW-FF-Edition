//! Synthetic transport and library regressions for the actual preset importer.
//! No JNI, filesystem, picker, application or user assets are exercised here.
use super::*;
use serde_json::json;
use std::cell::Cell;

const URI: &str = "content://synthetic.documents/document/42?grant=PRIVATE_TEST_ONLY";
const XMP: &str = r#"<x:xmpmeta><rdf:Description crs:PresetType="Normal" crs:Exposure2012="+0.50" crs:Contrast2012="+10" crs:CameraProfile="Cobalt Modular" crs:ProfileName="Captured profile name">
<crs:Name><rdf:Alt><rdf:li xml:lang="x-default">Synthetic mixed preset</rdf:li></rdf:Alt></crs:Name>
</rdf:Description></x:xmpmeta>"#;

fn native_file() -> Vec<u8> {
    serde_json::to_vec(&json!({"presets":[{"preset":{
        "id":"original-id", "name":"Tone", "adjustments":{"exposure":0.5},
        "includeProfile":false, "includeMasks":true, "presetType":"tool"
    }}]}))
    .unwrap()
}

fn values(items: &[PresetItem]) -> Value {
    serde_json::to_value(items).unwrap()
}

#[test]
fn saf_uses_resolved_extension_and_reads_original_identifier_once() {
    let names = Cell::new(0);
    let reads = Cell::new(0);
    let mut resolved = None;
    let imported = parse_preset_source_with(
        URI,
        &mut resolved,
        |source| {
            assert_eq!(source, URI);
            names.set(names.get() + 1);
            Ok("Mixed.XMP".into())
        },
        |source| {
            assert_eq!(source, URI);
            reads.set(reads.get() + 1);
            Ok(XMP.as_bytes().to_vec())
        },
    )
    .unwrap();
    assert_eq!((names.get(), reads.get()), (1, 1));
    assert_eq!(resolved.as_deref(), Some("Mixed.XMP"));
    let imported = values(&imported);
    assert_eq!(imported[0]["preset"]["name"], "Synthetic mixed preset");
    assert_eq!(imported[0]["preset"]["adjustments"]["exposure"], 0.5);
    assert_eq!(imported[0]["preset"]["adjustments"]["contrast"], 10);
    // A develop preset naming a camera profile remains an adjustment preset.
    assert!(imported[0]["preset"].get("includeProfile").is_none());
    assert!(
        imported[0]["preset"]["adjustments"]
            .get("profile")
            .is_none()
    );
}

#[test]
fn unresolved_or_empty_names_fail_before_reading_and_never_echo_uri() {
    for missing in [true, false] {
        let reads = Cell::new(0);
        let result = import_preset_files_with(&[URI.into()], vec![], |source, name| {
            parse_preset_source_with(
                source,
                name,
                |_| {
                    if missing {
                        Err(format!(
                            "Cannot resolve {URI} via CONTENT://other/private?secret=1"
                        ))
                    } else {
                        Ok(String::new())
                    }
                },
                |_| {
                    reads.set(reads.get() + 1);
                    Ok(native_file())
                },
            )
        });
        assert_eq!(reads.get(), 0);
        assert_eq!(result.imported, 0);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].file_name, "Item 1");
        let wire = serde_json::to_string(&result).unwrap();
        for private in ["content://", "CONTENT://", "PRIVATE_TEST_ONLY", "secret=1"] {
            assert!(!wire.contains(private), "{wire}");
        }
        assert!(result.failures[0].error.contains(if missing {
            "Cannot resolve"
        } else {
            "no file name"
        }));
    }
}

#[test]
fn read_utf8_and_format_failures_keep_library_and_bound_redacted_details() {
    let existing = parse_preset_bytes("native.rrpreset", &native_file()).unwrap();
    let before = values(&existing);
    let sources = [
        format!("{URI}&n=0"),
        format!("{URI}&n=1"),
        format!("{URI}&n=2"),
    ];
    let mut index = 0;
    let result = import_preset_files_with(&sources, existing, |source, name| {
        let n = index;
        index += 1;
        parse_preset_source_with(
            source,
            name,
            |_| {
                Ok(if n == 2 {
                    "Unsupported.txt"
                } else {
                    "Exposure.xmp"
                }
                .into())
            },
            |_| match n {
                0 => Err(format!(
                    "Access denied: {source} CONTENT://other/private?grant=KEEP_PRIVATE"
                )),
                1 => Ok(vec![0xff, 0xfe]),
                _ => Ok(b"not a preset".to_vec()),
            },
        )
    });
    assert_eq!(result.imported, 0);
    assert_eq!(result.failures.len(), 3);
    assert_eq!(values(&result.presets), before);
    for (failure, reason) in
        result
            .failures
            .iter()
            .zip(["Access denied", "UTF-8", "Choose a .rrpreset"])
    {
        assert!(failure.error.contains(reason), "{}", failure.error);
        assert!(failure.error.chars().count() <= IMPORT_DETAIL_MAX_CHARS);
    }
    let wire = serde_json::to_string(&result).unwrap();
    for private in [
        "content://",
        "CONTENT://",
        "PRIVATE_TEST_ONLY",
        "KEEP_PRIVATE",
    ] {
        assert!(!wire.contains(private), "{wire}");
    }
}

#[test]
fn native_json_and_legacy_wrapper_preserve_supported_adjustments_and_capture() {
    for amount in [0, 100, 200] {
        let original = json!({"presets":[{"folder":{
            "id":"folder", "name":"Synthetic folder", "children":[
                {"id":"captured", "name":"Captured", "includeProfile":true,
                 "adjustments":{"exposure":0.25,"profile":{"base":"a".repeat(64),"look":"synthetic-look","amount":amount}}},
                {"id":"not-captured", "name":"Not captured", "includeProfile":false,
                 "adjustments":{"contrast":10}},
                {"id":"legacy", "name":"Legacy", "adjustments":{"exposure":-0.5}}
            ]
        }}]});
        let bytes = serde_json::to_vec(&original).unwrap();
        for name in ["folder.rrpreset", "folder.json", "extensionless"] {
            assert_eq!(
                values(&parse_preset_bytes(name, &bytes).unwrap()),
                original["presets"]
            );
        }
    }
    let legacy = format!("s.xmp = \"{}\"", XMP.replace('"', r#"\""#));
    let direct = values(&parse_preset_bytes("Mixed.XMP", XMP.as_bytes()).unwrap());
    let wrapped = values(&parse_preset_bytes("Mixed.LRTEMPLATE", legacy.as_bytes()).unwrap());
    assert_eq!(wrapped[0]["preset"]["name"], direct[0]["preset"]["name"]);
    assert_eq!(
        wrapped[0]["preset"]["adjustments"],
        direct[0]["preset"]["adjustments"]
    );
}

#[test]
fn mixed_batches_preserve_existing_items_and_merge_names_ids_and_flags() {
    let existing = parse_preset_bytes("existing.rrpreset", &native_file()).unwrap();
    let before = values(&existing);
    let folder = json!({"presets":[{"folder":{
        "id":"incoming-folder", "name":"Tone", "children":[{
            "id":"incoming-child", "name":"Child", "includeProfile":true,
            "adjustments":{"profile":{"base":"b".repeat(64),"look":null,"amount":100}}
        }]
    }}]});
    let sources = vec!["one.rrpreset".into(), "two.rrpreset".into(), URI.into()];
    let result = import_preset_files_with(&sources, existing, |source, name| {
        *name = Some(source.into());
        match source {
            "one.rrpreset" => parse_preset_bytes(source, &native_file()),
            "two.rrpreset" => parse_preset_bytes(source, &serde_json::to_vec(&folder).unwrap()),
            _ => Err("Synthetic access denied".into()),
        }
    });
    assert_eq!(result.imported, 2); // Top-level items: one preset and one folder.
    assert_eq!(result.failures.len(), 1);
    let after = values(&result.presets);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1]["preset"]["name"], "Tone (1)");
    assert_eq!(after[2]["folder"]["name"], "Tone (2)");
    assert_eq!(
        after[1]["preset"]["adjustments"],
        before[0]["preset"]["adjustments"]
    );
    assert_eq!(after[1]["preset"]["includeProfile"], false);
    assert_eq!(after[2]["folder"]["children"][0]["includeProfile"], true);
    assert_eq!(
        after[2]["folder"]["children"][0]["adjustments"],
        folder["presets"][0]["folder"]["children"][0]["adjustments"]
    );
    let ids = [
        after[1]["preset"]["id"].as_str().unwrap(),
        after[2]["folder"]["id"].as_str().unwrap(),
        after[2]["folder"]["children"][0]["id"].as_str().unwrap(),
    ];
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), 3);
    for id in ids {
        assert!(Uuid::parse_str(id).is_ok());
        assert!(!["original-id", "incoming-folder", "incoming-child"].contains(&id));
    }
}

#[test]
fn actual_profile_look_metadata_is_rejected_without_rejecting_capture_names() {
    for look in [
        r#"<rdf:Description crs:PresetType="Look"/>"#,
        r#"<rdf:Description crs:PresetType='L&#111;ok'/>"#,
        r#"<rdf:Description crs:RGBTable="synthetic-table"/>"#,
    ] {
        for (name, content) in [
            ("look.xmp", look.to_string()),
            (
                "look.lrtemplate",
                format!("s.xmp = \"{}\"", look.replace('"', r#"\""#)),
            ),
        ] {
            let error = parse_preset_bytes(name, content.as_bytes()).unwrap_err();
            assert!(error.contains("profile Look"), "{error}");
            assert!(
                error.contains("Camera Profile > Import Look (.xmp)"),
                "{error}"
            );
        }
    }
    let allowed = format!("<!-- crs:PresetType=\"Look\" -->{XMP}");
    assert_eq!(
        parse_preset_bytes("ordinary.xmp", allowed.as_bytes())
            .unwrap()
            .len(),
        1
    );
    assert!(parse_preset_bytes("broken.xmp", b"<rdf:Description></wrong>").is_err());
    for unsupported in [
        r#"<rdf:Description crs:CameraProfile="Cobalt Modular" crs:ProfileName="Name only"/>"#,
        r#"<rdf:Description crs:UnsupportedSyntheticSetting="1"/>"#,
    ] {
        let error = parse_preset_bytes("unsupported.xmp", unsupported.as_bytes()).unwrap_err();
        assert!(
            error.contains("no supported adjustment settings"),
            "{error}"
        );
    }
    let zero = br#"<rdf:Description crs:CameraProfile="Cobalt Modular" crs:Exposure2012="0"/>"#;
    assert_eq!(
        values(&parse_preset_bytes("zero.xmp", zero).unwrap())[0]["preset"]["adjustments"]["exposure"],
        0
    );
}

#[test]
fn empty_selection_and_empty_native_file_report_zero_without_library_change() {
    let existing = parse_preset_bytes("existing.rrpreset", &native_file()).unwrap();
    let before = values(&existing);
    let empty = import_preset_files_with(&[], existing.clone(), |_, _| {
        panic!("empty selection must not read")
    });
    assert_eq!(empty.imported, 0);
    assert!(empty.failures.is_empty());
    assert_eq!(values(&empty.presets), before);
    let no_items = import_preset_files_with(&["empty.rrpreset".into()], existing, |_, _| {
        parse_preset_bytes("empty.rrpreset", br#"{"presets":[]}"#)
    });
    assert_eq!(no_items.imported, 0);
    assert!(no_items.failures.is_empty());
    assert_eq!(values(&no_items.presets), before);
}
