//! Independent public-API contract probes. Literal expectations were frozen by
//! root before catalog inspection; configuration syntax is not provider proof.
use super::pkce::ApprovedScope::{self, AppData, File};
use super::registration::{
    NativeRegistration, NativeRegistrationCatalog, RegistrationError, RegistrationSelection,
};
use RegistrationError::*;

fn record(id: &str, client: &str, project: &str, scopes: &[ApprovedScope]) -> NativeRegistration {
    NativeRegistration::from_native_configuration(id, client, project, scopes)
        .expect("valid synthetic configuration")
}
fn numbered(count: usize) -> Vec<NativeRegistration> {
    (0..count)
        .map(|i| {
            record(
                &format!("fixture-{i}"),
                &format!("fixture-{i}.apps.googleusercontent.com"),
                "7",
                &[File],
            )
        })
        .collect()
}

#[test]
fn literal_identifier_answers() {
    let cases = [
        ("camera", true),
        (".", true),
        (
            "A_-.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            true,
        ),
        (
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            false,
        ),
        ("", false),
        (" camera", false),
        ("camera/other", false),
        ("café", false),
    ];
    for (value, valid) in cases {
        let result = NativeRegistration::from_native_configuration(
            value,
            "fixed.apps.googleusercontent.com",
            "1",
            &[File],
        );
        assert_eq!(
            result.err(),
            if valid { None } else { Some(InvalidId) },
            "input: {value:?}"
        );
        assert_eq!(
            RegistrationSelection::new(value, &[File]).err(),
            if valid { None } else { Some(InvalidId) }
        );
    }
}

#[test]
fn literal_client_answers() {
    let cases = [
        ("synthetic.apps.googleusercontent.com", true),
        ("_.apps.googleusercontent.com", true),
        ("X.apps.googleusercontent.com", true),
        (
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.apps.googleusercontent.com",
            true,
        ),
        (
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx.apps.googleusercontent.com",
            false,
        ),
        (".apps.googleusercontent.com", false),
        ("x.apps.googleusercontent.com.evil", false),
        ("x.APPS.GOOGLEUSERCONTENT.COM", false),
        ("https://x.apps.googleusercontent.com", false),
        (" x.apps.googleusercontent.com", false),
    ];
    for (value, valid) in cases {
        let result = NativeRegistration::from_native_configuration("camera", value, "1", &[File]);
        assert_eq!(
            result.err(),
            if valid { None } else { Some(InvalidClientId) },
            "input: {value:?}"
        );
    }
}

#[test]
fn literal_project_answers() {
    let cases = [
        ("1", true),
        ("18446744073709551615", true),
        ("18446744073709551616", false),
        ("0", false),
        ("01", false),
        ("+1", false),
        ("1 ", false),
        ("١", false),
        ("1.0", false),
        ("", false),
    ];
    for (value, valid) in cases {
        let result = NativeRegistration::from_native_configuration(
            "camera",
            "fixed.apps.googleusercontent.com",
            value,
            &[File],
        );
        assert_eq!(
            result.err(),
            if valid {
                None
            } else {
                Some(InvalidProjectNumber)
            },
            "input: {value:?}"
        );
    }
}

#[test]
fn literal_scope_answers_and_order() {
    let cases: &[(&[ApprovedScope], bool)] = &[
        (&[File], true),
        (&[AppData], true),
        (&[AppData, File], true),
        (&[File, AppData], true),
        (&[], false),
        (&[File, File], false),
        (&[AppData, File, AppData], false),
    ];
    for &(scopes, valid) in cases {
        let item = NativeRegistration::from_native_configuration(
            "camera",
            "fixed.apps.googleusercontent.com",
            "1",
            scopes,
        );
        let selection = RegistrationSelection::new("camera", scopes);
        if valid {
            assert_eq!(item.unwrap().allowed_scopes(), scopes);
            assert_eq!(selection.unwrap().requested_scopes(), scopes);
        } else {
            assert_eq!(item.err(), Some(InvalidAllowedScopes));
            assert_eq!(selection.err(), Some(InvalidRequestedScopes));
        }
    }
}

