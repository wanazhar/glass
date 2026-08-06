//! Bounded, browser-free identity and ownership contracts for Glass workspaces.
//!
//! This module deliberately contains no browser/session handles.  It describes
//! the durable address space and the authority boundary that later integrations
//! may use to connect those handles.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

pub const MAX_ID_BYTES: usize = 64;
pub const MAX_ALIAS_BYTES: usize = 64;
pub const MAX_ALIASES: usize = 16;
pub const MAX_ATTACHMENTS: usize = 256;

fn normalize_name(value: &str, label: &'static str, max: usize) -> Result<String, WorkspaceError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(WorkspaceError::InvalidName { label, reason: "must not be empty" });
    }
    if trimmed.len() > max {
        return Err(WorkspaceError::InvalidName { label, reason: "is too long" });
    }
    let mut normalized = String::with_capacity(trimmed.len());
    let mut previous_separator = false;
    for character in trimmed.chars() {
        if character.is_ascii_whitespace() || character == '_' || character == '-' || character == '.' {
            if !normalized.is_empty() && !previous_separator {
                normalized.push('-');
                previous_separator = true;
            }
        } else if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else {
            return Err(WorkspaceError::InvalidName { label, reason: "contains an unsupported character" });
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    if normalized.is_empty() {
        return Err(WorkspaceError::InvalidName { label, reason: "must contain an alphanumeric character" });
    }
    if normalized.len() > max {
        return Err(WorkspaceError::InvalidName { label, reason: "is too long after normalization" });
    }
    Ok(normalized)
}

macro_rules! bounded_name {
    ($name:ident, $label:literal, $max:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl AsRef<str>) -> Result<Self, WorkspaceError> {
                Ok(Self(normalize_name(value.as_ref(), $label, $max)?))
            }

            pub fn as_str(&self) -> &str { &self.0 }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where S: Serializer {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where D: Deserializer<'de> {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }

        impl FromStr for $name {
            type Err = WorkspaceError;
            fn from_str(value: &str) -> Result<Self, Self::Err> { Self::new(value) }
        }
    };
}

bounded_name!(WorkspaceId, "workspace id", MAX_ID_BYTES);
bounded_name!(WorkspaceAlias, "workspace alias", MAX_ALIAS_BYTES);
bounded_name!(ProfileId, "profile id", MAX_ID_BYTES);
bounded_name!(ResourceId, "resource id", MAX_ID_BYTES);
bounded_name!(AttachmentId, "attachment id", MAX_ID_BYTES);
bounded_name!(LeaseId, "lease id", MAX_ID_BYTES);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIdentity {
    pub id: WorkspaceId,
    #[serde(default)]
    pub aliases: BTreeSet<WorkspaceAlias>,
}

impl WorkspaceIdentity {
    pub fn new(id: WorkspaceId, aliases: impl IntoIterator<Item = WorkspaceAlias>) -> Result<Self, WorkspaceError> {
        let aliases: BTreeSet<_> = aliases.into_iter().collect();
        if aliases.len() > MAX_ALIASES {
            return Err(WorkspaceError::TooManyAliases { maximum: MAX_ALIASES });
        }
        Ok(Self { id, aliases })
    }

    pub fn add_alias(&mut self, alias: WorkspaceAlias) -> Result<(), WorkspaceError> {
        if !self.aliases.contains(&alias) && self.aliases.len() >= MAX_ALIASES {
            return Err(WorkspaceError::TooManyAliases { maximum: MAX_ALIASES });
        }
        self.aliases.insert(alias);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceLifecycle {
    Active,
    Suspended,
    Closing,
    Closed,
}

impl WorkspaceLifecycle {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!((self, next),
            (Self::Active, Self::Suspended | Self::Closing) |
            (Self::Suspended, Self::Active | Self::Closing) |
            (Self::Closing, Self::Closed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileMode {
    Named,
    Incognito,
    Isolated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrivacyMode {
    Standard,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceStorage {
    Durable,
    Ephemeral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    pub profile_mode: ProfileMode,
    pub privacy_mode: PrivacyMode,
    pub storage: WorkspaceStorage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<ProfileId>,
}

impl WorkspaceConfig {
    pub fn durable_named(profile_id: ProfileId) -> Self {
        Self { profile_mode: ProfileMode::Named, privacy_mode: PrivacyMode::Standard, storage: WorkspaceStorage::Durable, profile_id: Some(profile_id) }
    }

    pub fn ephemeral_private(profile_id: Option<ProfileId>) -> Self {
        Self { profile_mode: ProfileMode::Isolated, privacy_mode: PrivacyMode::Private, storage: WorkspaceStorage::Ephemeral, profile_id }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceScope {
    pub workspace_id: WorkspaceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<ProfileId>,
}

impl WorkspaceScope {
    pub fn workspace(workspace_id: WorkspaceId) -> Self { Self { workspace_id, profile_id: None } }
    pub fn profile(workspace_id: WorkspaceId, profile_id: ProfileId) -> Self { Self { workspace_id, profile_id: Some(profile_id) } }

    pub fn contains(&self, other: &Self) -> bool {
        self.workspace_id == other.workspace_id && self.profile_id == other.profile_id
    }

    pub fn validate(&self, other: &Self) -> Result<(), ScopeError> {
        if self.workspace_id != other.workspace_id {
            return Err(ScopeError::WorkspaceMismatch { expected: self.workspace_id.clone(), actual: other.workspace_id.clone() });
        }
        if self.profile_id != other.profile_id {
            return Err(ScopeError::ProfileMismatch { expected: self.profile_id.clone(), actual: other.profile_id.clone() });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "camelCase")]
pub enum ResourceKind {
    Workspace,
    Browser(ResourceId),
    Target(ResourceId),
    Run(ResourceId),
    Revision(Revision),
    Entity(ResourceId),
    Memory(ResourceId),
    Workflow(ResourceId),
    Replay(ResourceId),
    Profile(ProfileId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReference {
    pub scope: WorkspaceScope,
    pub resource: ResourceKind,
}

pub type ResourceRef = ResourceReference;

impl ResourceReference {
    pub fn workspace(workspace_id: WorkspaceId) -> Self { Self { scope: WorkspaceScope::workspace(workspace_id), resource: ResourceKind::Workspace } }
    pub fn profile(scope: WorkspaceScope, profile_id: ProfileId) -> Result<Self, ReferenceError> {
        if let Some(existing) = &scope.profile_id
            && existing != &profile_id
        {
            return Err(ReferenceError::Scope(ScopeError::ProfileMismatch {
                expected: scope.profile_id,
                actual: Some(profile_id),
            }));
        }
        Ok(Self {
            scope: WorkspaceScope::profile(scope.workspace_id, profile_id.clone()),
            resource: ResourceKind::Profile(profile_id),
        })
    }
    pub fn browser(scope: WorkspaceScope, id: ResourceId) -> Self { Self::scoped(scope, ResourceKind::Browser(id)) }
    pub fn target(scope: WorkspaceScope, id: ResourceId) -> Self { Self::scoped(scope, ResourceKind::Target(id)) }
    pub fn run(scope: WorkspaceScope, id: ResourceId) -> Self { Self::scoped(scope, ResourceKind::Run(id)) }
    pub fn revision(scope: WorkspaceScope, revision: Revision) -> Self { Self::scoped(scope, ResourceKind::Revision(revision)) }
    pub fn entity(scope: WorkspaceScope, id: ResourceId) -> Self { Self::scoped(scope, ResourceKind::Entity(id)) }
    pub fn memory(scope: WorkspaceScope, id: ResourceId) -> Self { Self::scoped(scope, ResourceKind::Memory(id)) }
    pub fn workflow(scope: WorkspaceScope, id: ResourceId) -> Self { Self::scoped(scope, ResourceKind::Workflow(id)) }
    pub fn replay(scope: WorkspaceScope, id: ResourceId) -> Self { Self::scoped(scope, ResourceKind::Replay(id)) }

    fn scoped(scope: WorkspaceScope, resource: ResourceKind) -> Self { Self { scope, resource } }

    pub fn validate_scope(&self, scope: &WorkspaceScope) -> Result<(), ScopeError> { self.scope.validate(scope) }
    pub fn scope(&self) -> &WorkspaceScope { &self.scope }

    fn kind_name(&self) -> &'static str {
        match self.resource {
            ResourceKind::Workspace => "workspace",
            ResourceKind::Browser(_) => "browser",
            ResourceKind::Target(_) => "target",
            ResourceKind::Run(_) => "run",
            ResourceKind::Revision(_) => "revision",
            ResourceKind::Entity(_) => "entity",
            ResourceKind::Memory(_) => "memory",
            ResourceKind::Workflow(_) => "workflow",
            ResourceKind::Replay(_) => "replay",
            ResourceKind::Profile(_) => "profile",
        }
    }
}

impl fmt::Display for ResourceReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "glass://workspace/{}", self.scope.workspace_id)?;
        match &self.resource {
            ResourceKind::Workspace => Ok(()),
            ResourceKind::Profile(profile_id) => write!(formatter, "/profile/{profile_id}"),
            resource => {
                if let Some(profile_id) = &self.scope.profile_id {
                    write!(formatter, "/profile/{profile_id}")?;
                }
                write!(formatter, "/{}", self.kind_name())?;
                match resource {
                    ResourceKind::Revision(revision) => write!(formatter, "/{revision}"),
                    ResourceKind::Browser(id) | ResourceKind::Target(id) | ResourceKind::Run(id) |
                    ResourceKind::Entity(id) | ResourceKind::Memory(id) | ResourceKind::Workflow(id) |
                    ResourceKind::Replay(id) => write!(formatter, "/{id}"),
                    ResourceKind::Workspace | ResourceKind::Profile(_) => Ok(()),
                }
            }
        }
    }
}

impl FromStr for ResourceReference {
    type Err = ReferenceError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let rest = value.strip_prefix("glass://").ok_or(ReferenceError::InvalidScheme)?;
        let pieces: Vec<_> = rest.split('/').collect();
        if pieces.len() < 2 || pieces[0] != "workspace" {
            return Err(ReferenceError::InvalidPath);
        }
        let workspace = WorkspaceId::new(pieces[1]).map_err(ReferenceError::InvalidName)?;
        let tail = &pieces[2..];
        if tail.is_empty() {
            return Ok(Self::workspace(workspace));
        }
        if tail[0] == "profile" {
            if tail.len() == 2 {
                let profile_id = ProfileId::new(tail[1]).map_err(ReferenceError::InvalidName)?;
                return Ok(Self {
                    scope: WorkspaceScope::profile(workspace, profile_id.clone()),
                    resource: ResourceKind::Profile(profile_id),
                });
            }
            if tail.len() != 4 {
                return Err(ReferenceError::InvalidPath);
            }
            let profile_id = ProfileId::new(tail[1]).map_err(ReferenceError::InvalidName)?;
            let resource = parse_scoped_kind(tail[2], tail[3])?;
            return Ok(Self { scope: WorkspaceScope::profile(workspace, profile_id), resource });
        }
        if tail.len() != 2 {
            return Err(ReferenceError::InvalidPath);
        }
        let resource = parse_scoped_kind(tail[0], tail[1])?;
        Ok(Self { scope: WorkspaceScope::workspace(workspace), resource })
    }
}

fn parse_scoped_kind(kind: &str, id: &str) -> Result<ResourceKind, ReferenceError> {
    match kind {
        "browser" => Ok(ResourceKind::Browser(parse_id(id)?)),
        "target" => Ok(ResourceKind::Target(parse_id(id)?)),
        "run" => Ok(ResourceKind::Run(parse_id(id)?)),
        "revision" => Ok(ResourceKind::Revision(parse_revision(id)?)),
        "entity" => Ok(ResourceKind::Entity(parse_id(id)?)),
        "memory" => Ok(ResourceKind::Memory(parse_id(id)?)),
        "workflow" => Ok(ResourceKind::Workflow(parse_id(id)?)),
        "replay" => Ok(ResourceKind::Replay(parse_id(id)?)),
        _ => Err(ReferenceError::UnknownResource),
    }
}

fn parse_id(value: &str) -> Result<ResourceId, ReferenceError> { ResourceId::new(value).map_err(ReferenceError::InvalidName) }
fn parse_revision(value: &str) -> Result<Revision, ReferenceError> { value.parse::<u64>().map(Revision).map_err(|_| ReferenceError::InvalidRevision) }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(pub u64);

impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(formatter) }
}

impl Revision {
    pub fn next(self) -> Result<Self, StaleRevisionError> {
        self.0.checked_add(1).map(Self).ok_or(StaleRevisionError { expected: self.0, actual: self.0 })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActorRole { Human, Agent, Observer }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentCapability { Observe, Mutate, Takeover }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttachmentCapabilities(BTreeSet<AttachmentCapability>);

impl AttachmentCapabilities {
    pub fn observer() -> Self { Self::default() }
    pub fn mutating() -> Self { Self(BTreeSet::from([AttachmentCapability::Observe, AttachmentCapability::Mutate])) }
    pub fn takeover() -> Self { Self(BTreeSet::from([AttachmentCapability::Observe, AttachmentCapability::Mutate, AttachmentCapability::Takeover])) }
    pub fn contains(&self, capability: AttachmentCapability) -> bool { self.0.contains(&capability) }
}

impl Default for AttachmentCapabilities {
    fn default() -> Self { Self(BTreeSet::from([AttachmentCapability::Observe])) }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: AttachmentId,
    pub actor_id: ResourceId,
    pub role: ActorRole,
    pub capabilities: AttachmentCapabilities,
}

impl Attachment {
    pub fn new(id: AttachmentId, actor_id: ResourceId, role: ActorRole, capabilities: AttachmentCapabilities) -> Result<Self, LeaseError> {
        if role == ActorRole::Observer && (capabilities.contains(AttachmentCapability::Mutate) || capabilities.contains(AttachmentCapability::Takeover)) {
            return Err(LeaseError::ObserverMutationDenied);
        }
        Ok(Self { id, actor_id, role, capabilities })
    }

    pub fn can_mutate(&self) -> bool { self.role != ActorRole::Observer && self.capabilities.contains(AttachmentCapability::Mutate) }
    pub fn can_takeover(&self) -> bool { self.can_mutate() && self.capabilities.contains(AttachmentCapability::Takeover) }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OwnershipOwner {
    Workspace(WorkspaceId),
    Attachment(AttachmentId),
    External(ResourceId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OwnershipDomain { Workspace, Browser, Presentation, ExternalAttachment }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnershipBoundary {
    pub domain: OwnershipDomain,
    pub owner: OwnershipOwner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MutationLeaseState {
    Available,
    Held { lease_id: LeaseId, holder: AttachmentId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseGrant {
    pub lease_id: LeaseId,
    pub holder: AttachmentId,
    pub revision: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseSnapshot {
    pub state: MutationLeaseState,
    pub revision: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationLeaseAuthority {
    state: MutationLeaseState,
    revision: Revision,
}

impl Default for MutationLeaseAuthority {
    fn default() -> Self { Self { state: MutationLeaseState::Available, revision: Revision(0) } }
}

impl MutationLeaseAuthority {
    pub fn snapshot(&self) -> LeaseSnapshot { LeaseSnapshot { state: self.state.clone(), revision: self.revision } }

    pub fn acquire(&mut self, attachment: &Attachment, expected: Revision) -> Result<LeaseGrant, LeaseError> {
        self.check_revision(expected)?;
        self.check_mutation(attachment)?;
        if matches!(self.state, MutationLeaseState::Held { .. }) {
            return Err(LeaseError::AlreadyHeld);
        }
        let revision = self.bump_revision()?;
        let lease_id = LeaseId::new(format!("lease-{}", revision.0)).map_err(|_| LeaseError::InvalidLease)?;
        self.state = MutationLeaseState::Held { lease_id: lease_id.clone(), holder: attachment.id.clone() };
        Ok(LeaseGrant { lease_id, holder: attachment.id.clone(), revision })
    }

    pub fn takeover(&mut self, attachment: &Attachment, expected: Revision) -> Result<LeaseGrant, LeaseError> {
        self.check_revision(expected)?;
        self.check_mutation(attachment)?;
        if !attachment.can_takeover() { return Err(LeaseError::TakeoverDenied); }
        if matches!(self.state, MutationLeaseState::Available) { return Err(LeaseError::NotHeld); }
        let revision = self.bump_revision()?;
        let lease_id = LeaseId::new(format!("lease-{}", revision.0)).map_err(|_| LeaseError::InvalidLease)?;
        self.state = MutationLeaseState::Held { lease_id: lease_id.clone(), holder: attachment.id.clone() };
        Ok(LeaseGrant { lease_id, holder: attachment.id.clone(), revision })
    }

    pub fn release(&mut self, attachment: &Attachment, lease_id: &LeaseId, expected: Revision) -> Result<Revision, LeaseError> {
        self.check_revision(expected)?;
        self.check_mutation(attachment)?;
        match &self.state {
            MutationLeaseState::Available => Err(LeaseError::NotHeld),
            MutationLeaseState::Held { lease_id: held_id, holder } if held_id != lease_id => Err(LeaseError::InvalidLease),
            MutationLeaseState::Held { holder, .. } if holder != &attachment.id => Err(LeaseError::NotHolder),
            MutationLeaseState::Held { .. } => {
                let revision = self.bump_revision()?;
                self.state = MutationLeaseState::Available;
                Ok(revision)
            }
        }
    }

    fn check_revision(&self, expected: Revision) -> Result<(), LeaseError> {
        if expected != self.revision { return Err(LeaseError::StaleRevision(StaleRevisionError { expected: expected.0, actual: self.revision.0 })); }
        Ok(())
    }

    fn check_mutation(&self, attachment: &Attachment) -> Result<(), LeaseError> {
        if attachment.role == ActorRole::Observer { return Err(LeaseError::ObserverMutationDenied); }
        if !attachment.can_mutate() { return Err(LeaseError::CapabilityDenied); }
        Ok(())
    }

    fn bump_revision(&mut self) -> Result<Revision, LeaseError> {
        self.revision = self.revision.next().map_err(LeaseError::RevisionOverflow)?;
        Ok(self.revision)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub identity: WorkspaceIdentity,
    pub config: WorkspaceConfig,
    pub lifecycle: WorkspaceLifecycle,
    #[serde(default)]
    pub attachments: BTreeMap<AttachmentId, Attachment>,
    #[serde(skip)]
    lease_authority: MutationLeaseAuthority,
}

impl Workspace {
    pub fn new(identity: WorkspaceIdentity, config: WorkspaceConfig) -> Self {
        Self { identity, config, lifecycle: WorkspaceLifecycle::Active, attachments: BTreeMap::new(), lease_authority: MutationLeaseAuthority::default() }
    }

    pub fn scope(&self) -> WorkspaceScope {
        WorkspaceScope { workspace_id: self.identity.id.clone(), profile_id: self.config.profile_id.clone() }
    }

    pub fn transition(&mut self, next: WorkspaceLifecycle) -> Result<(), LifecycleError> {
        if !self.lifecycle.can_transition_to(next) {
            return Err(LifecycleError::InvalidTransition { from: self.lifecycle, to: next });
        }
        self.lifecycle = next;
        Ok(())
    }

    pub fn attach(&mut self, attachment: Attachment) -> Result<(), WorkspaceError> {
        if self.lifecycle == WorkspaceLifecycle::Closed { return Err(WorkspaceError::Lifecycle(LifecycleError::Closed)); }
        if !self.attachments.contains_key(&attachment.id) && self.attachments.len() >= MAX_ATTACHMENTS {
            return Err(WorkspaceError::TooManyAttachments { maximum: MAX_ATTACHMENTS });
        }
        self.attachments.insert(attachment.id.clone(), attachment);
        Ok(())
    }

    /// Detaching an attachment intentionally leaves the workspace lifecycle unchanged.
    pub fn disconnect(&mut self, attachment_id: &AttachmentId) -> Result<(), WorkspaceError> {
        self.attachments.remove(attachment_id).map(|_| ()).ok_or(WorkspaceError::UnknownAttachment)
    }

    pub fn lease(&self) -> LeaseSnapshot { self.lease_authority.snapshot() }
    pub fn acquire_lease(&mut self, attachment_id: &AttachmentId, expected: Revision) -> Result<LeaseGrant, WorkspaceError> {
        let attachment = self.attachments.get(attachment_id).ok_or(WorkspaceError::UnknownAttachment)?;
        self.lease_authority.acquire(attachment, expected).map_err(WorkspaceError::Lease)
    }
    pub fn takeover_lease(&mut self, attachment_id: &AttachmentId, expected: Revision) -> Result<LeaseGrant, WorkspaceError> {
        let attachment = self.attachments.get(attachment_id).ok_or(WorkspaceError::UnknownAttachment)?;
        self.lease_authority.takeover(attachment, expected).map_err(WorkspaceError::Lease)
    }
    pub fn release_lease(&mut self, attachment_id: &AttachmentId, lease_id: &LeaseId, expected: Revision) -> Result<Revision, WorkspaceError> {
        let attachment = self.attachments.get(attachment_id).ok_or(WorkspaceError::UnknownAttachment)?;
        self.lease_authority.release(attachment, lease_id, expected).map_err(WorkspaceError::Lease)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    InvalidName { label: &'static str, reason: &'static str },
    TooManyAliases { maximum: usize },
    TooManyAttachments { maximum: usize },
    UnknownAttachment,
    Lifecycle(LifecycleError),
    Lease(LeaseError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceError {
    InvalidScheme,
    InvalidPath,
    UnknownResource,
    InvalidRevision,
    InvalidName(WorkspaceError),
    Scope(ScopeError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeError {
    WorkspaceMismatch { expected: WorkspaceId, actual: WorkspaceId },
    ProfileMismatch { expected: Option<ProfileId>, actual: Option<ProfileId> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleRevisionError { pub expected: u64, pub actual: u64 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    InvalidTransition { from: WorkspaceLifecycle, to: WorkspaceLifecycle },
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseError {
    ObserverMutationDenied,
    CapabilityDenied,
    TakeoverDenied,
    AlreadyHeld,
    NotHeld,
    NotHolder,
    InvalidLease,
    StaleRevision(StaleRevisionError),
    RevisionOverflow(StaleRevisionError),
}

macro_rules! display_error {
    ($type:ty, { $($pattern:pat => $message:expr),+ $(,)? }) => {
        impl fmt::Display for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self { $($pattern => formatter.write_str($message),)+ }
            }
        }
        impl std::error::Error for $type {}
    };
}

display_error!(WorkspaceError, {
    Self::InvalidName { .. } => "invalid bounded workspace name",
    Self::TooManyAliases { .. } => "workspace has too many aliases",
    Self::TooManyAttachments { .. } => "workspace has too many attachments",
    Self::UnknownAttachment => "attachment is not connected to the workspace",
    Self::Lifecycle(_) => "workspace lifecycle rejected the operation",
    Self::Lease(_) => "workspace lease operation failed"
});
display_error!(ReferenceError, {
    Self::InvalidScheme => "resource reference must use glass://",
    Self::InvalidPath => "invalid glass resource path",
    Self::UnknownResource => "unknown glass resource kind",
    Self::InvalidRevision => "invalid resource revision",
    Self::InvalidName(_) => "invalid resource identifier",
    Self::Scope(_) => "resource reference scope is invalid"
});
display_error!(ScopeError, {
    Self::WorkspaceMismatch { .. } => "resource belongs to another workspace",
    Self::ProfileMismatch { .. } => "resource belongs to another profile"
});
display_error!(StaleRevisionError, { Self { .. } => "revision is stale" });
display_error!(LifecycleError, {
    Self::InvalidTransition { .. } => "invalid workspace lifecycle transition",
    Self::Closed => "workspace is closed"
});
display_error!(LeaseError, {
    Self::ObserverMutationDenied => "observers cannot obtain mutation authority",
    Self::CapabilityDenied => "attachment lacks mutation capability",
    Self::TakeoverDenied => "attachment lacks takeover capability",
    Self::AlreadyHeld => "mutation lease is already held",
    Self::NotHeld => "mutation lease is not held",
    Self::NotHolder => "attachment does not hold the mutation lease",
    Self::InvalidLease => "invalid mutation lease",
    Self::StaleRevision(_) => "mutation lease revision is stale",
    Self::RevisionOverflow(_) => "mutation lease revision overflow"
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_normalized_and_bounded() {
        assert_eq!(WorkspaceId::new(" Demo_Workspace ").unwrap().as_str(), "demo-workspace");
        assert!(WorkspaceId::new("!").is_err());
        assert!(WorkspaceId::new("a".repeat(MAX_ID_BYTES + 1)).is_err());
    }

    #[test]
    fn resource_reference_round_trips() {
        let scope = WorkspaceScope::profile(WorkspaceId::new("Demo").unwrap(), ProfileId::new("Private").unwrap());
        let reference = ResourceReference::workflow(scope, ResourceId::new("run-1").unwrap());
        let encoded = reference.to_string();
        assert_eq!(encoded, "glass://workspace/demo/profile/private/workflow/run-1");
        assert_eq!(encoded.parse::<ResourceReference>().unwrap(), reference);
    }

    #[test]
    fn lifecycle_and_disconnect_are_explicit() {
        let identity = WorkspaceIdentity::new(WorkspaceId::new("demo").unwrap(), []).unwrap();
        let mut workspace = Workspace::new(identity, WorkspaceConfig::ephemeral_private(None));
        let attachment = Attachment::new(AttachmentId::new("human").unwrap(), ResourceId::new("actor").unwrap(), ActorRole::Human, AttachmentCapabilities::mutating()).unwrap();
        workspace.attach(attachment.clone()).unwrap();
        workspace.disconnect(&attachment.id).unwrap();
        assert_eq!(workspace.lifecycle, WorkspaceLifecycle::Active);
        workspace.transition(WorkspaceLifecycle::Closing).unwrap();
        workspace.transition(WorkspaceLifecycle::Closed).unwrap();
        assert!(workspace.transition(WorkspaceLifecycle::Active).is_err());
    }

    #[test]
    fn lease_arbitration_and_stale_revision_are_guarded() {
        let human = Attachment::new(AttachmentId::new("human").unwrap(), ResourceId::new("h").unwrap(), ActorRole::Human, AttachmentCapabilities::takeover()).unwrap();
        let agent = Attachment::new(AttachmentId::new("agent").unwrap(), ResourceId::new("a").unwrap(), ActorRole::Agent, AttachmentCapabilities::takeover()).unwrap();
        let observer = Attachment::new(AttachmentId::new("observer").unwrap(), ResourceId::new("o").unwrap(), ActorRole::Observer, AttachmentCapabilities::observer()).unwrap();
        let mut authority = MutationLeaseAuthority::default();
        let grant = authority.acquire(&human, Revision(0)).unwrap();
        assert!(matches!(authority.acquire(&agent, grant.revision), Err(LeaseError::AlreadyHeld)));
        assert!(matches!(authority.acquire(&observer, grant.revision), Err(LeaseError::ObserverMutationDenied)));
        assert!(matches!(authority.takeover(&agent, Revision(0)), Err(LeaseError::StaleRevision(_))));
        let replacement = authority.takeover(&agent, grant.revision).unwrap();
        assert_eq!(replacement.holder, agent.id);
        assert!(matches!(authority.release(&human, &replacement.lease_id, replacement.revision), Err(LeaseError::NotHolder)));
        authority.release(&agent, &replacement.lease_id, replacement.revision).unwrap();
    }
}
