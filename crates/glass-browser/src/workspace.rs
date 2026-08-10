//! Bounded, browser-free identity and ownership contracts for Glass workspaces.
//!
//! This module deliberately contains no browser/session handles.  It describes
//! the durable address space and the authority boundary that later integrations
//! may use to connect those handles.

use fs2::FileExt;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

pub const MAX_ID_BYTES: usize = 64;
pub const MAX_ALIAS_BYTES: usize = 64;
pub const MAX_ALIASES: usize = 16;
pub const MAX_ATTACHMENTS: usize = 256;
pub const MAX_REFERENCE_URI_BYTES: usize = 512;
pub const MAX_REFERENCE_SEGMENTS: usize = 8;
pub const MAX_WIRE_BYTES: usize = 64 * 1024;
/// Ephemeral workspaces carry a non-zero generation so a later workspace
/// incarnation cannot accidentally consume an old resource reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct WorkspaceGeneration(u64);

impl WorkspaceGeneration {
    pub fn new(value: u64) -> Result<Self, WorkspaceError> {
        if value == 0 {
            return Err(WorkspaceError::InvalidGeneration);
        }
        Ok(Self(value))
    }

    /// Allocate an unforgeable nonce when the local OS random source exists;
    /// the timestamp/PID/sequence path remains a bounded fallback.
    pub fn allocate() -> Self {
        let mut random = [0_u8; 8];
        if File::open("/dev/urandom")
            .and_then(|mut source| source.read_exact(&mut random))
            .is_ok()
        {
            let value = u64::from_le_bytes(random);
            if value != 0 {
                return Self(value);
            }
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let process = u64::from(std::process::id()).rotate_left(29);
        let sequence = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        Self(
            timestamp
                .wrapping_add(process)
                .wrapping_add(sequence)
                .max(1),
        )
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for WorkspaceGeneration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for WorkspaceGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

fn normalize_name(value: &str, label: &'static str, max: usize) -> Result<String, WorkspaceError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(WorkspaceError::InvalidName {
            label,
            reason: "must not be empty",
        });
    }
    if trimmed.len() > max {
        return Err(WorkspaceError::InvalidName {
            label,
            reason: "is too long",
        });
    }
    let mut normalized = String::with_capacity(trimmed.len());
    let mut previous_separator = false;
    for character in trimmed.chars() {
        if character.is_ascii_whitespace()
            || character == '_'
            || character == '-'
            || character == '.'
        {
            if !normalized.is_empty() && !previous_separator {
                normalized.push('-');
                previous_separator = true;
            }
        } else if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else {
            return Err(WorkspaceError::InvalidName {
                label,
                reason: "contains an unsupported character",
            });
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    if normalized.is_empty() {
        return Err(WorkspaceError::InvalidName {
            label,
            reason: "must contain an alphanumeric character",
        });
    }
    if normalized.len() > max {
        return Err(WorkspaceError::InvalidName {
            label,
            reason: "is too long after normalization",
        });
    }
    Ok(normalized)
}

fn validate_wire_bytes(input: &str) -> Result<(), serde_json::Error> {
    if input.len() > MAX_WIRE_BYTES {
        return Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "workspace wire payload exceeds bound",
        )));
    }
    Ok(())
}
fn wire_error(error: impl ToString) -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        error.to_string(),
    ))
}

