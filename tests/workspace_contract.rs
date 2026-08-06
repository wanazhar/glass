#[path = "../src/workspace.rs"]
mod workspace;

use workspace::*;

#[test]
fn references_round_trip_and_reject_cross_scope() {
    let scope = WorkspaceScope::profile(WorkspaceId::new("Workspace-A").unwrap(), ProfileId::new("profile-a").unwrap());
    let reference = ResourceReference::browser(scope.clone(), ResourceId::new("browser-1").unwrap()).unwrap();
    let encoded = reference.to_string();
    assert_eq!(encoded.parse::<ResourceReference>().unwrap(), reference);
    let other = WorkspaceScope::profile(WorkspaceId::new("workspace-b").unwrap(), ProfileId::new("profile-a").unwrap());
    assert!(matches!(reference.validate_scope(&other), Err(ScopeError::WorkspaceMismatch { .. })));
}

#[test]
fn observers_are_read_only_and_takeover_is_revision_guarded() {
    let scope = WorkspaceScope::workspace(WorkspaceId::new("lease-test").unwrap());
    let observer = Attachment::new(AttachmentId::new("observer").unwrap(), ResourceId::new("actor-o").unwrap(), ActorRole::Observer, AttachmentCapabilities::observer(), scope.clone()).unwrap();
    let human = Attachment::new(AttachmentId::new("human").unwrap(), ResourceId::new("actor-h").unwrap(), ActorRole::Human, AttachmentCapabilities::takeover(), scope).unwrap();
    let mut authority = MutationLeaseAuthority::default();
    assert!(matches!(authority.acquire(&observer, Revision(0)), Err(LeaseError::ObserverMutationDenied)));
    let grant = authority.acquire(&human, Revision(0)).unwrap();
    assert!(matches!(authority.takeover(&human, Revision(0)), Err(LeaseError::StaleRevision(_))));
    authority.release(&human, &grant.lease_id, grant.revision).unwrap();
    assert_eq!(authority.snapshot().state, MutationLeaseState::Available);
}

#[test]
fn ephemeral_generation_is_part_of_reference_identity() {
    let scope = WorkspaceScope::workspace(WorkspaceId::new("ephemeral").unwrap())
        .with_generation(WorkspaceGeneration::new(7).unwrap());
    let reference = ResourceReference::browser(scope, ResourceId::new("b").unwrap()).unwrap();
    assert_eq!(reference.to_string(), "glass://workspace/ephemeral/generation/7/browser/b");
    assert_eq!(reference.to_string().parse::<ResourceReference>().unwrap(), reference);
    assert!(WorkspaceGeneration::new(0).is_err());
}

#[test]
fn invalid_reference_and_configuration_are_rejected_on_deserialize() {
    let invalid = serde_json::json!({
        "scope": {"workspaceId": "w", "profileId": null},
        "resource": {"type": "profile", "id": "p"}
    });
    assert!(serde_json::from_value::<ResourceReference>(invalid).is_err());
    let invalid_config = WorkspaceConfig {
        profile_mode: ProfileMode::Named,
        privacy_mode: PrivacyMode::Private,
        storage: WorkspaceStorage::Durable,
        profile_id: None,
        generation: None,
    };
    assert!(invalid_config.validate().is_err());
}

#[test]
fn workspace_disconnect_revokes_lease_and_closing_rejects_mutations() {
    let identity = WorkspaceIdentity::new(WorkspaceId::new("lease-workspace").unwrap(), []).unwrap();
    let mut workspace = Workspace::new(identity, WorkspaceConfig::ephemeral_private(None)).unwrap();
    let scope = workspace.scope();
    let attachment = Attachment::new(
        AttachmentId::new("human").unwrap(),
        ResourceId::new("actor").unwrap(),
        ActorRole::Human,
        AttachmentCapabilities::mutating(),
        scope,
    ).unwrap();
    workspace.attach(attachment.clone()).unwrap();
    let grant = workspace.acquire_lease(attachment.id(), Revision(0)).unwrap();
    assert_eq!(grant.revision, Revision(1));
    workspace.disconnect(attachment.id()).unwrap();
    assert_eq!(workspace.lease().state, MutationLeaseState::Available);
    assert_eq!(workspace.lease().revision, Revision(2));
    workspace.transition(WorkspaceLifecycle::Closing).unwrap();
    assert!(matches!(workspace.acquire_lease(attachment.id(), Revision(2)), Err(WorkspaceError::Lifecycle(LifecycleError::NotMutable { .. }))));
}

#[test]
fn ephemeral_generations_are_unique() {
    let first = WorkspaceConfig::ephemeral_private(None).generation;
    let second = WorkspaceConfig::ephemeral_private(None).generation;
    assert!(first.is_some());
    assert!(second.is_some());
    assert_ne!(first, second);
}

#[test]
fn duplicate_attachments_and_unbounded_references_are_rejected() {
    let identity = WorkspaceIdentity::new(WorkspaceId::new("dupes").unwrap(), []).unwrap();
    let mut workspace = Workspace::new(identity, WorkspaceConfig::ephemeral_private(None)).unwrap();
    let attachment = Attachment::new(
        AttachmentId::new("human").unwrap(),
        ResourceId::new("actor").unwrap(),
        ActorRole::Human,
        AttachmentCapabilities::mutating(),
        workspace.scope(),
    ).unwrap();
    workspace.attach(attachment.clone()).unwrap();
    assert!(matches!(workspace.attach(attachment), Err(WorkspaceError::DuplicateAttachment)));
    let oversized = format!("glass://workspace/w/browser/{}", "x".repeat(MAX_REFERENCE_URI_BYTES));
    assert!(oversized.parse::<ResourceReference>().is_err());
    assert!("glass://workspace/w/generation/0/browser/b".parse::<ResourceReference>().is_err());
}

#[test]
fn profile_reference_cannot_overwrite_scope() {
    let scope = WorkspaceScope::profile(
        WorkspaceId::new("scoped").unwrap(),
        ProfileId::new("profile-a").unwrap(),
    );
    assert!(ResourceReference::profile(scope, ProfileId::new("profile-b").unwrap()).is_err());
}

#[test]
fn ephemeral_scope_requires_generation_and_ownership_is_checked() {
    let missing_scope: WorkspaceScope = serde_json::from_value(serde_json::json!({
        "workspaceId": "ephemeral",
        "storage": "ephemeral"
    })).unwrap();
    assert!(ResourceReference::browser(missing_scope, ResourceId::new("b").unwrap()).is_err());
    let scope = WorkspaceScope::workspace(WorkspaceId::new("owner").unwrap());
    assert!(OwnershipBoundary::new(
        scope,
        OwnershipDomain::Browser,
        OwnershipOwner::Workspace(WorkspaceId::new("owner").unwrap()),
    ).is_err());
    assert!(serde_json::from_value::<WorkspaceConfig>(serde_json::json!({
        "profileMode": "named",
        "privacyMode": "private",
        "storage": "durable"
    })).is_err());
}
