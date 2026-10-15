//! Immutable native registration configuration and exact capability selection.
//!
//! Constructors validate syntax and policy only. The caller must supply separately
//! reviewed native configuration; these strings do not attest a client/project
//! mapping, account identity or actual provider grants. No registrations, loader,
//! default selection, I/O or session activation are supplied by this module.

use std::fmt;

use super::pkce::ApprovedScope;

const MAX_ID_BYTES: usize = 128;
const MAX_CLIENT_BYTES: usize = 256;
const MAX_PROJECT_BYTES: usize = 20;
const MAX_RECORDS: usize = 8;
const CLIENT_SUFFIX: &str = ".apps.googleusercontent.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationError {
    InvalidId,
    InvalidClientId,
    InvalidProjectNumber,
    InvalidAllowedScopes,
    InvalidRequestedScopes,
    TooManyRecords,
    DuplicateId,
    DuplicateClientId,
    NotConfigured,
    UnknownRegistration,
    DisallowedScopes,
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidId => "Invalid native registration identifier",
            Self::InvalidClientId => "Invalid native desktop client identifier",
            Self::InvalidProjectNumber => "Invalid native project number",
            Self::InvalidAllowedScopes => "Invalid configured capability set",
            Self::InvalidRequestedScopes => "Invalid requested capability set",
            Self::TooManyRecords => "Native registration catalog limit exceeded",
            Self::DuplicateId => "Duplicate native registration identifier",
            Self::DuplicateClientId => "Duplicate native desktop client identifier",
            Self::NotConfigured => "Native registration catalog is not configured",
            Self::UnknownRegistration => "Native registration is unavailable",
            Self::DisallowedScopes => "Requested capabilities are not configured",
        })
    }
}

impl std::error::Error for RegistrationError {}

/// One validated native configuration record, not authenticated provider mapping.
pub struct NativeRegistration {
    id: String,
    desktop_client_id: String,
    project_number: String,
    allowed: Vec<ApprovedScope>,
}