#[test]
fn catalog_capacity_presence_and_duplicates_are_order_independent() {
    let empty = NativeRegistrationCatalog::from_native_records(vec![]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(
        empty
            .resolve(&RegistrationSelection::new("camera", &[File]).unwrap())
            .err(),
        Some(NotConfigured)
    );
    assert!(empty.is_empty());
    for count in [1, 8] {
        for reverse in [false, true] {
            let mut items = numbered(count);
            if reverse {
                items.reverse();
            }
            let catalog = NativeRegistrationCatalog::from_native_records(items).unwrap();
            assert!(!catalog.is_empty());
            for i in 0..count {
                let select = RegistrationSelection::new(&format!("fixture-{i}"), &[File]).unwrap();
                assert_eq!(catalog.resolve(&select).unwrap().project_number(), "7");
            }
        }
    }
    assert_eq!(
        NativeRegistrationCatalog::from_native_records(numbered(9)).err(),
        Some(TooManyRecords)
    );
    for reverse in [false, true] {
        for (second_id, second_client, second_project, expected) in [
            ("a", "b.apps.googleusercontent.com", "2", DuplicateId),
            ("b", "a.apps.googleusercontent.com", "1", DuplicateClientId),
            ("b", "a.apps.googleusercontent.com", "2", DuplicateClientId),
        ] {
            let mut records = vec![
                record("a", "a.apps.googleusercontent.com", "1", &[File]),
                record(second_id, second_client, second_project, &[AppData]),
            ];
            if reverse {
                records.reverse();
            }
            assert_eq!(
                NativeRegistrationCatalog::from_native_records(records).err(),
                Some(expected)
            );
        }
        let mut records = vec![
            record("a", "a.apps.googleusercontent.com", "1", &[File]),
            record("b", "b.apps.googleusercontent.com", "1", &[AppData]),
        ];
        if reverse {
            records.reverse();
        }
        let catalog = NativeRegistrationCatalog::from_native_records(records).unwrap();
        assert_eq!(
            catalog
                .resolve(&RegistrationSelection::new("b", &[AppData]).unwrap())
                .unwrap()
                .project_number(),
            "1"
        );
    }
}

#[test]
fn exact_resolution_has_no_fallback_union_or_presence_mutation() {
    let catalog = NativeRegistrationCatalog::from_native_records(vec![
        record("camera", "camera.apps.googleusercontent.com", "1", &[File]),
        record(
            "camera.plus",
            "plus.apps.googleusercontent.com",
            "1",
            &[AppData, File],
        ),
    ])
    .unwrap();
    let good = RegistrationSelection::new("camera", &[File]).unwrap();
    let before = catalog.resolve(&good).unwrap();
    for id in ["cam", "Camera", "camera.plus.extra", "unknown"] {
        assert_eq!(
            catalog
                .resolve(&RegistrationSelection::new(id, &[File]).unwrap())
                .err(),
            Some(UnknownRegistration)
        );
        assert!(!catalog.is_empty());
    }
    for scopes in [&[AppData][..], &[AppData, File]] {
        assert_eq!(
            catalog
                .resolve(&RegistrationSelection::new("camera", scopes).unwrap())
                .err(),
            Some(DisallowedScopes)
        );
        assert!(!catalog.is_empty());
    }
    for requested in [&[File][..], &[AppData], &[File, AppData], &[AppData, File]] {
        let select = RegistrationSelection::new("camera.plus", requested).unwrap();
        let both = catalog.resolve(&select).unwrap();
        assert_eq!(both.allowed_scopes(), &[AppData, File]);
        assert_eq!(select.requested_scopes(), requested);
    }
    // A successful request to the other record cannot augment this record.
    assert_eq!(
        catalog
            .resolve(&RegistrationSelection::new("camera", &[AppData]).unwrap())
            .err(),
        Some(DisallowedScopes)
    );
    let after = catalog.resolve(&good).unwrap();
    assert!(std::ptr::eq(before, after));
    assert_eq!(after.id(), "camera");
    assert_eq!(
        after.desktop_client_id(),
        "camera.apps.googleusercontent.com"
    );
    assert_eq!(after.project_number(), "1");
    assert_eq!(after.allowed_scopes(), &[File]);
    assert_eq!(good.requested_scopes(), &[File]);
    assert!(!catalog.is_empty());
}

#[test]
fn owned_configuration_and_selector_outlive_mutated_input_buffers() {
    let mut id = String::from(".");
    let mut client = String::from("_.apps.googleusercontent.com");
    let mut project = String::from("18446744073709551615");
    let mut scopes = vec![File, AppData];
    let entry = record(&id, &client, &project, &scopes);
    let select = RegistrationSelection::new(&id, &scopes).unwrap();
    id.clear();
    client.clear();
    project.clear();
    scopes.clear();
    let catalog = NativeRegistrationCatalog::from_native_records(vec![entry]).unwrap();
    let result = catalog.resolve(&select).unwrap();
    assert_eq!(result.id(), ".");
    assert_eq!(result.desktop_client_id(), "_.apps.googleusercontent.com");
    assert_eq!(result.project_number(), "18446744073709551615");
    assert_eq!(result.allowed_scopes(), &[File, AppData]);
    assert_eq!(select.id(), ".");
    assert_eq!(select.requested_scopes(), &[File, AppData]);
}
