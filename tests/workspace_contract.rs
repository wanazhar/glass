#[path = "../src/workspace.rs"]
mod workspace;

use workspace::*;

#[test]
fn references_round_trip_and_reject_cross_scope() {
    let scope = WorkspaceScope::profile(WorkspaceId::new("Workspace-A").unwrap(), ProfileId::new("profile-a").unwrap());
    let reference = ResourceReference::browser(scope.clone(), ResourceId::new("browser-1").unwrap());
    let encoded = reference.to_string();
    assert_eq!(encoded.parse::<ResourceReference>().unwrap(), reference);
    let other = WorkspaceScope::profile(WorkspaceId::new("workspace-b").unwrap(), ProfileId::new("profile-a").unwrap());
    assert!(matches!(reference.validate_scope(&other), Err(ScopeError::WorkspaceMismatch { .. })));
}

#[test]
fn observers_are_read_only_and_takeover_is_revision_guarded() {
    let observer = Attachment::new(AttachmentId::new("observer").unwrap(), ResourceId::new("actor-o").unwrap(), ActorRole::Observer, AttachmentCapabilities::observer()).unwrap();
    let human = Attachment::new(AttachmentId::new("human").unwrap(), ResourceId::new("actor-h").unwrap(), ActorRole::Human, AttachmentCapabilities::takeover()).unwrap();
    let mut authority = MutationLeaseAuthority::default();
    assert!(matches!(authority.acquire(&observer, Revision(0)), Err(LeaseError::ObserverMutationDenied)));
    let grant = authority.acquire(&human, Revision(0)).unwrap();
    assert!(matches!(authority.takeover(&human, Revision(0)), Err(LeaseError::StaleRevision(_))));
    authority.release(&human, &grant.lease_id, grant.revision).unwrap();
    assert_eq!(authority.snapshot().state, MutationLeaseState::Available);
}
