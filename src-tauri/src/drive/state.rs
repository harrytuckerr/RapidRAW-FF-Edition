//! Payload-free, bounded structural ancestry index. No storage or application path.
//!
//! Account association and known blob lengths are caller assertions, not auth or
//! content verification performed here. A structurally single head is NOT Ready:
//! its payload, local dirty state and render compatibility remain unvalidated.

use std::collections::{BTreeMap, HashMap, VecDeque};

use super::model::{
    AccountKey, BlobRef, CanonicalUuid, MAX_REVISION_REFS, ObjectId, ReceivedRevision,
    RevisionKind, RevisionRef, Sha256Hex, UnsignedDecimal,
};

pub const MAX_HEADERS: usize = 10_000;

/// Constructed from a decoded envelope; never retains raw bytes or a payload Value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionHeader {
    account: AccountKey,
    workspace: CanonicalUuid,
    object: ObjectId,
    reference: RevisionRef,
    kind: RevisionKind,
    parents: Vec<RevisionRef>,
    required: Vec<RevisionRef>,
    blobs: Vec<BlobRef>,
}
impl RevisionHeader {
    /// `account` must come from the eventual authenticated ingest boundary.
    /// This constructor only records that association; it does not authenticate it.
    pub fn from_received(account: AccountKey, received: &ReceivedRevision) -> Self {
        let revision = received.revision();
        Self {
            account,
            workspace: revision.workspace_id().clone(),
            object: revision.object_id().clone(),
            reference: RevisionRef {
                revision_id: revision.revision_id().clone(),
                sha256: received.raw_sha256().clone(),
            },
            kind: revision.kind(),
            parents: revision.parents().to_vec(),
            required: revision.required_revisions().to_vec(),
            blobs: revision.dependencies().to_vec(),
        }
    }
    pub fn reference(&self) -> &RevisionRef {
        &self.reference
    }
    pub fn object_id(&self) -> &ObjectId {
        &self.object
    }
    pub fn kind(&self) -> RevisionKind {
        self.kind
    }
    pub fn parents(&self) -> &[RevisionRef] {
        &self.parents
    }
    pub fn required_revisions(&self) -> &[RevisionRef] {
        &self.required
    }
    pub fn dependencies(&self) -> &[BlobRef] {
        &self.blobs
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    ScopeMismatch,
    HeaderLimit,
    RevisionCollision(CanonicalUuid),
    ReferenceHashMismatch {
        child: CanonicalUuid,
        target: CanonicalUuid,
    },
    CrossObjectParent {
        child: CanonicalUuid,
        parent: CanonicalUuid,
    },
    BlobLengthMismatch(Sha256Hex),
    Cycle,
    UnknownObject,
    PendingInputs,
    TooManyResolutionHeads,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestOutcome {
    Inserted,
    Duplicate,
}

/// Counts are diagnostic only. Inspect the corresponding retained header's refs
/// to fetch missing inputs; no transitive payload/reason trees are duplicated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingReasons {
    pub missing_parents: usize,
    pub missing_required_revisions: usize,
    pub missing_blobs: usize,
    pub blocked_revisions: usize,
}
impl PendingReasons {
    pub fn is_pending(self) -> bool {
        self.missing_parents != 0
            || self.missing_required_revisions != 0
            || self.missing_blobs != 0
            || self.blocked_revisions != 0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRevision {
    pub reference: RevisionRef,
    pub reasons: PendingReasons,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralHead {
    pub reference: RevisionRef,
    /// Tombstone is a declared marker, not validated payload or deletion authority.
    pub declared_kind: RevisionKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralHeads {
    None,
    Single(StructuralHead),
    Conflict(Vec<StructuralHead>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectClassification {
    pub object_id: ObjectId,
    /// Known resolved heads remain visible even when descendants are pending.
    pub heads: StructuralHeads,
    pub pending: Vec<PendingRevision>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphAnalysis {
    // Private so callers cannot forge a resolution plan by deleting observed heads.
    objects: Vec<ObjectClassification>,
}
impl GraphAnalysis {
    pub fn objects(&self) -> &[ObjectClassification] {
        &self.objects
    }

    /// Parents for a resolution of this exact observed snapshot. Does not choose
    /// a payload or create a revision. A later unseen branch still conflicts.
    /// Restore may additionally reference a selected prior value, within the
    /// envelope limit; such payload/restore semantics are a later codec concern.
    pub fn resolution_parents(&self, object: &ObjectId) -> Result<Vec<RevisionRef>, GraphError> {
        let found = self
            .objects
            .iter()
            .find(|item| item.object_id == *object)
            .ok_or(GraphError::UnknownObject)?;
        if !found.pending.is_empty() {
            return Err(GraphError::PendingInputs);
        }
        let heads = match &found.heads {
            StructuralHeads::None => return Err(GraphError::PendingInputs),
            StructuralHeads::Single(head) => return Ok(vec![head.reference.clone()]),
            StructuralHeads::Conflict(heads) => heads,
        };
        if heads.len() > MAX_REVISION_REFS {
            return Err(GraphError::TooManyResolutionHeads);
        }
        Ok(heads.iter().map(|head| head.reference.clone()).collect())
    }
}

// Analysis assembly only; not a second persisted index or payload container.
struct ObjectBuilder {
    object_id: ObjectId,
    heads: Vec<StructuralHead>,
    pending: Vec<PendingRevision>,
}

/// One explicit account/workspace partition. No implicit switching or merging.
/// Collisions/capacity rejection latch a failure: a rejected suffix cannot leave
/// an apparently complete analysis of the earlier prefix. Recovery constructs a
/// new index from separately reconciled inputs; there is no winner/reset method.
#[derive(Debug)]
pub struct RevisionGraph {
    account: AccountKey,
    workspace: CanonicalUuid,
    headers: HashMap<CanonicalUuid, RevisionHeader>,
    failure: Option<GraphError>,
}
impl RevisionGraph {
    pub fn new(account: AccountKey, workspace: CanonicalUuid) -> Self {
        Self {
            account,
            workspace,
            headers: HashMap::new(),
            failure: None,
        }
    }
    pub fn len(&self) -> usize {
        self.headers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }
    pub fn header(&self, id: &CanonicalUuid) -> Option<&RevisionHeader> {
        self.headers.get(id)
    }

    /// Adds a header, not an accepted graph: callers must run `analyze` for edges/heads.
    pub fn ingest(&mut self, header: RevisionHeader) -> Result<IngestOutcome, GraphError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if header.account != self.account || header.workspace != self.workspace {
            // Foreign input is rejected, never associated with this partition.
            return Err(GraphError::ScopeMismatch);
        }
        let id = &header.reference.revision_id;
        if let Some(existing) = self.headers.get(id) {
            if existing.reference.sha256 == header.reference.sha256 {
                return Ok(IngestOutcome::Duplicate);
            }
            let error = GraphError::RevisionCollision(id.clone());
            self.failure = Some(error.clone());
            return Err(error);
        }
        if self.headers.len() >= MAX_HEADERS {
            self.failure = Some(GraphError::HeaderLimit);
            return Err(GraphError::HeaderLimit);
        }
        self.headers.insert(id.clone(), header);
        Ok(IngestOutcome::Inserted)
    }

    /// Classification is relative to supplied headers, not proof of provider listing
    /// completeness. `known_blob_length` must be a stable, pure lookup for this call
    /// over a caller-verified content inventory.
    /// None means unavailable; a claimed present hash with a different length is
    /// corruption. This function performs no I/O and never verifies blob bytes.
    /// Parent and required-reference edges both propagate blocking/cycle checks;
    /// only same-object PARENT edges remove ancestor heads. Required refs may be
    /// cross-object within this partition, and never make another object a parent.
    pub fn analyze<F>(&self, known_blob_length: F) -> Result<GraphAnalysis, GraphError>
    where
        F: Fn(&Sha256Hex) -> Option<UnsignedDecimal>,
    {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let mut nodes: Vec<_> = self.headers.values().collect();
        nodes.sort_by(|a, b| {
            a.reference
                .revision_id
                .as_str()
                .cmp(b.reference.revision_id.as_str())
        });
        let positions: HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, header)| (header.reference.revision_id.clone(), i))
            .collect();
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
        let mut indegrees = vec![0usize; nodes.len()];
        let mut pending = vec![PendingReasons::default(); nodes.len()];
        for (i, header) in nodes.iter().enumerate() {
            for (is_parent, references) in [(true, &header.parents), (false, &header.required)] {
                for reference in references {
                    if let Some(&target) = positions.get(&reference.revision_id) {
                        let ancestor = nodes[target];
                        if ancestor.reference.sha256 != reference.sha256 {
                            return Err(GraphError::ReferenceHashMismatch {
                                child: header.reference.revision_id.clone(),
                                target: reference.revision_id.clone(),
                            });
                        }
                        if is_parent && ancestor.object != header.object {
                            return Err(GraphError::CrossObjectParent {
                                child: header.reference.revision_id.clone(),
                                parent: reference.revision_id.clone(),
                            });
                        }
                        children[target].push(i);
                        indegrees[i] += 1;
                    } else if is_parent {
                        pending[i].missing_parents += 1;
                    } else {
                        pending[i].missing_required_revisions += 1;
                    }
                }
            }
            for blob in &header.blobs {
                match known_blob_length(&blob.sha256) {
                    None => pending[i].missing_blobs += 1,
                    Some(bytes) if bytes != blob.bytes => {
                        return Err(GraphError::BlobLengthMismatch(blob.sha256.clone()));
                    }
                    Some(_) => {}
                }
            }
        }
        // Kahn traversal avoids recursion and a quadratic ancestor-closure matrix.
        let mut queue: VecDeque<_> = indegrees
            .iter()
            .enumerate()
            .filter_map(|(i, &degree)| (degree == 0).then_some(i))
            .collect();
        let mut visited = 0;
        while let Some(i) = queue.pop_front() {
            visited += 1;
            for &child in &children[i] {
                if pending[i].is_pending() {
                    pending[child].blocked_revisions += 1;
                }
                indegrees[child] -= 1;
                if indegrees[child] == 0 {
                    queue.push_back(child);
                }
            }
        }
        if visited != nodes.len() {
            return Err(GraphError::Cycle);
        }
        let mut superseded = vec![false; nodes.len()];
        for (i, header) in nodes.iter().enumerate() {
            if !pending[i].is_pending() {
                for parent in &header.parents {
                    // A nonpending header has every parent present and resolved.
                    if let Some(&target) = positions.get(&parent.revision_id) {
                        superseded[target] = true;
                    }
                }
            }
        }
        let mut objects: BTreeMap<String, ObjectBuilder> = BTreeMap::new();
        for (i, header) in nodes.iter().enumerate() {
            let entry = objects
                .entry(header.object.as_str().to_owned())
                .or_insert_with(|| ObjectBuilder {
                    object_id: header.object.clone(),
                    heads: Vec::new(),
                    pending: Vec::new(),
                });
            if pending[i].is_pending() {
                entry.pending.push(PendingRevision {
                    reference: header.reference.clone(),
                    reasons: pending[i],
                });
            } else if !superseded[i] {
                entry.heads.push(StructuralHead {
                    reference: header.reference.clone(),
                    declared_kind: header.kind,
                });
            }
        }
        Ok(GraphAnalysis {
            objects: objects
                .into_values()
                .map(|mut object| {
                    let heads = match object.heads.len() {
                        0 => StructuralHeads::None,
                        1 => StructuralHeads::Single(object.heads.remove(0)),
                        _ => StructuralHeads::Conflict(object.heads),
                    };
                    ObjectClassification {
                        object_id: object.object_id,
                        heads,
                        pending: object.pending,
                    }
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{StorageSpace, decode_revision};
    use super::*;
    use serde_json::{Value, json};

    fn id(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012x}")
    }
    fn account() -> AccountKey {
        serde_json::from_value(json!({"projectNumber":"123","drivePermissionId":"test-user"}))
            .unwrap()
    }
    fn workspace() -> CanonicalUuid {
        CanonicalUuid::try_from(id(1)).unwrap()
    }
    fn object() -> ObjectId {
        ObjectId::try_from("settings".to_owned()).unwrap()
    }
    fn wire(n: u32, parents: &[RevisionRef]) -> Value {
        json!({"schemaVersion":1,"workspaceId":id(1),"objectId":"settings","kind":"settings",
            "revisionId":id(n),"parents":parents,"deviceId":id(2),"deviceSequence":"1",
            "createdAt":"2026-10-15","engineContract":"v1","payload":{"synthetic":n},
            "dependencies":[],"requiredRevisions":[]})
    }
    fn from_wire(value: Value) -> RevisionHeader {
        let received = decode_revision(
            &serde_json::to_vec(&value).unwrap(),
            StorageSpace::AppDataFolder,
        )
        .unwrap();
        RevisionHeader::from_received(account(), &received)
    }
    fn header(n: u32, parents: &[RevisionRef]) -> RevisionHeader {
        from_wire(wire(n, parents))
    }
    fn graph(headers: &[RevisionHeader]) -> RevisionGraph {
        let mut graph = RevisionGraph::new(account(), workspace());
        for header in headers {
            graph.ingest(header.clone()).unwrap();
        }
        graph
    }
    fn analyze(graph: &RevisionGraph) -> GraphAnalysis {
        graph.analyze(|_| None).unwrap()
    }
    fn only_head(analysis: &GraphAnalysis) -> &StructuralHead {
        match &analysis.objects()[0].heads {
            StructuralHeads::Single(head) => head,
            other => panic!("expected single structural head, got {other:?}"),
        }
    }

    #[test]
    fn all_arrival_orders_are_idempotent_and_have_the_same_heads() {
        let a = header(10, &[]);
        let b = header(11, std::slice::from_ref(&a.reference));
        let c = header(12, std::slice::from_ref(&a.reference));
        let expected = analyze(&graph(&[a.clone(), b.clone(), c.clone()]));
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let nodes = [a.clone(), b.clone(), c.clone()];
            let mut g = graph(&[]);
            for i in order {
                assert_eq!(g.ingest(nodes[i].clone()), Ok(IngestOutcome::Inserted));
                assert_eq!(g.ingest(nodes[i].clone()), Ok(IngestOutcome::Duplicate));
            }
            assert_eq!(analyze(&g), expected);
        }
        let StructuralHeads::Conflict(heads) = &expected.objects()[0].heads else {
            panic!("expected branches");
        };
        assert_eq!(
            heads
                .iter()
                .map(|h| h.reference.clone())
                .collect::<Vec<_>>(),
            vec![b.reference, c.reference]
        );
    }

    #[test]
    fn pending_descendant_keeps_known_head_and_blocks_resolution() {
        let a = header(10, &[]);
        let absent = header(11, &[]);
        let b = header(12, &[a.reference.clone(), absent.reference.clone()]);
        let c = header(13, std::slice::from_ref(&b.reference));
        let mut g = graph(&[c, b, a.clone()]);
        let result = analyze(&g);
        assert_eq!(only_head(&result).reference, a.reference);
        assert_eq!(result.objects()[0].pending.len(), 2);
        assert_eq!(result.objects()[0].pending[0].reasons.missing_parents, 1);
        assert_eq!(result.objects()[0].pending[1].reasons.blocked_revisions, 1);
        assert_eq!(
            result.resolution_parents(&object()),
            Err(GraphError::PendingInputs)
        );
        g.ingest(absent).unwrap();
        let result = analyze(&g);
        assert_eq!(only_head(&result).reference.revision_id.as_str(), id(13));
        assert!(result.objects()[0].pending.is_empty());
    }

    #[test]
    fn missing_ancestor_never_becomes_an_apparent_single_tip() {
        let a = header(10, &[]);
        let b = header(11, &[a.reference]);
        let c = header(12, std::slice::from_ref(&b.reference));
        let result = analyze(&graph(&[c, b]));
        assert_eq!(result.objects()[0].heads, StructuralHeads::None);
        assert_eq!(result.objects()[0].pending.len(), 2);
    }

    #[test]
    fn raw_identity_collision_latches_instead_of_selecting_a_winner() {
        let a = header(10, &[]);
        let mut other = wire(10, &[]);
        other["payload"] = json!({"different":true});
        let b = from_wire(other);
        for nodes in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
            let mut g = graph(&[nodes[0].clone()]);
            let expected = GraphError::RevisionCollision(a.reference.revision_id.clone());
            assert_eq!(g.ingest(nodes[1].clone()), Err(expected.clone()));
            assert_eq!(g.analyze(|_| None), Err(expected));
            assert_eq!(g.len(), 1);
        }
    }

    #[test]
    fn whitespace_changes_are_a_raw_hash_collision_for_the_same_revision_id() {
        let value = wire(10, &[]);
        let raw = serde_json::to_vec_pretty(&value).unwrap();
        let received = decode_revision(&raw, StorageSpace::AppDataFolder).unwrap();
        let mut g = graph(&[from_wire(value)]);
        assert!(matches!(
            g.ingest(RevisionHeader::from_received(account(), &received)),
            Err(GraphError::RevisionCollision(_))
        ));
    }

    #[test]
    fn account_and_workspace_cannot_retarget_an_index() {
        let a = header(10, &[]);
        let mut g = graph(std::slice::from_ref(&a));
        let mut foreign_account = a.clone();
        foreign_account.account.drive_permission_id =
            super::super::model::ProviderId::try_from("other".to_owned()).unwrap();
        assert_eq!(g.ingest(foreign_account), Err(GraphError::ScopeMismatch));
        let mut v = wire(11, &[]);
        v["workspaceId"] = json!(id(99));
        assert_eq!(g.ingest(from_wire(v)), Err(GraphError::ScopeMismatch));
        assert_eq!(g.len(), 1);
        assert_eq!(only_head(&analyze(&g)).reference, a.reference);
    }

    #[test]
    fn cross_object_parent_is_corruption_but_required_revision_is_not_ancestry() {
        let a = header(10, &[]);
        let mut v = wire(11, std::slice::from_ref(&a.reference));
        v["objectId"] = json!(format!("checkpoint:{}", id(77)));
        v["kind"] = json!("checkpoint");
        let bad = from_wire(v.clone());
        assert!(matches!(
            graph(&[a.clone(), bad]).analyze(|_| None),
            Err(GraphError::CrossObjectParent { .. })
        ));
        v["parents"] = json!([]);
        v["requiredRevisions"] = json!([a.reference.clone()]);
        let good = from_wire(v);
        let result = analyze(&graph(&[good, a]));
        assert_eq!(result.objects().len(), 2);
        assert!(
            result
                .objects()
                .iter()
                .all(|o| matches!(&o.heads, StructuralHeads::Single(_)) && o.pending.is_empty())
        );
    }

    #[test]
    fn required_revision_and_blob_availability_propagate_pending() {
        let dependency = header(10, &[]);
        let mut v = wire(11, &[]);
        v["objectId"] = json!(format!("checkpoint:{}", id(77)));
        v["kind"] = json!("checkpoint");
        v["requiredRevisions"] = json!([dependency.reference.clone()]);
        v["dependencies"] = json!([{"sha256":"a".repeat(64),"bytes":"8","kind":"state"}]);
        let node = from_wire(v);
        let mut g = graph(&[node]);
        let r = analyze(&g);
        assert_eq!(
            r.objects()[0].pending[0].reasons.missing_required_revisions,
            1
        );
        assert_eq!(r.objects()[0].pending[0].reasons.missing_blobs, 1);
        g.ingest(dependency).unwrap();
        let pending = analyze(&g);
        assert_eq!(pending.objects()[0].pending[0].reasons.missing_blobs, 1);
        let complete = g.analyze(|_| Some(UnsignedDecimal::new(8))).unwrap();
        assert!(complete.objects().iter().all(|o| o.pending.is_empty()));
        assert!(matches!(
            g.analyze(|_| Some(UnsignedDecimal::new(9))),
            Err(GraphError::BlobLengthMismatch(_))
        ));
    }

    #[test]
    fn present_parent_with_wrong_hash_is_not_missing_or_accepted() {
        let a = header(10, &[]);
        let mut wrong = a.reference.clone();
        wrong.sha256 = Sha256Hex::of_bytes(b"wrong");
        let b = header(11, &[wrong]);
        for nodes in [[a.clone(), b.clone()], [b, a]] {
            assert!(matches!(
                graph(&nodes).analyze(|_| None),
                Err(GraphError::ReferenceHashMismatch { .. })
            ));
        }
    }

    #[test]
    fn cycles_are_detected_iteratively_even_when_another_input_is_missing() {
        // Test-only corrupt header injection: cyclic raw-hash commitments cannot
        // conveniently be constructed as real hash-consistent JSON documents.
        let mut a = header(10, &[]);
        let mut b = header(11, &[]);
        let missing = header(12, &[]);
        a.parents = vec![b.reference.clone(), missing.reference];
        b.parents = vec![a.reference.clone()];
        assert_eq!(
            graph(&[a.clone(), b.clone()]).analyze(|_| None),
            Err(GraphError::Cycle)
        );
        // Required-dependency cycles block too; they do not remove object heads.
        a.parents.clear();
        a.required = vec![b.reference.clone()];
        b.parents.clear();
        b.required = vec![a.reference.clone()];
        assert_eq!(graph(&[b, a]).analyze(|_| None), Err(GraphError::Cycle));
    }

    #[test]
    fn resolution_covers_observed_heads_and_an_unseen_branch_still_conflicts() {
        let a = header(10, &[]);
        let b = header(11, std::slice::from_ref(&a.reference));
        let c = header(12, std::slice::from_ref(&a.reference));
        let mut g = graph(&[a.clone(), b.clone(), c.clone()]);
        let parents = analyze(&g).resolution_parents(&object()).unwrap();
        assert_eq!(parents, vec![b.reference, c.reference]);
        let resolved = header(13, &parents);
        g.ingest(resolved.clone()).unwrap();
        assert_eq!(only_head(&analyze(&g)).reference, resolved.reference);
        let unseen = header(14, &[a.reference]);
        g.ingest(unseen.clone()).unwrap();
        let final_analysis = analyze(&g);
        let StructuralHeads::Conflict(heads) = &final_analysis.objects()[0].heads else {
            panic!("unseen branch lost");
        };
        assert_eq!(heads.len(), 2);
        assert!(heads.iter().any(|h| h.reference == unseen.reference));
    }

    #[test]
    fn tombstone_is_causal_and_concurrent_edit_is_preserved() {
        let a = header(10, &[]);
        let mut v = wire(11, std::slice::from_ref(&a.reference));
        v["kind"] = json!("tombstone");
        let tombstone = from_wire(v);
        let mut g = graph(&[a.clone(), tombstone.clone()]);
        let analysis = analyze(&g);
        assert_eq!(only_head(&analysis).declared_kind, RevisionKind::Tombstone);
        let edited = header(12, std::slice::from_ref(&a.reference));
        g.ingest(edited.clone()).unwrap();
        let analysis = analyze(&g);
        let StructuralHeads::Conflict(heads) = &analysis.objects()[0].heads else {
            panic!("lost concurrent edit");
        };
        assert_eq!(heads.len(), 2);
        let restored = header(13, &analysis.resolution_parents(&object()).unwrap());
        g.ingest(restored.clone()).unwrap();
        assert_eq!(only_head(&analyze(&g)).reference, restored.reference);
        // No payload targetObjectId/observedHeads or actual deletion is validated here.
    }

    #[test]
    fn resolution_bound_does_not_truncate_conflicts() {
        let nodes: Vec<_> = (10..43).map(|n| header(n, &[])).collect();
        let r = analyze(&graph(&nodes));
        assert_eq!(
            r.resolution_parents(&object()),
            Err(GraphError::TooManyResolutionHeads)
        );
        let StructuralHeads::Conflict(heads) = &r.objects()[0].heads else {
            panic!("missing conflict");
        };
        assert_eq!(heads.len(), 33);
    }

    #[test]
    fn ten_thousand_header_boundary_duplicates_and_overflow_are_explicit() {
        let mut g = graph(&[]);
        for n in 0..MAX_HEADERS {
            g.ingest(header(n as u32 + 10, &[])).unwrap();
        }
        assert_eq!(g.len(), MAX_HEADERS);
        assert_eq!(g.ingest(header(10, &[])), Ok(IngestOutcome::Duplicate));
        let r = analyze(&g);
        let StructuralHeads::Conflict(heads) = &r.objects()[0].heads else {
            panic!("missing heads");
        };
        assert_eq!(heads.len(), MAX_HEADERS);
        assert_eq!(g.ingest(header(20_000, &[])), Err(GraphError::HeaderLimit));
        assert_eq!(g.analyze(|_| None), Err(GraphError::HeaderLimit));
        assert_eq!(g.len(), MAX_HEADERS);
    }

    #[test]
    fn long_chain_uses_iterative_ancestry_without_retaining_all_ancestor_sets() {
        let mut g = graph(&[]);
        let mut previous = header(10, &[]);
        g.ingest(previous.clone()).unwrap();
        for n in 1..MAX_HEADERS {
            let next = header(n as u32 + 10, &[previous.reference.clone()]);
            g.ingest(next.clone()).unwrap();
            previous = next;
        }
        let analysis = analyze(&g);
        assert_eq!(only_head(&analysis).reference, previous.reference);
        assert!(analysis.objects()[0].pending.is_empty());
    }

    #[test]
    fn required_revision_blocking_propagates_without_becoming_ancestry() {
        let mut dependency = wire(10, &[]);
        dependency["dependencies"] = json!([{"sha256":"a".repeat(64),"bytes":"8","kind":"state"}]);
        let dependency = from_wire(dependency);
        let mut v = wire(11, &[]);
        v["objectId"] = json!(format!("checkpoint:{}", id(77)));
        v["kind"] = json!("checkpoint");
        v["requiredRevisions"] = json!([dependency.reference.clone()]);
        let g = graph(&[from_wire(v), dependency]);
        let pending = analyze(&g);
        assert_eq!(pending.objects()[0].pending[0].reasons.blocked_revisions, 1);
        assert_eq!(pending.objects()[1].pending[0].reasons.missing_blobs, 1);
        assert!(
            pending
                .objects()
                .iter()
                .all(|o| o.heads == StructuralHeads::None)
        );
        let resolved = g.analyze(|_| Some(UnsignedDecimal::new(8))).unwrap();
        assert!(
            resolved
                .objects()
                .iter()
                .all(|o| matches!(&o.heads, StructuralHeads::Single(_)))
        );
    }

    #[test]
    fn extracted_header_does_not_keep_payload_bytes_or_tree() {
        let mut v = wire(10, &[]);
        v["payload"] = json!({"marker":"payload-not-retained","array":[1,2,3]});
        let h = from_wire(v);
        assert!(!format!("{h:?}").contains("payload-not-retained"));
        assert!(h.dependencies().is_empty());
        assert_eq!(h.reference().revision_id.as_str(), id(10));
    }
}