macro_rules! bounded_name {
    ($name:ident, $label:literal, $max:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl AsRef<str>) -> Result<Self, WorkspaceError> {
                Ok(Self(normalize_name(value.as_ref(), $label, $max)?))
            }
            pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
                validate_wire_bytes(input)?;
                let value = serde_json::from_str::<String>(input)?;
                Self::new(value).map_err(|error| {
                    serde_json::Error::io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        error.to_string(),
                    ))
                })
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(_: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Err(serde::de::Error::custom(
                    "direct bounded identity deserialization is unsupported; use from_json",
                ))
            }
        }

        impl FromStr for $name {
            type Err = WorkspaceError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

bounded_name!(WorkspaceId, "workspace id", MAX_ID_BYTES);
bounded_name!(WorkspaceAlias, "workspace alias", MAX_ALIAS_BYTES);
bounded_name!(ProfileId, "profile id", MAX_ID_BYTES);
bounded_name!(ResourceId, "resource id", MAX_ID_BYTES);
bounded_name!(AttachmentId, "attachment id", MAX_ID_BYTES);
bounded_name!(LeaseId, "lease id", MAX_ID_BYTES);
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIdentity {
    id: WorkspaceId,
    #[serde(default)]
    aliases: BTreeSet<WorkspaceAlias>,
}

impl WorkspaceIdentity {
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        validate_wire_bytes(input)?;
        let raw: RawWorkspaceIdentityWire = serde_json::from_str(input)?;
        Self::from_wire(raw)
    }
    fn from_wire(raw: RawWorkspaceIdentityWire) -> Result<Self, serde_json::Error> {
        let id = WorkspaceId::new(raw.id).map_err(wire_error)?;
        let aliases = raw
            .aliases
            .into_iter()
            .map(WorkspaceAlias::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(wire_error)?;
        Self::new(id, aliases).map_err(wire_error)
    }
    pub fn new(
        id: WorkspaceId,
        aliases: impl IntoIterator<Item = WorkspaceAlias>,
    ) -> Result<Self, WorkspaceError> {
        let mut normalized = BTreeSet::new();
        for alias in aliases {
            if normalized.contains(&alias) {
                return Err(WorkspaceError::DuplicateAlias);
            }
            if normalized.len() >= MAX_ALIASES {
                return Err(WorkspaceError::TooManyAliases {
                    maximum: MAX_ALIASES,
                });
            }
            normalized.insert(alias);
        }
        Ok(Self {
            id,
            aliases: normalized,
        })
    }

    pub fn add_alias(&mut self, alias: WorkspaceAlias) -> Result<(), WorkspaceError> {
        if self.aliases.contains(&alias) {
            return Err(WorkspaceError::DuplicateAlias);
        }
        if self.aliases.len() >= MAX_ALIASES {
            return Err(WorkspaceError::TooManyAliases {
                maximum: MAX_ALIASES,
            });
        }
        self.aliases.insert(alias);
        Ok(())
    }

    pub fn id(&self) -> &WorkspaceId {
        &self.id
    }
    pub fn aliases(&self) -> &BTreeSet<WorkspaceAlias> {
        &self.aliases
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspaceIdentityWire {
    id: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspaceIdentity {
    id: WorkspaceId,
    #[serde(default)]
    aliases: BoundedAliases,
}
#[derive(Default)]
struct BoundedAliases(Vec<WorkspaceAlias>);

impl<'de> Deserialize<'de> for BoundedAliases {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AliasesVisitor;
        impl<'de> Visitor<'de> for AliasesVisitor {
            type Value = BoundedAliases;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded workspace alias array")
            }
            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut aliases = Vec::new();
                while let Some(alias) = sequence.next_element::<WorkspaceAlias>()? {
                    if aliases.len() >= MAX_ALIASES {
                        return Err(serde::de::Error::custom(WorkspaceError::TooManyAliases {
                            maximum: MAX_ALIASES,
                        }));
                    }
                    aliases.push(alias);
                }
                Ok(BoundedAliases(aliases))
            }
        }
        deserializer.deserialize_seq(AliasesVisitor)
    }
}

impl<'de> Deserialize<'de> for WorkspaceIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawWorkspaceIdentity::deserialize(deserializer)?;
        Self::new(raw.id, raw.aliases.0).map_err(serde::de::Error::custom)
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
        matches!(
            (self, next),
            (Self::Active, Self::Suspended | Self::Closing)
                | (Self::Suspended, Self::Active | Self::Closing)
                | (Self::Closing, Self::Closed)
        )
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceStorage {
    #[default]
    Durable,
    Ephemeral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    pub profile_mode: ProfileMode,
    pub privacy_mode: PrivacyMode,
    pub storage: WorkspaceStorage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<ProfileId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<WorkspaceGeneration>,
}

impl WorkspaceConfig {
    pub fn durable_named(profile_id: ProfileId) -> Self {
        Self {
            profile_mode: ProfileMode::Named,
            privacy_mode: PrivacyMode::Standard,
            storage: WorkspaceStorage::Durable,
            profile_id: Some(profile_id),
            generation: None,
        }
    }

    pub fn ephemeral_private(profile_id: Option<ProfileId>) -> Self {
        Self {
            profile_mode: ProfileMode::Isolated,
            privacy_mode: PrivacyMode::Private,
            storage: WorkspaceStorage::Ephemeral,
            profile_id,
            generation: Some(WorkspaceGeneration::allocate()),
        }
    }

    pub fn validate(&self) -> Result<(), WorkspaceError> {
        let generation_valid = self.generation.is_none_or(|generation| generation.0 != 0);
        let valid = generation_valid
            && match (self.profile_mode, self.privacy_mode, self.storage) {
                (ProfileMode::Named, PrivacyMode::Standard, WorkspaceStorage::Durable) => {
                    self.profile_id.is_some() && self.generation.is_none()
                }
                (ProfileMode::Incognito, PrivacyMode::Private, WorkspaceStorage::Ephemeral)
                | (ProfileMode::Isolated, PrivacyMode::Private, WorkspaceStorage::Ephemeral) => {
                    self.generation.is_some()
                }
                _ => false,
            };
        if !valid {
            return Err(WorkspaceError::InvalidConfiguration);
        }
        Ok(())
    }
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        validate_wire_bytes(input)?;
        let raw: RawWorkspaceConfigWire = serde_json::from_str(input)?;
        let profile_id = raw
            .profile_id
            .map(ProfileId::new)
            .transpose()
            .map_err(wire_error)?;
        let config = Self {
            profile_mode: raw.profile_mode,
            privacy_mode: raw.privacy_mode,
            storage: raw.storage,
            profile_id,
            generation: raw.generation,
        };
        config.validate().map(|()| config).map_err(wire_error)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspaceConfig {
    profile_mode: ProfileMode,
    privacy_mode: PrivacyMode,
    storage: WorkspaceStorage,
    #[serde(default)]
    profile_id: Option<ProfileId>,
    #[serde(default)]
    generation: Option<WorkspaceGeneration>,
}

impl<'de> Deserialize<'de> for WorkspaceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawWorkspaceConfig::deserialize(deserializer)?;
        let config = Self {
            profile_mode: raw.profile_mode,
            privacy_mode: raw.privacy_mode,
            storage: raw.storage,
            profile_id: raw.profile_id,
            generation: raw.generation,
        };
        config
            .validate()
            .map(|()| config)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceScope {
    workspace_id: WorkspaceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile_id: Option<ProfileId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generation: Option<WorkspaceGeneration>,
    #[serde(default)]
    storage: WorkspaceStorage,
}
impl WorkspaceScope {
    pub fn workspace(workspace_id: WorkspaceId) -> Self {
        Self {
            workspace_id,
            profile_id: None,
            generation: None,
            storage: WorkspaceStorage::Durable,
        }
    }
    pub fn profile(workspace_id: WorkspaceId, profile_id: ProfileId) -> Self {
        Self {
            workspace_id,
            profile_id: Some(profile_id),
            generation: None,
            storage: WorkspaceStorage::Durable,
        }
    }
    pub fn ephemeral(workspace_id: WorkspaceId, generation: WorkspaceGeneration) -> Self {
        Self {
            workspace_id,
            profile_id: None,
            generation: Some(generation),
            storage: WorkspaceStorage::Ephemeral,
        }
    }
    pub fn with_generation(mut self, generation: WorkspaceGeneration) -> Self {
        self.generation = Some(generation);
        self.storage = WorkspaceStorage::Ephemeral;
        self
    }
    fn with_generation_opt(mut self, generation: Option<WorkspaceGeneration>) -> Self {
        self.generation = generation;
        self.storage = if generation.is_some() {
            WorkspaceStorage::Ephemeral
        } else {
            WorkspaceStorage::Durable
        };
        self
    }
    fn with_profile(mut self, profile_id: ProfileId) -> Self {
        self.profile_id = Some(profile_id);
        self
    }
    pub fn workspace_id(&self) -> &WorkspaceId {
        &self.workspace_id
    }
    pub fn profile_id(&self) -> Option<&ProfileId> {
        self.profile_id.as_ref()
    }
    pub fn generation(&self) -> Option<WorkspaceGeneration> {
        self.generation
    }
    pub fn storage(&self) -> WorkspaceStorage {
        self.storage
    }
    fn from_wire(raw: RawWorkspaceScopeWire) -> Result<Self, serde_json::Error> {
        let workspace_id = WorkspaceId::new(raw.workspace_id).map_err(wire_error)?;
        let profile_id = raw
            .profile_id
            .map(ProfileId::new)
            .transpose()
            .map_err(wire_error)?;
        let generation = raw
            .generation
            .map(WorkspaceGeneration::new)
            .transpose()
            .map_err(wire_error)?;
        let scope = Self {
            workspace_id,
            profile_id,
            generation,
            storage: raw.storage.unwrap_or(if generation.is_some() {
                WorkspaceStorage::Ephemeral
            } else {
                WorkspaceStorage::Durable
            }),
        };
        scope
            .validate_invariants()
            .map(|()| scope)
            .map_err(wire_error)
    }
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        validate_wire_bytes(input)?;
        let raw: RawWorkspaceScopeWire = serde_json::from_str(input)?;
        let workspace_id = WorkspaceId::new(raw.workspace_id).map_err(|e| {
            serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            ))
        })?;
        let profile_id = raw
            .profile_id
            .map(ProfileId::new)
            .transpose()
            .map_err(|e| {
                serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                ))
            })?;
        let generation = raw
            .generation
            .map(WorkspaceGeneration::new)
            .transpose()
            .map_err(|e| {
                serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                ))
            })?;
        let scope = Self {
            workspace_id,
            profile_id,
            generation,
            storage: raw.storage.unwrap_or(if generation.is_some() {
                WorkspaceStorage::Ephemeral
            } else {
                WorkspaceStorage::Durable
            }),
        };
        scope.validate_invariants().map(|()| scope).map_err(|e| {
            serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            ))
        })
    }
    pub fn validate(&self, other: &Self) -> Result<(), ScopeError> {
        if self.workspace_id != other.workspace_id {
            return Err(ScopeError::WorkspaceMismatch {
                expected: self.workspace_id.clone(),
                actual: other.workspace_id.clone(),
            });
        }
        if self.profile_id != other.profile_id {
            return Err(ScopeError::ProfileMismatch {
                expected: self.profile_id.clone(),
                actual: other.profile_id.clone(),
            });
        }
        if self.generation != other.generation {
            return Err(ScopeError::GenerationMismatch {
                expected: self.generation,
                actual: other.generation,
            });
        }
        if self.storage != other.storage {
            return Err(ScopeError::StorageMismatch {
                expected: self.storage,
                actual: other.storage,
            });
        }
        Ok(())
    }
    fn validate_invariants(&self) -> Result<(), ScopeError> {
        match (self.storage, self.generation) {
            (WorkspaceStorage::Durable, Some(generation)) => {
                Err(ScopeError::StorageGenerationMismatch {
                    storage: self.storage,
                    generation: Some(generation),
                })
            }
            (WorkspaceStorage::Ephemeral, None) => Err(ScopeError::StorageGenerationMismatch {
                storage: self.storage,
                generation: None,
            }),
            (_, Some(WorkspaceGeneration(0))) => Err(ScopeError::StorageGenerationMismatch {
                storage: self.storage,
                generation: self.generation,
            }),
            _ => Ok(()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspaceScopeWire {
    workspace_id: String,
    #[serde(default)]
    profile_id: Option<String>,
    #[serde(default)]
    generation: Option<u64>,
    #[serde(default)]
    storage: Option<WorkspaceStorage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspaceScope {
    workspace_id: WorkspaceId,
    #[serde(default)]
    profile_id: Option<ProfileId>,
    #[serde(default)]
    generation: Option<WorkspaceGeneration>,
    #[serde(default)]
    storage: Option<WorkspaceStorage>,
}

impl<'de> Deserialize<'de> for WorkspaceScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawWorkspaceScope::deserialize(deserializer)?;
        let scope = Self {
            workspace_id: raw.workspace_id,
            profile_id: raw.profile_id,
            generation: raw.generation,
            storage: raw.storage.unwrap_or(if raw.generation.is_some() {
                WorkspaceStorage::Ephemeral
            } else {
                WorkspaceStorage::Durable
            }),
        };
        scope
            .validate_invariants()
            .map(|()| scope)
            .map_err(serde::de::Error::custom)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReference {
    scope: WorkspaceScope,
    resource: ResourceKind,
}

pub type ResourceRef = ResourceReference;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResourceReference {
    scope: WorkspaceScope,
    resource: ResourceKind,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "camelCase")]
enum RawResourceKindWire {
    Workspace,
    Browser(String),
    Target(String),
    Run(String),
    Revision(Revision),
    Entity(String),
    Memory(String),
    Workflow(String),
    Replay(String),
    Profile(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResourceReferenceWire {
    scope: RawWorkspaceScopeWire,
    resource: RawResourceKindWire,
}

impl<'de> Deserialize<'de> for ResourceReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawResourceReference::deserialize(deserializer)?;
        Self::new(raw.scope, raw.resource).map_err(serde::de::Error::custom)
    }
}

impl ResourceReference {
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        validate_wire_bytes(input)?;
        let raw: RawResourceReferenceWire = serde_json::from_str(input)?;
        let scope = WorkspaceScope::from_wire(raw.scope)?;
        let resource = match raw.resource {
            RawResourceKindWire::Workspace => ResourceKind::Workspace,
            RawResourceKindWire::Browser(id) => {
                ResourceKind::Browser(ResourceId::new(id).map_err(wire_error)?)
            }
            RawResourceKindWire::Target(id) => {
                ResourceKind::Target(ResourceId::new(id).map_err(wire_error)?)
            }
            RawResourceKindWire::Run(id) => {
                ResourceKind::Run(ResourceId::new(id).map_err(wire_error)?)
            }
            RawResourceKindWire::Revision(revision) => ResourceKind::Revision(revision),
            RawResourceKindWire::Entity(id) => {
                ResourceKind::Entity(ResourceId::new(id).map_err(wire_error)?)
            }
            RawResourceKindWire::Memory(id) => {
                ResourceKind::Memory(ResourceId::new(id).map_err(wire_error)?)
            }
            RawResourceKindWire::Workflow(id) => {
                ResourceKind::Workflow(ResourceId::new(id).map_err(wire_error)?)
            }
            RawResourceKindWire::Replay(id) => {
                ResourceKind::Replay(ResourceId::new(id).map_err(wire_error)?)
            }
            RawResourceKindWire::Profile(id) => {
                ResourceKind::Profile(ProfileId::new(id).map_err(wire_error)?)
            }
        };
        Self::new(scope, resource).map_err(wire_error)
    }
    pub fn new(scope: WorkspaceScope, resource: ResourceKind) -> Result<Self, ReferenceError> {
        scope.validate_invariants().map_err(ReferenceError::Scope)?;
        if scope.storage == WorkspaceStorage::Ephemeral && scope.generation.is_none() {
            return Err(ReferenceError::MissingGeneration);
        }
        if scope.generation.is_some_and(|generation| generation.0 == 0) {
            return Err(ReferenceError::InvalidGeneration);
        }
        match &resource {
            ResourceKind::Profile(profile_id) if scope.profile_id.as_ref() != Some(profile_id) => {
                return Err(ReferenceError::Scope(ScopeError::ProfileMismatch {
                    expected: scope.profile_id,
                    actual: Some(profile_id.clone()),
                }));
            }
            _ => {}
        }
        Ok(Self { scope, resource })
    }

    pub fn workspace(workspace_id: WorkspaceId) -> Self {
        Self {
            scope: WorkspaceScope::workspace(workspace_id),
            resource: ResourceKind::Workspace,
        }
    }
    pub fn profile(scope: WorkspaceScope, profile_id: ProfileId) -> Result<Self, ReferenceError> {
        if let Some(existing) = scope.profile_id.as_ref()
            && existing != &profile_id
        {
            return Err(ReferenceError::Scope(ScopeError::ProfileMismatch {
                expected: scope.profile_id,
                actual: Some(profile_id),
            }));
        }
        Self::new(
            WorkspaceScope {
                workspace_id: scope.workspace_id,
                profile_id: Some(profile_id.clone()),
                generation: scope.generation,
                storage: scope.storage,
            },
            ResourceKind::Profile(profile_id),
        )
    }
    pub fn browser(scope: WorkspaceScope, id: ResourceId) -> Result<Self, ReferenceError> {
        Self::new(scope, ResourceKind::Browser(id))
    }
    pub fn target(scope: WorkspaceScope, id: ResourceId) -> Result<Self, ReferenceError> {
        Self::new(scope, ResourceKind::Target(id))
    }
    pub fn run(scope: WorkspaceScope, id: ResourceId) -> Result<Self, ReferenceError> {
        Self::new(scope, ResourceKind::Run(id))
    }
    pub fn revision(scope: WorkspaceScope, revision: Revision) -> Result<Self, ReferenceError> {
        Self::new(scope, ResourceKind::Revision(revision))
    }
    pub fn entity(scope: WorkspaceScope, id: ResourceId) -> Result<Self, ReferenceError> {
        Self::new(scope, ResourceKind::Entity(id))
    }
    pub fn memory(scope: WorkspaceScope, id: ResourceId) -> Result<Self, ReferenceError> {
        Self::new(scope, ResourceKind::Memory(id))
    }
    pub fn workflow(scope: WorkspaceScope, id: ResourceId) -> Result<Self, ReferenceError> {
        Self::new(scope, ResourceKind::Workflow(id))
    }
    pub fn replay(scope: WorkspaceScope, id: ResourceId) -> Result<Self, ReferenceError> {
        Self::new(scope, ResourceKind::Replay(id))
    }

    pub fn validate_scope(&self, scope: &WorkspaceScope) -> Result<(), ScopeError> {
        self.scope.validate(scope)
    }
    pub fn scope(&self) -> &WorkspaceScope {
        &self.scope
    }
    pub fn resource(&self) -> &ResourceKind {
        &self.resource
    }

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
        if let Some(generation) = self.scope.generation {
            write!(formatter, "/generation/{generation}")?;
        }
        match &self.resource {
            ResourceKind::Workspace => {
                if let Some(profile_id) = &self.scope.profile_id {
                    write!(formatter, "/profile/{profile_id}/workspace")
                } else {
                    Ok(())
                }
            }
            ResourceKind::Profile(profile_id) => write!(formatter, "/profile/{profile_id}"),
            resource => {
                if let Some(profile_id) = &self.scope.profile_id {
                    write!(formatter, "/profile/{profile_id}")?;
                }
                write!(formatter, "/{}", self.kind_name())?;
                match resource {
                    ResourceKind::Revision(revision) => write!(formatter, "/{revision}"),
                    ResourceKind::Browser(id)
                    | ResourceKind::Target(id)
                    | ResourceKind::Run(id)
                    | ResourceKind::Entity(id)
                    | ResourceKind::Memory(id)
                    | ResourceKind::Workflow(id)
                    | ResourceKind::Replay(id) => write!(formatter, "/{id}"),
                    ResourceKind::Workspace | ResourceKind::Profile(_) => Ok(()),
                }
            }
        }
    }
}

impl FromStr for ResourceReference {
    type Err = ReferenceError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() > MAX_REFERENCE_URI_BYTES {
            return Err(ReferenceError::InvalidPath);
        }
        let rest = value
            .strip_prefix("glass://")
            .ok_or(ReferenceError::InvalidScheme)?;
        let pieces: Vec<_> = rest.split('/').collect();
        if pieces.len() < 2 || pieces.len() > MAX_REFERENCE_SEGMENTS || pieces[0] != "workspace" {
            return Err(ReferenceError::InvalidPath);
        }
        let workspace = WorkspaceId::new(pieces[1]).map_err(ReferenceError::InvalidName)?;
        let mut tail = &pieces[2..];
        let generation = if tail.first() == Some(&"generation") {
            if tail.len() < 2 {
                return Err(ReferenceError::InvalidPath);
            }
            let generation = tail[1]
                .parse::<u64>()
                .map_err(|_| ReferenceError::InvalidGeneration)?;
            tail = &tail[2..];
            Some(
                WorkspaceGeneration::new(generation)
                    .map_err(|_| ReferenceError::InvalidGeneration)?,
            )
        } else {
            None
        };
        let workspace_scope = WorkspaceScope::workspace(workspace).with_generation_opt(generation);
        if tail.is_empty() {
            return Self::new(workspace_scope, ResourceKind::Workspace);
        }
        if tail[0] == "profile" {
            if tail.len() == 2 {
                let profile_id = ProfileId::new(tail[1]).map_err(ReferenceError::InvalidName)?;
                return Self::new(
                    workspace_scope.with_profile(profile_id.clone()),
                    ResourceKind::Profile(profile_id),
                );
            }
            if tail.len() == 3 && tail[2] == "workspace" {
                let profile_id = ProfileId::new(tail[1]).map_err(ReferenceError::InvalidName)?;
                return Self::new(
                    workspace_scope.with_profile(profile_id),
                    ResourceKind::Workspace,
                );
            }
            if tail.len() != 4 {
                return Err(ReferenceError::InvalidPath);
            }
            let profile_id = ProfileId::new(tail[1]).map_err(ReferenceError::InvalidName)?;
            let resource = parse_scoped_kind(tail[2], tail[3])?;
            return Self::new(workspace_scope.with_profile(profile_id), resource);
        }
        if tail.len() != 2 {
            return Err(ReferenceError::InvalidPath);
        }
        let resource = parse_scoped_kind(tail[0], tail[1])?;
        Self::new(workspace_scope, resource)
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

fn parse_id(value: &str) -> Result<ResourceId, ReferenceError> {
    ResourceId::new(value).map_err(ReferenceError::InvalidName)
}
fn parse_revision(value: &str) -> Result<Revision, ReferenceError> {
    value
        .parse::<u64>()
        .map(Revision)
        .map_err(|_| ReferenceError::InvalidRevision)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(pub u64);

impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Revision {
    pub fn next(self) -> Result<Self, StaleRevisionError> {
        self.0.checked_add(1).map(Self).ok_or(StaleRevisionError {
            expected: self.0,
            actual: self.0,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActorRole {
    Human,
    Agent,
    Observer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentCapability {
    Observe,
    Mutate,
    Takeover,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttachmentCapabilities(BTreeSet<AttachmentCapability>);

impl AttachmentCapabilities {
    pub fn observer() -> Self {
        Self::default()
    }
    pub fn mutating() -> Self {
        Self(BTreeSet::from([
            AttachmentCapability::Observe,
            AttachmentCapability::Mutate,
        ]))
    }
    pub fn takeover() -> Self {
        Self(BTreeSet::from([
            AttachmentCapability::Observe,
            AttachmentCapability::Mutate,
            AttachmentCapability::Takeover,
        ]))
    }
    pub fn contains(&self, capability: AttachmentCapability) -> bool {
        self.0.contains(&capability)
    }
}

impl Default for AttachmentCapabilities {
    fn default() -> Self {
        Self(BTreeSet::from([AttachmentCapability::Observe]))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    id: AttachmentId,
    actor_id: ResourceId,
    role: ActorRole,
    capabilities: AttachmentCapabilities,
    scope: WorkspaceScope,
}

impl Attachment {
    pub fn new(
        id: AttachmentId,
        actor_id: ResourceId,
        role: ActorRole,
        capabilities: AttachmentCapabilities,
        scope: WorkspaceScope,
    ) -> Result<Self, LeaseError> {
        if role == ActorRole::Observer
            && (capabilities.contains(AttachmentCapability::Mutate)
                || capabilities.contains(AttachmentCapability::Takeover))
        {
            return Err(LeaseError::ObserverMutationDenied);
        }
        Ok(Self {
            id,
            actor_id,
            role,
            capabilities,
            scope,
        })
    }
    pub fn id(&self) -> &AttachmentId {
        &self.id
    }
    pub fn actor_id(&self) -> &ResourceId {
        &self.actor_id
    }
    pub fn role(&self) -> ActorRole {
        self.role
    }
    pub fn capabilities(&self) -> &AttachmentCapabilities {
        &self.capabilities
    }
    pub fn scope(&self) -> &WorkspaceScope {
        &self.scope
    }

    pub fn can_mutate(&self) -> bool {
        self.role != ActorRole::Observer && self.capabilities.contains(AttachmentCapability::Mutate)
    }
    pub fn can_takeover(&self) -> bool {
        self.can_mutate() && self.capabilities.contains(AttachmentCapability::Takeover)
    }
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        validate_wire_bytes(input)?;
        let raw: RawAttachmentWire = serde_json::from_str(input)?;
        Self::from_wire(raw)
    }
    fn from_wire(raw: RawAttachmentWire) -> Result<Self, serde_json::Error> {
        let id = AttachmentId::new(raw.id).map_err(wire_error)?;
        let actor_id = ResourceId::new(raw.actor_id).map_err(wire_error)?;
        let scope = WorkspaceScope::from_wire(raw.scope)?;
        Self::new(id, actor_id, raw.role, raw.capabilities, scope).map_err(wire_error)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAttachment {
    id: AttachmentId,
    actor_id: ResourceId,
    role: ActorRole,
    capabilities: AttachmentCapabilities,
    scope: WorkspaceScope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAttachmentWire {
    id: String,
    actor_id: String,
    role: ActorRole,
    capabilities: AttachmentCapabilities,
    scope: RawWorkspaceScopeWire,
}
impl<'de> Deserialize<'de> for Attachment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawAttachment::deserialize(deserializer)?;
        Self::new(raw.id, raw.actor_id, raw.role, raw.capabilities, raw.scope)
            .map_err(serde::de::Error::custom)
    }
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
pub enum OwnershipDomain {
    Workspace,
    Browser,
    Presentation,
    ExternalAttachment,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnershipBoundary {
    scope: WorkspaceScope,
    domain: OwnershipDomain,
    owner: OwnershipOwner,
}

impl OwnershipBoundary {
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        validate_wire_bytes(input)?;
        let raw: RawOwnershipBoundaryWire = serde_json::from_str(input)?;
        let scope = WorkspaceScope::from_wire(raw.scope)?;
        let owner = match raw.owner {
            RawOwnershipOwnerWire::Workspace(id) => {
                OwnershipOwner::Workspace(WorkspaceId::new(id).map_err(wire_error)?)
            }
            RawOwnershipOwnerWire::Attachment(id) => {
                OwnershipOwner::Attachment(AttachmentId::new(id).map_err(wire_error)?)
            }
            RawOwnershipOwnerWire::External(id) => {
                OwnershipOwner::External(ResourceId::new(id).map_err(wire_error)?)
            }
        };
        Self::new(scope, raw.domain, owner).map_err(wire_error)
    }
    pub fn new(
        scope: WorkspaceScope,
        domain: OwnershipDomain,
        owner: OwnershipOwner,
    ) -> Result<Self, OwnershipError> {
        let valid = match (&domain, &owner) {
            (OwnershipDomain::Workspace, OwnershipOwner::Workspace(id)) => {
                id == scope.workspace_id()
            }
            (
                OwnershipDomain::Browser | OwnershipDomain::Presentation,
                OwnershipOwner::Attachment(_),
            ) => true,
            (OwnershipDomain::ExternalAttachment, OwnershipOwner::External(_)) => true,
            _ => false,
        };
        if !valid {
            return Err(OwnershipError::DomainOwnerMismatch);
        }
        Ok(Self {
            scope,
            domain,
            owner,
        })
    }
    pub fn scope(&self) -> &WorkspaceScope {
        &self.scope
    }
    pub fn domain(&self) -> OwnershipDomain {
        self.domain
    }
    pub fn owner(&self) -> &OwnershipOwner {
        &self.owner
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOwnershipBoundary {
    scope: WorkspaceScope,
    domain: OwnershipDomain,
    owner: OwnershipOwner,
}
#[derive(Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "camelCase")]
enum RawOwnershipOwnerWire {
    Workspace(String),
    Attachment(String),
    External(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOwnershipBoundaryWire {
    scope: RawWorkspaceScopeWire,
    domain: OwnershipDomain,
    owner: RawOwnershipOwnerWire,
}
impl<'de> Deserialize<'de> for OwnershipBoundary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawOwnershipBoundary::deserialize(deserializer)?;
        Self::new(raw.scope, raw.domain, raw.owner).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MutationLeaseState {
    Available,
    Held {
        lease_id: LeaseId,
        holder: AttachmentId,
    },
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
impl Default for LeaseSnapshot {
    fn default() -> Self {
        Self {
            state: MutationLeaseState::Available,
            revision: Revision(0),
        }
    }
}
impl LeaseSnapshot {
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        validate_wire_bytes(input)?;
        let raw: RawLeaseSnapshotWire = serde_json::from_str(input)?;
        let state = match raw.state {
            RawLeaseStateWire::Available => MutationLeaseState::Available,
            RawLeaseStateWire::Held { lease_id, holder } => MutationLeaseState::Held {
                lease_id: LeaseId::new(lease_id).map_err(wire_error)?,
                holder: AttachmentId::new(holder).map_err(wire_error)?,
            },
        };
        Ok(Self {
            state,
            revision: raw.revision,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLeaseSnapshotWire {
    state: RawLeaseStateWire,
    revision: Revision,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum RawLeaseStateWire {
    Available,
    Held { lease_id: String, holder: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationLeaseAuthority {
    state: MutationLeaseState,
    revision: Revision,
}
fn parse_raw_lease(raw: Option<RawLeaseSnapshotWire>) -> Result<LeaseSnapshot, String> {
    let Some(raw) = raw else {
        return Ok(LeaseSnapshot::default());
    };
    let state = match raw.state {
        RawLeaseStateWire::Available => MutationLeaseState::Available,
        RawLeaseStateWire::Held { lease_id, holder } => MutationLeaseState::Held {
            lease_id: LeaseId::new(lease_id).map_err(|error| error.to_string())?,
            holder: AttachmentId::new(holder).map_err(|error| error.to_string())?,
        },
    };
    Ok(LeaseSnapshot {
        state,
        revision: raw.revision,
    })
}
fn validate_lease_holder(
    lease: &LeaseSnapshot,
    attachments: &BTreeMap<AttachmentId, Attachment>,
) -> Result<(), String> {
    if let MutationLeaseState::Held { holder, .. } = &lease.state
        && !attachments.contains_key(holder)
    {
        return Err(format!("mutation lease holder {holder} is not attached"));
    }
    Ok(())
}

impl Default for MutationLeaseAuthority {
    fn default() -> Self {
        Self {
            state: MutationLeaseState::Available,
            revision: Revision(0),
        }
    }
}
impl MutationLeaseAuthority {
    pub fn snapshot(&self) -> LeaseSnapshot {
        LeaseSnapshot {
            state: self.state.clone(),
            revision: self.revision,
        }
    }

    pub fn from_snapshot(snapshot: LeaseSnapshot) -> Self {
        Self {
            state: snapshot.state,
            revision: snapshot.revision,
        }
    }
    pub fn acquire(
        &mut self,
        attachment: &Attachment,
        expected: Revision,
    ) -> Result<LeaseGrant, LeaseError> {
        self.check_revision(expected)?;
        self.check_mutation(attachment)?;

        if matches!(self.state, MutationLeaseState::Held { .. }) {
            return Err(LeaseError::AlreadyHeld);
        }
        let revision = self.bump_revision()?;
        let lease_id =
            LeaseId::new(format!("lease-{}", revision.0)).map_err(|_| LeaseError::InvalidLease)?;
        self.state = MutationLeaseState::Held {
            lease_id: lease_id.clone(),
            holder: attachment.id.clone(),
        };
        Ok(LeaseGrant {
            lease_id,
            holder: attachment.id.clone(),
            revision,
        })
    }

    pub fn takeover(
        &mut self,
        attachment: &Attachment,
        expected: Revision,
    ) -> Result<LeaseGrant, LeaseError> {
        self.check_revision(expected)?;
        self.check_mutation(attachment)?;
        if !attachment.can_takeover() {
            return Err(LeaseError::TakeoverDenied);
        }
        if matches!(self.state, MutationLeaseState::Available) {
            return Err(LeaseError::NotHeld);
        }
        let revision = self.bump_revision()?;
        let lease_id =
            LeaseId::new(format!("lease-{}", revision.0)).map_err(|_| LeaseError::InvalidLease)?;
        self.state = MutationLeaseState::Held {
            lease_id: lease_id.clone(),
            holder: attachment.id.clone(),
        };
        Ok(LeaseGrant {
            lease_id,
            holder: attachment.id.clone(),
            revision,
        })
    }

    pub fn release(
        &mut self,
        attachment: &Attachment,
        lease_id: &LeaseId,
        expected: Revision,
    ) -> Result<Revision, LeaseError> {
        self.check_revision(expected)?;
        self.check_mutation(attachment)?;
        match &self.state {
            MutationLeaseState::Available => Err(LeaseError::NotHeld),
            MutationLeaseState::Held {
                lease_id: held_id, ..
            } if held_id != lease_id => Err(LeaseError::InvalidLease),
            MutationLeaseState::Held { holder, .. } if holder != &attachment.id => {
                Err(LeaseError::NotHolder)
            }
            MutationLeaseState::Held { .. } => {
                let revision = self.bump_revision()?;
                self.state = MutationLeaseState::Available;
                Ok(revision)
            }
        }
    }

    /// Remove a disconnected holder without requiring a stale client token.
    fn revoke_for(&mut self, attachment_id: &AttachmentId) -> Result<(), LeaseError> {
        if matches!(&self.state, MutationLeaseState::Held { holder, .. } if holder == attachment_id)
        {
            self.bump_revision()?;
            self.state = MutationLeaseState::Available;
        }
        Ok(())
    }

    fn check_revision(&self, expected: Revision) -> Result<(), LeaseError> {
        if expected != self.revision {
            return Err(LeaseError::StaleRevision(StaleRevisionError {
                expected: expected.0,
                actual: self.revision.0,
            }));
        }
        Ok(())
    }

    fn check_mutation(&self, attachment: &Attachment) -> Result<(), LeaseError> {
        if attachment.role == ActorRole::Observer {
            return Err(LeaseError::ObserverMutationDenied);
        }
        if !attachment.can_mutate() {
            return Err(LeaseError::CapabilityDenied);
        }
        Ok(())
    }

    fn bump_revision(&mut self) -> Result<Revision, LeaseError> {
        self.revision = self.revision.next().map_err(LeaseError::RevisionOverflow)?;
        Ok(self.revision)
    }
}
fn serialize_lease_authority<S>(
    authority: &MutationLeaseAuthority,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    authority.snapshot().serialize(serializer)
}

#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    identity: WorkspaceIdentity,
    config: WorkspaceConfig,
    lifecycle: WorkspaceLifecycle,
    #[serde(rename = "snapshotRevision", default)]
    snapshot_revision: u64,
    #[serde(default)]
    attachments: BTreeMap<AttachmentId, Attachment>,
    #[serde(rename = "lease", serialize_with = "serialize_lease_authority")]
    lease_authority: MutationLeaseAuthority,
    #[serde(skip)]
    persisted_revision: Option<u64>,
}
impl PartialEq for Workspace {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.config == other.config
            && self.lifecycle == other.lifecycle
            && self.snapshot_revision == other.snapshot_revision
            && self.attachments == other.attachments
            && self.lease_authority == other.lease_authority
    }
}

impl Eq for Workspace {}
impl Workspace {
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        validate_wire_bytes(input)?;
        let raw: RawWorkspaceWire = serde_json::from_str(input)?;
        let mut attachments = BTreeMap::new();
        if raw.attachments.0.len() > MAX_ATTACHMENTS {
            return Err(wire_error(WorkspaceError::TooManyAttachments {
                maximum: MAX_ATTACHMENTS,
            }));
        }
        for (key, raw_attachment) in raw.attachments.0 {
            let attachment = Attachment::from_wire(raw_attachment)?;
            let expected = attachment.id().to_string();
            if key != expected {
                return Err(wire_error(
                    "attachment map key does not match attachment id",
                ));
            }
            attachments.insert(attachment.id().clone(), attachment);
        }
        let identity = WorkspaceIdentity::from_wire(raw.identity)?;
        let profile_id = raw
            .config
            .profile_id
            .map(ProfileId::new)
            .transpose()
            .map_err(wire_error)?;
        let config = WorkspaceConfig {
            profile_mode: raw.config.profile_mode,
            privacy_mode: raw.config.privacy_mode,
            storage: raw.config.storage,
            profile_id,
            generation: raw.config.generation,
        };
        let mut workspace = Self::new(identity, config).map_err(wire_error)?;
        workspace.lifecycle = raw.lifecycle;
        workspace.snapshot_revision = raw.snapshot_revision;
        workspace.lease_authority =
            MutationLeaseAuthority::from_snapshot(parse_raw_lease(raw.lease).map_err(wire_error)?);
        let scope = WorkspaceScope {
            workspace_id: workspace.identity.id().clone(),
            profile_id: workspace.config.profile_id.clone(),
            generation: workspace.config.generation,
            storage: workspace.config.storage,
        };
        for attachment in attachments.values() {
            if attachment.scope() != &scope {
                return Err(wire_error(
                    "attachment scope does not match workspace scope",
                ));
            }
        }
        workspace.attachments = attachments;
        validate_lease_holder(&workspace.lease(), &workspace.attachments).map_err(wire_error)?;
        workspace.persisted_revision = Some(workspace.snapshot_revision);
        Ok(workspace)
    }
    pub fn new(
        identity: WorkspaceIdentity,
        config: WorkspaceConfig,
    ) -> Result<Self, WorkspaceError> {
        config.validate()?;
        Ok(Self {
            identity,
            config,
            lifecycle: WorkspaceLifecycle::Active,
            snapshot_revision: 0,
            attachments: BTreeMap::new(),
            lease_authority: MutationLeaseAuthority::default(),
            persisted_revision: None,
        })
    }
    pub fn identity(&self) -> &WorkspaceIdentity {
        &self.identity
    }
    pub fn config(&self) -> &WorkspaceConfig {
        &self.config
    }
    pub fn lifecycle(&self) -> WorkspaceLifecycle {
        self.lifecycle
    }
    pub fn attachments(&self) -> &BTreeMap<AttachmentId, Attachment> {
        &self.attachments
    }

    pub fn scope(&self) -> WorkspaceScope {
        WorkspaceScope {
            workspace_id: self.identity.id().clone(),
            profile_id: self.config.profile_id.clone(),
            generation: self.config.generation,
            storage: self.config.storage,
        }
    }

    /// Validate that a resource belongs to this workspace incarnation.
    pub fn validate_reference(&self, reference: &ResourceReference) -> Result<(), ScopeError> {
        self.scope().validate(reference.scope())
    }

    /// Return a workspace reference carrying profile and ephemeral generation scope.
    pub fn resource_reference(&self) -> ResourceReference {
        ResourceReference::new(self.scope(), ResourceKind::Workspace)
            .expect("validated workspace scope")
    }

    pub fn transition(&mut self, next: WorkspaceLifecycle) -> Result<(), LifecycleError> {
        if !self.lifecycle.can_transition_to(next) {
            return Err(LifecycleError::InvalidTransition {
                from: self.lifecycle,
                to: next,
            });
        }
        self.lifecycle = next;
        Ok(())
    }

    /// Reset runtime attachments and lease state while preserving workspace identity/config.
    pub fn reset(&mut self) -> Result<(), LifecycleError> {
        if matches!(
            self.lifecycle,
            WorkspaceLifecycle::Closing | WorkspaceLifecycle::Closed
        ) {
            return Err(LifecycleError::NotMutable {
                state: self.lifecycle,
            });
        }
        self.attachments.clear();
        self.lease_authority = MutationLeaseAuthority::default();
        self.lifecycle = WorkspaceLifecycle::Active;
        Ok(())
    }

    pub fn attach(&mut self, attachment: Attachment) -> Result<(), WorkspaceError> {
        self.ensure_mutable()?;
        self.scope()
            .validate(&attachment.scope)
            .map_err(WorkspaceError::Scope)?;
        if self.attachments.contains_key(&attachment.id) {
            return Err(WorkspaceError::DuplicateAttachment);
        }
        if self.attachments.len() >= MAX_ATTACHMENTS {
            return Err(WorkspaceError::TooManyAttachments {
                maximum: MAX_ATTACHMENTS,
            });
        }
        self.attachments.insert(attachment.id.clone(), attachment);
        Ok(())
    }
    /// Detaching an attachment revokes its lease but never closes the workspace.
    pub fn disconnect(&mut self, attachment_id: &AttachmentId) -> Result<(), WorkspaceError> {
        self.attachments
            .remove(attachment_id)
            .ok_or(WorkspaceError::UnknownAttachment)?;
        self.lease_authority
            .revoke_for(attachment_id)
            .map_err(WorkspaceError::Lease)
    }

    pub fn lease(&self) -> LeaseSnapshot {
        self.lease_authority.snapshot()
    }
    pub fn acquire_lease(
        &mut self,
        attachment_id: &AttachmentId,
        expected: Revision,
    ) -> Result<LeaseGrant, WorkspaceError> {
        self.ensure_mutable()?;
        let attachment = self
            .attachments
            .get(attachment_id)
            .ok_or(WorkspaceError::UnknownAttachment)?;
        self.lease_authority
            .acquire(attachment, expected)
            .map_err(WorkspaceError::Lease)
    }
    pub fn takeover_lease(
        &mut self,
        attachment_id: &AttachmentId,
        expected: Revision,
    ) -> Result<LeaseGrant, WorkspaceError> {
        self.ensure_mutable()?;
        let attachment = self
            .attachments
            .get(attachment_id)
            .ok_or(WorkspaceError::UnknownAttachment)?;
        self.lease_authority
            .takeover(attachment, expected)
            .map_err(WorkspaceError::Lease)
    }
    pub fn release_lease(
        &mut self,
        attachment_id: &AttachmentId,
        lease_id: &LeaseId,
        expected: Revision,
    ) -> Result<Revision, WorkspaceError> {
        self.ensure_mutable()?;
        let attachment = self
            .attachments
            .get(attachment_id)
            .ok_or(WorkspaceError::UnknownAttachment)?;
        self.lease_authority
            .release(attachment, lease_id, expected)
            .map_err(WorkspaceError::Lease)
    }

    fn ensure_mutable(&self) -> Result<(), WorkspaceError> {
        match self.lifecycle {
            WorkspaceLifecycle::Active | WorkspaceLifecycle::Suspended => Ok(()),
            state => Err(WorkspaceError::Lifecycle(LifecycleError::NotMutable {
                state,
            })),
        }
    }
}

#[derive(Default)]
struct BoundedAttachments(BTreeMap<AttachmentId, Attachment>);

impl<'de> Deserialize<'de> for BoundedAttachments {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AttachmentsVisitor;
        impl<'de> Visitor<'de> for AttachmentsVisitor {
            type Value = BoundedAttachments;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded attachment map")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut attachments = BTreeMap::new();
                while let Some(id) = map.next_key::<AttachmentId>()? {
                    if attachments.len() >= MAX_ATTACHMENTS {
                        return Err(serde::de::Error::custom(
                            WorkspaceError::TooManyAttachments {
                                maximum: MAX_ATTACHMENTS,
                            },
                        ));
                    }
                    if attachments.contains_key(&id) {
                        return Err(serde::de::Error::custom(
                            WorkspaceError::DuplicateAttachment,
                        ));
                    }
                    let attachment = map.next_value::<Attachment>()?;
                    attachments.insert(id, attachment);
                }
                Ok(BoundedAttachments(attachments))
            }
        }
        deserializer.deserialize_map(AttachmentsVisitor)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspace {
    identity: WorkspaceIdentity,
    config: WorkspaceConfig,
    lifecycle: WorkspaceLifecycle,
    #[serde(default)]
    snapshot_revision: u64,
    #[serde(default)]
    attachments: BoundedAttachments,
    #[serde(default)]
    lease: Option<RawLeaseSnapshotWire>,
}
impl<'de> Deserialize<'de> for Workspace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawWorkspace::deserialize(deserializer)?;
        if raw.attachments.0.len() > MAX_ATTACHMENTS {
            return Err(serde::de::Error::custom(
                WorkspaceError::TooManyAttachments {
                    maximum: MAX_ATTACHMENTS,
                },
            ));
        }
        let scope = WorkspaceScope {
            workspace_id: raw.identity.id().clone(),
            profile_id: raw.config.profile_id.clone(),
            generation: raw.config.generation,
            storage: raw.config.storage,
        };
        for (key, attachment) in &raw.attachments.0 {
            if key != &attachment.id || attachment.scope != scope {
                return Err(serde::de::Error::custom(WorkspaceError::Scope(
                    ScopeError::WorkspaceMismatch {
                        expected: scope.workspace_id.clone(),
                        actual: attachment.scope.workspace_id.clone(),
                    },
                )));
            }
            Attachment::new(
                attachment.id.clone(),
                attachment.actor_id.clone(),
                attachment.role,
                attachment.capabilities.clone(),
                attachment.scope.clone(),
            )
            .map_err(|error| serde::de::Error::custom(WorkspaceError::Lease(error)))?;
        }
        let mut workspace =
            Workspace::new(raw.identity, raw.config).map_err(serde::de::Error::custom)?;
        workspace.lifecycle = raw.lifecycle;
        workspace.snapshot_revision = raw.snapshot_revision;
        workspace.lease_authority = MutationLeaseAuthority::from_snapshot(
            parse_raw_lease(raw.lease).map_err(serde::de::Error::custom)?,
        );
        workspace.attachments = raw.attachments.0;
        validate_lease_holder(&workspace.lease(), &workspace.attachments)
            .map_err(serde::de::Error::custom)?;
        workspace.persisted_revision = Some(workspace.snapshot_revision);
        Ok(workspace)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspaceConfigWire {
    profile_mode: ProfileMode,
    privacy_mode: PrivacyMode,
    storage: WorkspaceStorage,
    #[serde(default)]
    profile_id: Option<String>,
    #[serde(default)]
    generation: Option<WorkspaceGeneration>,
}

#[derive(Default)]
struct BoundedWireAttachments(BTreeMap<String, RawAttachmentWire>);

impl<'de> Deserialize<'de> for BoundedWireAttachments {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WireAttachmentsVisitor;
        impl<'de> Visitor<'de> for WireAttachmentsVisitor {
            type Value = BoundedWireAttachments;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded attachment map")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut attachments = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if attachments.len() >= MAX_ATTACHMENTS {
                        return Err(serde::de::Error::custom(
                            WorkspaceError::TooManyAttachments {
                                maximum: MAX_ATTACHMENTS,
                            },
                        ));
                    }
                    if attachments.contains_key(&key) {
                        return Err(serde::de::Error::custom(
                            WorkspaceError::DuplicateAttachment,
                        ));
                    }
                    attachments.insert(key, map.next_value::<RawAttachmentWire>()?);
                }
                Ok(BoundedWireAttachments(attachments))
            }
        }
        deserializer.deserialize_map(WireAttachmentsVisitor)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspaceWire {
    identity: RawWorkspaceIdentityWire,
    config: RawWorkspaceConfigWire,
    lifecycle: WorkspaceLifecycle,
    #[serde(default)]
    snapshot_revision: u64,
    #[serde(default)]
    attachments: BoundedWireAttachments,
    #[serde(default)]
    lease: Option<RawLeaseSnapshotWire>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    InvalidName {
        label: &'static str,
        reason: &'static str,
    },
    InvalidGeneration,
    InvalidConfiguration,
    DuplicateAlias,
    DuplicateAttachment,
    TooManyAliases {
        maximum: usize,
    },
    TooManyAttachments {
        maximum: usize,
    },
    UnknownAttachment,
    Scope(ScopeError),
    Lifecycle(LifecycleError),
    Lease(LeaseError),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceError {
    InvalidScheme,
    InvalidPath,
    UnknownResource,
    InvalidRevision,
    InvalidGeneration,
    MissingGeneration,
    InvalidName(WorkspaceError),
    Scope(ScopeError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeError {
    WorkspaceMismatch {
        expected: WorkspaceId,
        actual: WorkspaceId,
    },
    ProfileMismatch {
        expected: Option<ProfileId>,
        actual: Option<ProfileId>,
    },
    GenerationMismatch {
        expected: Option<WorkspaceGeneration>,
        actual: Option<WorkspaceGeneration>,
    },
    StorageMismatch {
        expected: WorkspaceStorage,
        actual: WorkspaceStorage,
    },
    StorageGenerationMismatch {
        storage: WorkspaceStorage,
        generation: Option<WorkspaceGeneration>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleRevisionError {
    pub expected: u64,
    pub actual: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    InvalidTransition {
        from: WorkspaceLifecycle,
        to: WorkspaceLifecycle,
    },
    NotMutable {
        state: WorkspaceLifecycle,
    },
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
    Self::InvalidGeneration => "invalid workspace generation",
    Self::InvalidConfiguration => "invalid workspace profile/privacy/storage configuration",
    Self::DuplicateAlias => "workspace contains a duplicate alias",
    Self::DuplicateAttachment => "workspace already has this attachment",
    Self::TooManyAliases { .. } => "workspace has too many aliases",
    Self::TooManyAttachments { .. } => "workspace has too many attachments",
    Self::UnknownAttachment => "attachment is not connected to the workspace",
    Self::Scope(_) => "workspace scope rejected the operation",
    Self::Lifecycle(_) => "workspace lifecycle rejected the operation",
    Self::Lease(_) => "workspace lease operation failed"
});
display_error!(ReferenceError, {
    Self::InvalidScheme => "resource reference must use glass://",
    Self::InvalidPath => "invalid glass resource path",
    Self::UnknownResource => "unknown glass resource kind",
    Self::InvalidRevision => "invalid resource revision",
    Self::InvalidGeneration => "invalid resource generation",
    Self::MissingGeneration => "ephemeral references require a generation",
    Self::InvalidName(_) => "invalid resource identifier",
    Self::Scope(_) => "resource reference scope is invalid"
});
display_error!(ScopeError, {
    Self::WorkspaceMismatch { .. } => "resource belongs to another workspace",
    Self::ProfileMismatch { .. } => "resource belongs to another profile",
    Self::GenerationMismatch { .. } => "resource belongs to another workspace generation",
    Self::StorageMismatch { .. } => "resource belongs to another workspace storage mode",
    Self::StorageGenerationMismatch { .. } => "workspace storage and generation are inconsistent"
});
display_error!(StaleRevisionError, { Self { .. } => "revision is stale" });
display_error!(LifecycleError, {
    Self::InvalidTransition { .. } => "invalid workspace lifecycle transition",
    Self::NotMutable { .. } => "workspace is closing or closed",
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
/// Schema marker for durable workspace snapshots.
pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;
const WORKSPACE_FILE_SUFFIX: &str = ".json";
const WORKSPACE_LOCK_SUFFIX: &str = ".lock";

/// Durable, process-safe storage for named workspaces.
///
/// A workspace snapshot is written by rename, so readers never observe a
/// partially written document. Mutations take an advisory lock beside the
/// snapshot and fail explicitly when another coordinator owns it.
#[derive(Debug, Clone)]
pub struct WorkspaceStore {
    root: PathBuf,
}
/// Stable coordinator name for callers that do not need to know the storage
/// implementation details.
pub type WorkspaceCoordinator = WorkspaceStore;

#[derive(Debug)]
pub struct WorkspaceProfileLock {
    _file: File,
    pub workspace_id: WorkspaceId,
    pub profile_id: ProfileId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceStoreError {
    Io(String),
    Corrupt(String),
    InvalidName(WorkspaceError),
    AlreadyExists(WorkspaceId),
    NotFound(WorkspaceId),
    ProfileLocked(ProfileId),
    WorkspaceLocked(WorkspaceId),
    WorkspaceMismatch {
        expected: WorkspaceId,
        actual: WorkspaceId,
    },
    Lifecycle(LifecycleError),
    StaleSnapshot(WorkspaceId),
    UnpersistedWorkspace(WorkspaceId),
    Workspace(WorkspaceError),
}

impl fmt::Display for WorkspaceStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "workspace storage I/O error: {error}"),
            Self::Corrupt(error) => write!(formatter, "workspace snapshot is corrupt: {error}"),
            Self::InvalidName(error) => write!(formatter, "invalid workspace name: {error}"),
            Self::AlreadyExists(id) => write!(formatter, "workspace already exists: {id}"),
            Self::NotFound(id) => write!(formatter, "workspace not found: {id}"),
            Self::ProfileLocked(id) => write!(formatter, "profile is already owned: {id}"),
            Self::WorkspaceLocked(id) => write!(formatter, "workspace is already owned: {id}"),
            Self::WorkspaceMismatch { expected, actual } => write!(
                formatter,
                "workspace snapshot belongs to {actual}, expected {expected}"
            ),
            Self::Lifecycle(error) => error.fmt(formatter),
            Self::Workspace(error) => error.fmt(formatter),
            Self::StaleSnapshot(id) => write!(formatter, "workspace snapshot is stale: {id}"),
            Self::UnpersistedWorkspace(id) => {
                write!(formatter, "workspace has not been persisted: {id}")
            }
        }
    }
}
impl std::error::Error for WorkspaceStoreError {}
impl From<WorkspaceError> for WorkspaceStoreError {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}
impl From<LifecycleError> for WorkspaceStoreError {
    fn from(error: LifecycleError) -> Self {
        Self::Lifecycle(error)
    }
}

/// A loaded workspace together with its workspace/profile ownership guards.
/// Guards remain held for the session lifetime, preventing another runtime
/// from mutating or deleting the active workspace/profile.
#[derive(Debug)]
pub struct WorkspaceSession {
    workspace: Workspace,
    store: WorkspaceStore,
    _workspace_lock: File,
    _profile_lock: Option<WorkspaceProfileLock>,
}
impl WorkspaceSession {
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Mutate and persist this workspace while retaining its ownership guards.
    pub fn mutate<F>(&mut self, operation: F) -> Result<(), WorkspaceStoreError>
    where
        F: FnOnce(&mut Workspace) -> Result<(), WorkspaceStoreError>,
    {
        operation(&mut self.workspace)?;
        let previous_revision = self.workspace.snapshot_revision;
        self.workspace.snapshot_revision = next_snapshot_revision(previous_revision)?;
        if let Err(error) = self.store.write_unlocked(&self.workspace) {
            self.workspace.snapshot_revision = previous_revision;
            return Err(error);
        }
        self.workspace.persisted_revision = Some(self.workspace.snapshot_revision);
        Ok(())
    }
}
impl WorkspaceStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, WorkspaceStoreError> {
        let root = root.into();
        fs::create_dir_all(root.join("workspaces")).map_err(io_error)?;
        fs::create_dir_all(root.join("profiles")).map_err(io_error)?;
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn default_path() -> PathBuf {
        std::env::var_os("GLASS_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(dirs::config_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("glass")
    }

    pub fn open_default() -> Result<Self, WorkspaceStoreError> {
        Self::new(Self::default_path())
    }
    pub fn path_for(&self, id: &WorkspaceId) -> PathBuf {
        self.root
            .join("workspaces")
            .join(format!("{id}{WORKSPACE_FILE_SUFFIX}"))
    }
    fn lock_path_for(&self, id: &WorkspaceId) -> PathBuf {
        self.root
            .join("workspaces")
            .join(format!("{id}{WORKSPACE_LOCK_SUFFIX}"))
    }
    fn read_unlocked(&self, id: &WorkspaceId) -> Result<Workspace, WorkspaceStoreError> {
        let path = self.path_for(id);
        let bytes = fs::read(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                WorkspaceStoreError::NotFound(id.clone())
            } else {
                io_error(error)
            }
        })?;
        if bytes.len() > MAX_WIRE_BYTES {
            return Err(WorkspaceStoreError::Corrupt(
                "snapshot exceeds wire limit".into(),
            ));
        }
        let text = String::from_utf8(bytes)
            .map_err(|error| WorkspaceStoreError::Corrupt(error.to_string()))?;
        let workspace = Workspace::from_json(&text)
            .map_err(|error| WorkspaceStoreError::Corrupt(error.to_string()))?;
        if workspace.identity().id() != id {
            return Err(WorkspaceStoreError::WorkspaceMismatch {
                expected: id.clone(),
                actual: workspace.identity().id().clone(),
            });
        }
        Ok(workspace)
    }

    fn write_unlocked(&self, workspace: &Workspace) -> Result<(), WorkspaceStoreError> {
        let id = workspace.identity().id();
        let bytes = serde_json::to_vec(workspace)
            .map_err(|error| WorkspaceStoreError::Corrupt(error.to_string()))?;
        if bytes.len() > MAX_WIRE_BYTES {
            return Err(WorkspaceStoreError::Corrupt(
                "snapshot exceeds wire limit".into(),
            ));
        }
        atomic_write(&self.path_for(id), &bytes)
    }

    pub fn create(
        &self,
        id: WorkspaceId,
        aliases: impl IntoIterator<Item = WorkspaceAlias>,
        config: WorkspaceConfig,
    ) -> Result<Workspace, WorkspaceStoreError> {
        let identity = WorkspaceIdentity::new(id.clone(), aliases)?;
        let mut workspace = Workspace::new(identity, config)?;
        let _lock = self.lock_workspace(&id)?;
        let profile_lock = workspace
            .config()
            .profile_id
            .as_ref()
            .map(|profile_id| self.lock_profile(&id, profile_id))
            .transpose()?;
        if self.path_for(&id).exists() {
            return Err(WorkspaceStoreError::AlreadyExists(id));
        }
        self.write_unlocked(&workspace)?;
        workspace.persisted_revision = Some(workspace.snapshot_revision);
        drop(profile_lock);
        FileExt::unlock(&_lock).map_err(io_error)?;
        Ok(workspace)
    }

    pub fn open(&self, id: &WorkspaceId) -> Result<Workspace, WorkspaceStoreError> {
        let lock = self.lock_workspace(id)?;
        let workspace = self.read_unlocked(id);
        FileExt::unlock(&lock).map_err(io_error)?;
        workspace
    }
    pub fn open_owned(&self, id: &WorkspaceId) -> Result<WorkspaceSession, WorkspaceStoreError> {
        let workspace_lock = self.lock_workspace(id)?;
        let workspace = self.read_unlocked(id)?;
        let profile_lock = workspace
            .config()
            .profile_id
            .as_ref()
            .map(|profile_id| self.lock_profile(id, profile_id))
            .transpose()?;
        Ok(WorkspaceSession {
            workspace,
            store: self.clone(),
            _workspace_lock: workspace_lock,
            _profile_lock: profile_lock,
        })
    }
    pub fn create_owned(
        &self,
        id: WorkspaceId,
        aliases: impl IntoIterator<Item = WorkspaceAlias>,
        config: WorkspaceConfig,
    ) -> Result<WorkspaceSession, WorkspaceStoreError> {
        let identity = WorkspaceIdentity::new(id.clone(), aliases)?;
        let _workspace_lock = self.lock_workspace(&id)?;
        let mut workspace = Workspace::new(identity, config)?;
        let _profile_lock = workspace
            .config()
            .profile_id
            .as_ref()
            .map(|profile_id| self.lock_profile(&id, profile_id))
            .transpose()?;
        if self.path_for(&id).exists() {
            return Err(WorkspaceStoreError::AlreadyExists(id));
        }
        self.write_unlocked(&workspace)?;
        workspace.persisted_revision = Some(workspace.snapshot_revision);
        Ok(WorkspaceSession {
            workspace,
            store: self.clone(),
            _workspace_lock,
            _profile_lock,
        })
    }
    pub fn save(&self, workspace: &mut Workspace) -> Result<(), WorkspaceStoreError> {
        let id = workspace.identity().id();
        let _lock = self.lock_workspace(id)?;
        let _profile_lock = workspace
            .config()
            .profile_id
            .as_ref()
            .map(|profile_id| self.lock_profile(id, profile_id))
            .transpose()?;
        let Some(expected) = workspace.persisted_revision else {
            return Err(WorkspaceStoreError::UnpersistedWorkspace(id.clone()));
        };
        let current_bytes = fs::read(self.path_for(id)).map_err(io_error)?;
        let current = Workspace::from_json(
            std::str::from_utf8(&current_bytes)
                .map_err(|error| WorkspaceStoreError::Corrupt(error.to_string()))?,
        )
        .map_err(|error| WorkspaceStoreError::Corrupt(error.to_string()))?;
        if current.snapshot_revision != expected {
            return Err(WorkspaceStoreError::StaleSnapshot(id.clone()));
        }
        let previous_revision = workspace.snapshot_revision;
        workspace.snapshot_revision = next_snapshot_revision(previous_revision)?;
        if let Err(error) = self.write_unlocked(workspace) {
            workspace.snapshot_revision = previous_revision;
            return Err(error);
        }
        workspace.persisted_revision = Some(workspace.snapshot_revision);
        drop(_profile_lock);
        FileExt::unlock(&_lock).map_err(io_error)?;
        Ok(())
    }

    pub fn update<F>(
        &self,
        id: &WorkspaceId,
        operation: F,
    ) -> Result<Workspace, WorkspaceStoreError>
    where
        F: FnOnce(&mut Workspace) -> Result<(), WorkspaceStoreError>,
    {
        let _lock = self.lock_workspace(id)?;
        let mut workspace = self.read_unlocked(id)?;
        let _profile_lock = workspace
            .config()
            .profile_id
            .as_ref()
            .map(|profile_id| self.lock_profile(id, profile_id))
            .transpose()?;
        operation(&mut workspace)?;
        let previous_revision = workspace.snapshot_revision;
        workspace.snapshot_revision = next_snapshot_revision(previous_revision)?;
        if let Err(error) = self.write_unlocked(&workspace) {
            workspace.snapshot_revision = previous_revision;
            return Err(error);
        }
        workspace.persisted_revision = Some(workspace.snapshot_revision);
        drop(_profile_lock);
        FileExt::unlock(&_lock).map_err(io_error)?;
        Ok(workspace)
    }

    pub fn list(&self) -> Result<Vec<WorkspaceId>, WorkspaceStoreError> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(self.root.join("workspaces")).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let id = WorkspaceId::new(name).map_err(WorkspaceStoreError::InvalidName)?;
            self.open(&id)?;
            ids.push(id);
        }
        ids.sort();
        Ok(ids)
    }

    pub fn clone_workspace(
        &self,
        source: &WorkspaceId,
        target: WorkspaceId,
    ) -> Result<Workspace, WorkspaceStoreError> {
        let source_workspace = self.open(source)?;
        let config = match source_workspace.config().storage {
            WorkspaceStorage::Durable => WorkspaceConfig::durable_named(
                ProfileId::new(target.as_str()).map_err(WorkspaceStoreError::InvalidName)?,
            ),
            WorkspaceStorage::Ephemeral => WorkspaceConfig::ephemeral_private(None),
        };
        self.create(target, std::iter::empty(), config)
    }

    pub fn suspend(&self, id: &WorkspaceId) -> Result<Workspace, WorkspaceStoreError> {
        self.update(id, |workspace| {
            workspace.transition(WorkspaceLifecycle::Suspended)?;
            Ok(())
        })
    }
    pub fn resume(&self, id: &WorkspaceId) -> Result<Workspace, WorkspaceStoreError> {
        self.update(id, |workspace| {
            workspace.transition(WorkspaceLifecycle::Active)?;
            Ok(())
        })
    }
    pub fn reset(&self, id: &WorkspaceId) -> Result<Workspace, WorkspaceStoreError> {
        self.update(id, |workspace| {
            workspace.reset()?;
            Ok(())
        })
    }
    pub fn delete(&self, id: &WorkspaceId) -> Result<(), WorkspaceStoreError> {
        let lock = self.lock_workspace(id)?;
        let mut workspace = self.read_unlocked(id)?;
        let _profile_lock = workspace
            .config()
            .profile_id
            .as_ref()
            .map(|profile_id| self.lock_profile(id, profile_id))
            .transpose()?;
        if workspace.lifecycle() != WorkspaceLifecycle::Closed {
            if workspace.lifecycle() != WorkspaceLifecycle::Closing {
                workspace.transition(WorkspaceLifecycle::Closing)?;
            }
            workspace.transition(WorkspaceLifecycle::Closed)?;
        }
        let path = self.path_for(id);
        fs::remove_file(&path).map_err(io_error)?;
        let parent = path
            .parent()
            .ok_or_else(|| WorkspaceStoreError::Io("workspace path has no parent".into()))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(io_error)?;
        drop(_profile_lock);
        FileExt::unlock(&lock).map_err(io_error)?;
        Ok(())
    }
    pub fn lock_workspace(&self, id: &WorkspaceId) -> Result<File, WorkspaceStoreError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.lock_path_for(id))
            .map_err(io_error)?;
        file.try_lock_exclusive()
            .map_err(|_| WorkspaceStoreError::WorkspaceLocked(id.clone()))?;
        Ok(file)
    }
    pub fn lock_profile(
        &self,
        workspace_id: &WorkspaceId,
        profile_id: &ProfileId,
    ) -> Result<WorkspaceProfileLock, WorkspaceStoreError> {
        let path = self
            .root
            .join("profiles")
            .join(format!("{profile_id}.lock"));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(io_error)?;
        file.try_lock_exclusive()
            .map_err(|_| WorkspaceStoreError::ProfileLocked(profile_id.clone()))?;
        Ok(WorkspaceProfileLock {
            _file: file,
            workspace_id: workspace_id.clone(),
            profile_id: profile_id.clone(),
        })
    }
}

fn io_error(error: io::Error) -> WorkspaceStoreError {
    WorkspaceStoreError::Io(error.to_string())
}

fn next_snapshot_revision(current: u64) -> Result<u64, WorkspaceStoreError> {
    current
        .checked_add(1)
        .ok_or_else(|| WorkspaceStoreError::Corrupt("workspace snapshot revision overflow".into()))
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), WorkspaceStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| WorkspaceStoreError::Io("workspace path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(io_error)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace");
    let temp_prefix = format!(".{file_name}.tmp-");
    for entry in fs::read_dir(parent).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|name| name.starts_with(&temp_prefix))
        {
            let _ = fs::remove_file(entry.path());
        }
    }
    let tmp = parent.join(format!(
        "{temp_prefix}{}",
        NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    drop(file);
    fs::rename(&tmp, path).map_err(|error| {
        let _ = fs::remove_file(&tmp);
        io_error(error)
    })?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnershipError {
    DomainOwnerMismatch,
}
impl fmt::Display for OwnershipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ownership domain and owner do not match")
    }
}

impl std::error::Error for OwnershipError {}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_normalized_and_bounded() {
        assert_eq!(
            WorkspaceId::new(" Demo_Workspace ").unwrap().as_str(),
            "demo-workspace"
        );
        assert!(WorkspaceId::new("!").is_err());
        assert!(WorkspaceId::new("a".repeat(MAX_ID_BYTES + 1)).is_err());
    }

    #[test]
    fn resource_reference_round_trips() {
        let scope = WorkspaceScope::profile(
            WorkspaceId::new("Demo").unwrap(),
            ProfileId::new("Private").unwrap(),
        );
        let reference =
            ResourceReference::workflow(scope, ResourceId::new("run-1").unwrap()).unwrap();
        let encoded = reference.to_string();
        assert_eq!(
            encoded,
            "glass://workspace/demo/profile/private/workflow/run-1"
        );
        assert_eq!(encoded.parse::<ResourceReference>().unwrap(), reference);
    }

    #[test]
    fn lifecycle_and_disconnect_are_explicit() {
        let identity = WorkspaceIdentity::new(WorkspaceId::new("demo").unwrap(), []).unwrap();
        let mut workspace =
            Workspace::new(identity, WorkspaceConfig::ephemeral_private(None)).unwrap();
        let scope = workspace.scope();
        let attachment = Attachment::new(
            AttachmentId::new("human").unwrap(),
            ResourceId::new("actor").unwrap(),
            ActorRole::Human,
            AttachmentCapabilities::mutating(),
            scope,
        )
        .unwrap();
        workspace.attach(attachment.clone()).unwrap();
        workspace.disconnect(attachment.id()).unwrap();
        assert_eq!(workspace.lifecycle(), WorkspaceLifecycle::Active);
        workspace.transition(WorkspaceLifecycle::Closing).unwrap();
        assert!(matches!(
            workspace.attach(attachment),
            Err(WorkspaceError::Lifecycle(LifecycleError::NotMutable { .. }))
        ));
        workspace.transition(WorkspaceLifecycle::Closed).unwrap();
        assert!(workspace.transition(WorkspaceLifecycle::Active).is_err());
    }

    #[test]
    fn lease_arbitration_and_stale_revision_are_guarded() {
        let scope = WorkspaceScope::workspace(WorkspaceId::new("lease-test").unwrap());
        let human = Attachment::new(
            AttachmentId::new("human").unwrap(),
            ResourceId::new("h").unwrap(),
            ActorRole::Human,
            AttachmentCapabilities::takeover(),
            scope.clone(),
        )
        .unwrap();
        let agent = Attachment::new(
            AttachmentId::new("agent").unwrap(),
            ResourceId::new("a").unwrap(),
            ActorRole::Agent,
            AttachmentCapabilities::takeover(),
            scope.clone(),
        )
        .unwrap();
        let observer = Attachment::new(
            AttachmentId::new("observer").unwrap(),
            ResourceId::new("o").unwrap(),
            ActorRole::Observer,
            AttachmentCapabilities::observer(),
            scope,
        )
        .unwrap();
        let mut authority = MutationLeaseAuthority::default();
        let grant = authority.acquire(&human, Revision(0)).unwrap();
        assert!(matches!(
            authority.acquire(&agent, grant.revision),
            Err(LeaseError::AlreadyHeld)
        ));
        assert!(matches!(
            authority.acquire(&observer, grant.revision),
            Err(LeaseError::ObserverMutationDenied)
        ));
        assert!(matches!(
            authority.takeover(&agent, Revision(0)),
            Err(LeaseError::StaleRevision(_))
        ));
        let replacement = authority.takeover(&agent, grant.revision).unwrap();
        assert_eq!(replacement.holder, agent.id);
        assert!(matches!(
            authority.release(&human, &replacement.lease_id, replacement.revision),
            Err(LeaseError::NotHolder)
        ));
        authority
            .release(&agent, &replacement.lease_id, replacement.revision)
            .unwrap();
    }
}
#[cfg(test)]
mod persistence_tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "glass-workspace-{name}-{}-{}",
            std::process::id(),
            NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn workspace_constructor_rejects_invalid_configuration() {
        let identity =
            WorkspaceIdentity::new(WorkspaceId::new("invalid-config").unwrap(), []).unwrap();
        let config = WorkspaceConfig {
            profile_mode: ProfileMode::Named,
            privacy_mode: PrivacyMode::Private,
            storage: WorkspaceStorage::Durable,
            profile_id: Some(ProfileId::new("invalid-config").unwrap()),
            generation: None,
        };
        assert!(Workspace::new(identity, config).is_err());
    }

    #[test]
    fn named_workspace_lifecycle_is_persisted_atomically() {
        let root = test_root("lifecycle");
        let store = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("research").unwrap();
        store
            .create(
                id.clone(),
                [],
                WorkspaceConfig::durable_named(ProfileId::new("research").unwrap()),
            )
            .unwrap();
        assert_eq!(store.list().unwrap(), vec![id.clone()]);
        assert_eq!(
            store.suspend(&id).unwrap().lifecycle(),
            WorkspaceLifecycle::Suspended
        );
        assert_eq!(
            store.resume(&id).unwrap().lifecycle(),
            WorkspaceLifecycle::Active
        );
        assert_eq!(
            store.reset(&id).unwrap().lifecycle(),
            WorkspaceLifecycle::Active
        );
        let clone = store
            .clone_workspace(&id, WorkspaceId::new("sandbox").unwrap())
            .unwrap();
        assert_eq!(
            clone.config().profile_id.as_ref().unwrap().as_str(),
            "sandbox"
        );
        store.delete(&id).unwrap();
        let deleted = store.open(&id);
        assert!(
            matches!(deleted, Err(WorkspaceStoreError::NotFound(_))),
            "deleted workspace remained observable: {deleted:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn profile_locks_are_exclusive_but_workspace_locks_are_independent() {
        let root = test_root("locks");
        let first = WorkspaceStore::new(&root).unwrap();
        let second = WorkspaceStore::new(&root).unwrap();
        let workspace_a = WorkspaceId::new("one").unwrap();
        let workspace_b = WorkspaceId::new("two").unwrap();
        let profile = ProfileId::new("shared").unwrap();
        let _workspace_guard = first.lock_workspace(&workspace_a).unwrap();
        let workspace_error = second.lock_workspace(&workspace_a).unwrap_err();
        assert_eq!(
            workspace_error.to_string(),
            "workspace is already owned: one"
        );
        let _first = first.lock_profile(&workspace_a, &profile).unwrap();
        assert!(matches!(
            second.create(
                workspace_b.clone(),
                [],
                WorkspaceConfig::durable_named(profile.clone())
            ),
            Err(WorkspaceStoreError::ProfileLocked(_))
        ));
        assert!(matches!(
            second.lock_profile(&workspace_b, &profile),
            Err(WorkspaceStoreError::ProfileLocked(_))
        ));
        let other = ProfileId::new("isolated").unwrap();
        let _second = second.lock_profile(&workspace_b, &other).unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resource_and_attachment_scope_validation_survives_reload() {
        let root = test_root("scope");
        let store = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("bounded").unwrap();
        let mut workspace = store
            .create(
                id.clone(),
                [],
                WorkspaceConfig::durable_named(ProfileId::new("bounded").unwrap()),
            )
            .unwrap();
        let scope = workspace.scope();
        let attachment = Attachment::new(
            AttachmentId::new("tui").unwrap(),
            ResourceId::new("actor").unwrap(),
            ActorRole::Human,
            AttachmentCapabilities::mutating(),
            scope.clone(),
        )
        .unwrap();
        workspace.attach(attachment).unwrap();
        store.save(&mut workspace).unwrap();
        let loaded = store.open(&id).unwrap();
        assert_eq!(loaded.attachments().len(), 1);
        let foreign = ResourceReference::target(
            WorkspaceScope::profile(
                WorkspaceId::new("other").unwrap(),
                ProfileId::new("other").unwrap(),
            ),
            ResourceId::new("tab").unwrap(),
        )
        .unwrap();
        assert!(loaded.validate_reference(&foreign).is_err());
        let own_reference = loaded.resource_reference();
        assert_eq!(
            own_reference
                .to_string()
                .parse::<ResourceReference>()
                .unwrap(),
            own_reference
        );
        assert!(loaded.validate_reference(&own_reference).is_ok());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn profile_lock_blocks_unowned_mutation() {
        let root = test_root("profile-mutation");
        let owner = WorkspaceStore::new(&root).unwrap();
        let contender = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("owned").unwrap();
        let profile = ProfileId::new("shared").unwrap();
        owner
            .create(
                id.clone(),
                [],
                WorkspaceConfig::durable_named(profile.clone()),
            )
            .unwrap();
        let _profile_guard = owner.lock_profile(&id, &profile).unwrap();
        assert!(matches!(
            contender.suspend(&id),
            Err(WorkspaceStoreError::ProfileLocked(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mutation_lease_survives_workspace_reload() {
        let root = test_root("lease");
        let store = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("leased").unwrap();
        let mut workspace = store
            .create(
                id.clone(),
                [],
                WorkspaceConfig::durable_named(ProfileId::new("leased").unwrap()),
            )
            .unwrap();

        let attachment = Attachment::new(
            AttachmentId::new("human").unwrap(),
            ResourceId::new("actor").unwrap(),
            ActorRole::Human,
            AttachmentCapabilities::mutating(),
            workspace.scope(),
        )
        .unwrap();
        workspace.attach(attachment.clone()).unwrap();
        let grant = workspace
            .acquire_lease(attachment.id(), Revision(0))
            .unwrap();
        store.save(&mut workspace).unwrap();
        assert_eq!(
            store.open(&id).unwrap().lease().state,
            MutationLeaseState::Held {
                lease_id: grant.lease_id,
                holder: attachment.id().clone()
            }
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn owned_session_retains_profile_lock_until_drop() {
        let root = test_root("owned-session");
        let first = WorkspaceStore::new(&root).unwrap();
        let second = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("owned").unwrap();
        let profile = ProfileId::new("owned").unwrap();
        let session = first
            .create_owned(
                id.clone(),
                [],
                WorkspaceConfig::durable_named(profile.clone()),
            )
            .unwrap();
        assert!(matches!(
            first.delete(&id),
            Err(WorkspaceStoreError::WorkspaceLocked(_))
        ));
        assert!(matches!(
            second.lock_profile(&id, &profile),
            Err(WorkspaceStoreError::ProfileLocked(_))
        ));
        drop(session);
        let _released = second.lock_profile(&id, &profile).unwrap();
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn owned_session_mutation_persists_under_origin_store() {
        let root = test_root("owned-mutate");
        let store = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("mutate").unwrap();
        let mut session = store
            .create_owned(
                id.clone(),
                [],
                WorkspaceConfig::durable_named(ProfileId::new("mutate").unwrap()),
            )
            .unwrap();
        session
            .mutate(|workspace| {
                workspace.transition(WorkspaceLifecycle::Suspended)?;
                Ok(())
            })
            .unwrap();
        drop(session);
        assert_eq!(
            store.open(&id).unwrap().lifecycle(),
            WorkspaceLifecycle::Suspended
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn stale_workspace_save_is_rejected_after_concurrent_update() {
        let root = test_root("stale-save");
        let store = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("stale").unwrap();
        store
            .create(
                id.clone(),
                [],
                WorkspaceConfig::durable_named(ProfileId::new("stale").unwrap()),
            )
            .unwrap();
        let mut first = store.open(&id).unwrap();
        let mut second = store.open(&id).unwrap();
        first.transition(WorkspaceLifecycle::Suspended).unwrap();
        store.save(&mut first).unwrap();
        assert_eq!(store.open(&id).unwrap().snapshot_revision, 1);
        second.transition(WorkspaceLifecycle::Suspended).unwrap();
        assert!(matches!(
            store.save(&mut second),
            Err(WorkspaceStoreError::StaleSnapshot(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn coordinator_profile_lock_uses_canonical_profile_manager_path() {
        let root = test_root("profile-contention");
        let store = WorkspaceStore::new(&root).unwrap();
        let workspace_id = WorkspaceId::new("owner").unwrap();
        let profile_id = ProfileId::new("shared").unwrap();
        let _guard = store.lock_profile(&workspace_id, &profile_id).unwrap();
        let path = root.join("profiles").join("shared.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        assert!(file.try_lock_exclusive().is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn list_rejects_invalid_snapshots_and_recovers_stale_temps() {
        let root = test_root("recovery");
        let store = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("stable").unwrap();
        store
            .create(
                id.clone(),
                [],
                WorkspaceConfig::durable_named(ProfileId::new("stable").unwrap()),
            )
            .unwrap();
        let workspaces = root.join("workspaces");
        fs::write(workspaces.join("invalid.json"), b"not-json").unwrap();
        fs::write(workspaces.join(".stable.json.tmp-stale"), b"partial").unwrap();
        assert!(matches!(store.list(), Err(WorkspaceStoreError::Corrupt(_))));
        fs::remove_file(workspaces.join("invalid.json")).unwrap();
        let mut loaded = store.open(&id).unwrap();
        store.save(&mut loaded).unwrap();
        assert!(!workspaces.join(".stable.json.tmp-stale").exists());
        assert_eq!(store.list().unwrap(), vec![id]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ghost_lease_holder_is_rejected_on_load() {
        let json = r#"{"identity":{"id":"ghost","aliases":[]},"config":{"profileMode":"named","privacyMode":"standard","storage":"durable","profileId":"ghost"},"lifecycle":"active","attachments":{},"lease":{"state":{"held":{"leaseId":"lease-1","holder":"missing"}},"revision":1}}"#;
        assert!(Workspace::from_json(json).is_err());
    }

    #[test]
    fn ephemeral_workspace_reference_carries_generation_scope() {
        let root = test_root("generation");
        let store = WorkspaceStore::new(&root).unwrap();
        let id = WorkspaceId::new("temporary").unwrap();
        let workspace = store
            .create(id, [], WorkspaceConfig::ephemeral_private(None))
            .unwrap();
        let reference = workspace.resource_reference();
        assert_eq!(
            reference.scope().generation(),
            workspace.scope().generation()
        );
        assert_eq!(
            reference.to_string().parse::<ResourceReference>().unwrap(),
            reference
        );
        let _ = fs::remove_dir_all(root);
    }
}