impl NativeRegistration {
    pub fn from_native_configuration(
        id: &str,
        desktop_client_id: &str,
        project_number: &str,
        allowed: &[ApprovedScope],
    ) -> Result<Self, RegistrationError> {
        if !identifier(id, MAX_ID_BYTES) {
            return Err(RegistrationError::InvalidId);
        }
        if !identifier(desktop_client_id, MAX_CLIENT_BYTES)
            || desktop_client_id.len() <= CLIENT_SUFFIX.len()
            || !desktop_client_id.ends_with(CLIENT_SUFFIX)
        {
            return Err(RegistrationError::InvalidClientId);
        }
        // Bound before scanning/parsing or copying; keep the canonical input, not
        // a guessed client prefix or a normalized replacement. This matches S2's
        // positive-u64 context, intentionally narrower than legacy NativeStart.
        if project_number.is_empty()
            || project_number.len() > MAX_PROJECT_BYTES
            || project_number.starts_with('0')
            || !project_number.bytes().all(|b| b.is_ascii_digit())
            || project_number.parse::<u64>().is_err()
        {
            return Err(RegistrationError::InvalidProjectNumber);
        }
        if !scope_set(allowed) {
            return Err(RegistrationError::InvalidAllowedScopes);
        }
        // Every variable-length input has passed its bound before any copy.
        Ok(Self {
            id: id.to_owned(),
            desktop_client_id: desktop_client_id.to_owned(),
            project_number: project_number.to_owned(),
            allowed: allowed.to_vec(),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn desktop_client_id(&self) -> &str {
        &self.desktop_client_id
    }

    pub fn project_number(&self) -> &str {
        &self.project_number
    }

    /// Configured policy only; this is not the account's actual granted set.
    pub fn allowed_scopes(&self) -> &[ApprovedScope] {
        &self.allowed
    }
}

/// An immutable, bounded catalog. An empty catalog resolves as NotConfigured.
pub struct NativeRegistrationCatalog {
    records: Vec<NativeRegistration>,
}

impl NativeRegistrationCatalog {
    pub fn from_native_records(
        records: Vec<NativeRegistration>,
    ) -> Result<Self, RegistrationError> {
        if records.len() > MAX_RECORDS {
            return Err(RegistrationError::TooManyRecords);
        }
        for (index, record) in records.iter().enumerate() {
            if records[..index].iter().any(|prior| prior.id == record.id) {
                return Err(RegistrationError::DuplicateId);
            }
            if records[..index]
                .iter()
                .any(|prior| prior.desktop_client_id == record.desktop_client_id)
            {
                return Err(RegistrationError::DuplicateClientId);
            }
        }
        Ok(Self { records })
    }

    /// Whether native records are absent; not provider mapping, actual grants,
    /// account verification, connection status or Drive readiness.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn resolve(
        &self,
        selection: &RegistrationSelection,
    ) -> Result<&NativeRegistration, RegistrationError> {
        if self.records.is_empty() {
            return Err(RegistrationError::NotConfigured);
        }
        let record = self
            .records
            .iter()
            .find(|record| record.id == selection.id)
            .ok_or(RegistrationError::UnknownRegistration)?;
        if !selection
            .requested
            .iter()
            .all(|scope| record.allowed.contains(scope))
        {
            return Err(RegistrationError::DisallowedScopes);
        }
        Ok(record)
    }
}

/// An exact catalog selector and requested capabilities, never client metadata.
pub struct RegistrationSelection {
    id: String,
    requested: Vec<ApprovedScope>,
}

impl RegistrationSelection {
    pub fn new(id: &str, requested: &[ApprovedScope]) -> Result<Self, RegistrationError> {
        if !identifier(id, MAX_ID_BYTES) {
            return Err(RegistrationError::InvalidId);
        }
        if !scope_set(requested) {
            return Err(RegistrationError::InvalidRequestedScopes);
        }
        Ok(Self {
            id: id.to_owned(),
            requested: requested.to_vec(),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn requested_scopes(&self) -> &[ApprovedScope] {
        &self.requested
    }
}

fn identifier(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn scope_set(scopes: &[ApprovedScope]) -> bool {
    // ApprovedScope is the existing closed AppData/File enum. Check length
    // before indexing or copying; a pair must contain the two distinct values.
    matches!(scopes.len(), 1 | 2) && (scopes.len() == 1 || scopes[0] != scopes[1])
}

#[cfg(test)]
mod tests {
    use super::ApprovedScope::{AppData, File};
    use super::RegistrationError::*;
    use super::*;

    const CLIENT_A: &str = "synthetic-a.apps.googleusercontent.com";
    const CLIENT_B: &str = "synthetic-b.apps.googleusercontent.com";

    fn record(
        id: &str,
        client: &str,
        project: &str,
        allowed: &[ApprovedScope],
    ) -> NativeRegistration {
        NativeRegistration::from_native_configuration(id, client, project, allowed).unwrap()
    }

    fn numbered_records(count: usize) -> Vec<NativeRegistration> {
        (0..count)
            .map(|n| {
                record(
                    &format!("synthetic-{n}"),
                    &format!("synthetic-{n}.apps.googleusercontent.com"),
                    "7",
                    &[File],
                )
            })
            .collect()
    }

    #[test]
    fn borrowed_getters_retain_exact_owned_configuration_and_request_order() {
        let mut id = String::from("Native.A_-9");
        let mut client = String::from("Synthetic_A-9.apps.googleusercontent.com");
        let mut project = String::from("18446744073709551615");
        let mut scopes = [File, AppData];
        let registration = record(&id, &client, &project, &scopes);
        let selection = RegistrationSelection::new(&id, &scopes).unwrap();
        id.clear();
        client.clear();
        project.clear();
        scopes.swap(0, 1);
        assert_eq!(registration.id(), "Native.A_-9");
        assert_eq!(
            registration.desktop_client_id(),
            "Synthetic_A-9.apps.googleusercontent.com"
        );
        assert_eq!(registration.project_number(), "18446744073709551615");
        assert_eq!(registration.allowed_scopes(), &[File, AppData]);
        assert_eq!(selection.id(), "Native.A_-9");
        assert_eq!(selection.requested_scopes(), &[File, AppData]);
    }

    #[test]
    fn registration_and_selector_ids_require_exact_bounded_ascii_syntax() {
        for id in ["", " a", "a ", "a/b", "a:b", "a+b", "a\n", "é", "Ａ"] {
            assert_eq!(
                NativeRegistration::from_native_configuration(id, CLIENT_A, "1", &[File]).err(),
                Some(InvalidId)
            );
            assert_eq!(
                RegistrationSelection::new(id, &[File]).err(),
                Some(InvalidId)
            );
        }
        let at_limit = "A".repeat(128);
        assert_eq!(record(&at_limit, CLIENT_A, "1", &[File]).id(), at_limit);
        assert_eq!(
            RegistrationSelection::new(&at_limit, &[File]).unwrap().id(),
            at_limit
        );
        let over_limit = "A".repeat(129);
        assert_eq!(
            NativeRegistration::from_native_configuration(&over_limit, CLIENT_A, "1", &[File])
                .err(),
            Some(InvalidId)
        );
        assert_eq!(
            RegistrationSelection::new(&over_limit, &[File]).err(),
            Some(InvalidId)
        );
        assert_eq!(record("._-aAZ09", CLIENT_A, "1", &[File]).id(), "._-aAZ09");
    }

    #[test]
    fn client_requires_nonempty_prefix_exact_suffix_and_256_byte_bound() {
        for client in [
            "",
            ".apps.googleusercontent.com",
            "synthetic-a",
            "synthetic-a.apps.googleusercontent.com.evil",
            "synthetic-a.apps.Googleusercontent.com",
            " synthetic-a.apps.googleusercontent.com",
            "synthetic-a.apps.googleusercontent.com ",
            "https://synthetic-a.apps.googleusercontent.com",
            "é.apps.googleusercontent.com",
            "synthetic+a.apps.googleusercontent.com",
            "synthetic-a.apps.googleusercontent.com\0",
        ] {
            assert_eq!(
                NativeRegistration::from_native_configuration("native", client, "1", &[File]).err(),
                Some(InvalidClientId)
            );
        }
        let at_limit = format!(
            "{}{}",
            "a".repeat(256 - ".apps.googleusercontent.com".len()),
            ".apps.googleusercontent.com"
        );
        assert_eq!(at_limit.len(), 256);
        assert_eq!(
            record("native", &at_limit, "1", &[File]).desktop_client_id(),
            at_limit
        );
        let over_limit = format!("a{at_limit}");
        assert_eq!(
            NativeRegistration::from_native_configuration("native", &over_limit, "1", &[File])
                .err(),
            Some(InvalidClientId)
        );
        assert_eq!(
            record("native", "x.apps.googleusercontent.com", "1", &[File]).desktop_client_id(),
            "x.apps.googleusercontent.com"
        );
    }

    #[test]
    fn project_is_retained_canonical_positive_u64_without_normalization() {
        for project in ["1", "18446744073709551615"] {
            assert_eq!(
                record("native", CLIENT_A, project, &[File]).project_number(),
                project
            );
        }
        for project in [
            "",
            "0",
            "00",
            "01",
            "+1",
            "-1",
            "1.0",
            "1e3",
            " 1",
            "1 ",
            "1\n",
            "١",
            "18446744073709551616",
            "99999999999999999999",
            "100000000000000000000",
            "12345678901234567890123456789012",
        ] {
            assert_eq!(
                NativeRegistration::from_native_configuration("native", CLIENT_A, project, &[File])
                    .err(),
                Some(InvalidProjectNumber)
            );
        }
    }

    #[test]
    fn scope_sets_are_nonempty_bounded_unique_and_keep_caller_order() {
        for scopes in [
            &[][..],
            &[File, File],
            &[AppData, AppData],
            &[File, AppData, File],
        ] {
            assert_eq!(
                NativeRegistration::from_native_configuration("native", CLIENT_A, "1", scopes)
                    .err(),
                Some(InvalidAllowedScopes)
            );
            assert_eq!(
                RegistrationSelection::new("native", scopes).err(),
                Some(InvalidRequestedScopes)
            );
        }
        for scopes in [&[File][..], &[AppData], &[File, AppData], &[AppData, File]] {
            assert_eq!(
                record("native", CLIENT_A, "1", scopes).allowed_scopes(),
                scopes
            );
            assert_eq!(
                RegistrationSelection::new("native", scopes)
                    .unwrap()
                    .requested_scopes(),
                scopes
            );
        }
    }

    #[test]
    fn empty_catalog_is_not_configured_and_single_record_resolves() {
        let selection = RegistrationSelection::new("native", &[File]).unwrap();
        let empty = NativeRegistrationCatalog::from_native_records(vec![]).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.resolve(&selection).err(), Some(NotConfigured));
        let one = NativeRegistrationCatalog::from_native_records(vec![record(
            "native",
            CLIENT_A,
            "7",
            &[File],
        )])
        .unwrap();
        assert!(!one.is_empty());
        let resolved = one.resolve(&selection).unwrap();
        assert_eq!(resolved.id(), "native");
        assert_eq!(resolved.desktop_client_id(), CLIENT_A);
        assert_eq!(resolved.project_number(), "7");
    }

    #[test]
    fn catalog_accepts_eight_records_in_either_order_and_rejects_nine() {
        for reverse in [false, true] {
            let mut records = numbered_records(8);
            if reverse {
                records.reverse();
            }
            let catalog = NativeRegistrationCatalog::from_native_records(records).unwrap();
            for (id, client) in [
                ("synthetic-0", "synthetic-0.apps.googleusercontent.com"),
                ("synthetic-7", "synthetic-7.apps.googleusercontent.com"),
            ] {
                let selection = RegistrationSelection::new(id, &[File]).unwrap();
                assert_eq!(
                    catalog.resolve(&selection).unwrap().desktop_client_id(),
                    client
                );
            }
        }
        assert_eq!(
            NativeRegistrationCatalog::from_native_records(numbered_records(9)).err(),
            Some(TooManyRecords)
        );
    }

    #[test]
    fn duplicate_ids_and_clients_reject_conflicting_projects_in_either_order() {
        for reverse in [false, true] {
            let mut same_id = vec![
                record("same", CLIENT_A, "1", &[File]),
                record("same", CLIENT_B, "2", &[AppData]),
            ];
            let mut same_client = vec![
                record("first", CLIENT_A, "1", &[File]),
                record("second", CLIENT_A, "2", &[AppData]),
            ];
            if reverse {
                same_id.reverse();
                same_client.reverse();
            }
            assert_eq!(
                NativeRegistrationCatalog::from_native_records(same_id).err(),
                Some(DuplicateId)
            );
            assert_eq!(
                NativeRegistrationCatalog::from_native_records(same_client).err(),
                Some(DuplicateClientId)
            );
        }
        assert_eq!(
            NativeRegistrationCatalog::from_native_records(vec![
                record("same", CLIENT_A, "1", &[File]),
                record("same", CLIENT_A, "1", &[File])
            ])
            .err(),
            Some(DuplicateId)
        );
        assert_eq!(
            NativeRegistrationCatalog::from_native_records(vec![
                record("first", CLIENT_A, "1", &[File]),
                record("second", CLIENT_A, "1", &[File])
            ])
            .err(),
            Some(DuplicateClientId)
        );
    }

    #[test]
    fn selection_is_exact_and_never_falls_back_to_another_allowed_record() {
        let catalog = NativeRegistrationCatalog::from_native_records(vec![
            record("native", CLIENT_A, "1", &[File]),
            record("native.plus", CLIENT_B, "2", &[AppData]),
        ])
        .unwrap();
        for id in ["Native", "nativ", "native.plus.more", "missing"] {
            let selection = RegistrationSelection::new(id, &[File]).unwrap();
            assert_eq!(catalog.resolve(&selection).err(), Some(UnknownRegistration));
        }
        let wrong_capability = RegistrationSelection::new("native", &[AppData]).unwrap();
        assert_eq!(
            catalog.resolve(&wrong_capability).err(),
            Some(DisallowedScopes)
        );
        let exact = RegistrationSelection::new("native.plus", &[AppData]).unwrap();
        assert_eq!(
            catalog.resolve(&exact).unwrap().desktop_client_id(),
            CLIENT_B
        );
    }

    #[test]
    fn subset_resolution_preserves_record_and_does_not_union_capabilities() {
        let catalog = NativeRegistrationCatalog::from_native_records(vec![
            record("both", CLIENT_A, "1", &[File, AppData]),
            record("file", CLIENT_B, "2", &[File]),
        ])
        .unwrap();
        for scopes in [&[File][..], &[AppData], &[AppData, File], &[File, AppData]] {
            let selection = RegistrationSelection::new("both", scopes).unwrap();
            assert_eq!(
                catalog.resolve(&selection).unwrap().allowed_scopes(),
                &[File, AppData]
            );
            assert_eq!(selection.requested_scopes(), scopes);
        }
        let valid = RegistrationSelection::new("file", &[File]).unwrap();
        let before = catalog.resolve(&valid).unwrap();
        let denied = RegistrationSelection::new("file", &[File, AppData]).unwrap();
        assert_eq!(catalog.resolve(&denied).err(), Some(DisallowedScopes));
        let unknown = RegistrationSelection::new("unknown", &[File]).unwrap();
        assert_eq!(catalog.resolve(&unknown).err(), Some(UnknownRegistration));
        let after = catalog.resolve(&valid).unwrap();
        assert!(std::ptr::eq(before, after));
        assert_eq!(after.id(), "file");
        assert_eq!(after.desktop_client_id(), CLIENT_B);
        assert_eq!(after.project_number(), "2");
        assert_eq!(after.allowed_scopes(), &[File]);
        assert_eq!(valid.requested_scopes(), &[File]);
    }

    #[test]
    fn errors_are_typed_static_and_do_not_reflect_invalid_input() {
        let input = "SYNTHETIC/DO_NOT_REFLECT";
        let error = NativeRegistration::from_native_configuration(input, CLIENT_A, "1", &[File])
            .err()
            .unwrap();
        assert_eq!(error, InvalidId);
        assert_eq!(error.to_string(), "Invalid native registration identifier");
        assert_eq!(format!("{error:?}"), "InvalidId");
        assert!(!error.to_string().contains(input));
    }
}
