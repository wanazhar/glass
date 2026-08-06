//! Transport-neutral browser capability contracts.
//!
//! This module deliberately speaks only in Glass semantic terms.  A backend
//! adapter may translate these requests to CDP, WebDriver BiDi, or another
//! transport, but transport identifiers and command/domain types must not cross
//! this boundary.

use serde::{Deserialize, Serialize};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::future::Future;
use std::pin::Pin;

pub const BROWSER_BACKEND_SCHEMA_VERSION: u32 = 1;
pub const MAX_BACKEND_ID_BYTES: usize = 128;
pub const MAX_VERSION_BYTES: usize = 128;
pub const MAX_BROWSER_FAMILY_BYTES: usize = 64;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_DEPENDENCIES: usize = 16;
pub const MAX_LIMITATIONS: usize = 16;
pub const MAX_LIMITATION_BYTES: usize = 256;
pub const MAX_SELECTION_REQUIREMENTS: usize = 32;
pub const MAX_BACKEND_CANDIDATES: usize = 32;
pub const MAX_DIAGNOSTIC_BYTES: usize = 512;
pub const MAX_CONTEXTS: usize = 64;
pub const MAX_STORAGE_ENTRIES: usize = 128;
pub const MAX_DOWNLOADS: usize = 64;
pub const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_JSON_BYTES: usize = 64 * 1024;
pub const MAX_JSON_DEPTH: usize = 16;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;

fn validate_text(field: &str, value: &str, max: usize) -> Result<(), BrowserBackendError> {
    if value.is_empty() {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: field.into(),
            reason: "must not be empty".into(),
        });
    }
    if value.len() > max {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: field.into(),
            reason: format!("must be at most {max} UTF-8 bytes"),
        });
    }
    if !value.is_char_boundary(value.len()) {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: field.into(),
            reason: "must be valid UTF-8".into(),
        });
    }
    Ok(())
}

fn validate_vec_len(field: &str, len: usize, max: usize) -> Result<(), BrowserBackendError> {
    if len > max {
        return Err(invalid(field, &format!("must contain at most {max} entries")));
    }
    Ok(())
}

fn validate_json(field: &str, value: &serde_json::Value) -> Result<(), BrowserBackendError> {
    let bytes = serde_json::to_vec(value).map_err(|error| invalid(field, &error.to_string()))?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(invalid(field, "serialized JSON exceeds the bounded payload size"));
    }
    Ok(())
}
struct BoundedStringVisitor;

impl<'de> Visitor<'de> for BoundedStringVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded UTF-8 string")
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(value)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.is_empty() {
            return Err(E::custom("string must not be empty"));
        }
        if value.len() > MAX_TEXT_BYTES {
            return Err(E::custom("string exceeds the bounded payload size"));
        }
        Ok(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.is_empty() {
            return Err(E::custom("string must not be empty"));
        }
        if value.len() > MAX_TEXT_BYTES {
            return Err(E::custom("string exceeds the bounded payload size"));
        }
        Ok(value)
    }
}

fn deserialize_bounded_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_str(BoundedStringVisitor)
}

struct BoundedOptionVisitor;

impl<'de> Visitor<'de> for BoundedOptionVisitor {
    type Value = Option<String>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an optional bounded UTF-8 string")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_bounded_string(deserializer).map(Some)
    }
}

fn deserialize_bounded_string_option<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_option(BoundedOptionVisitor)
}


struct BoundedVecVisitor<T, const LIMIT: usize>(std::marker::PhantomData<T>);

impl<'de, T, const LIMIT: usize> Visitor<'de> for BoundedVecVisitor<T, LIMIT>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded sequence")
    }

    fn visit_seq<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(access.size_hint().unwrap_or(0).min(LIMIT));
        while values.len() < LIMIT {
            let Some(value) = access.next_element()? else {
                return Ok(values);
            };
            values.push(value);
        }
        if access.next_element::<T>()?.is_some() {
            return Err(serde::de::Error::custom("vector exceeds the bounded entry count"));
        }
        Ok(values)
    }
}

fn deserialize_bounded_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::<T, MAX_CONTEXTS>(std::marker::PhantomData))
}

struct BoundedBytesVisitor;

impl<'de> Visitor<'de> for BoundedBytesVisitor {
    type Value = Vec<u8>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("bounded bytes")
    }

    fn visit_borrowed_bytes<E>(self, value: &'de [u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_bytes(value)
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.len() > MAX_CAPTURE_BYTES {
            return Err(E::custom("capture exceeds the bounded payload size"));
        }
        Ok(value.to_vec())
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.len() > MAX_CAPTURE_BYTES {
            return Err(E::custom("capture exceeds the bounded payload size"));
        }
        Ok(value)
    }

    fn visit_seq<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(access.size_hint().unwrap_or(0).min(MAX_CAPTURE_BYTES));
        while values.len() < MAX_CAPTURE_BYTES {
            let Some(value) = access.next_element::<u8>()? else {
                return Ok(values);
            };
            values.push(value);
        }
        if access.next_element::<u8>()?.is_some() {
            return Err(serde::de::Error::custom("capture exceeds the bounded payload size"));
        }
        Ok(values)
    }
}

fn deserialize_bounded_bytes<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_bytes(BoundedBytesVisitor)
}

struct JsonStringSeed;

impl<'de> DeserializeSeed<'de> for JsonStringSeed {
    type Value = String;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_bounded_string(deserializer)
    }
}

struct JsonSeed {
    budget: std::rc::Rc<std::cell::Cell<usize>>,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for JsonSeed {
    type Value = serde_json::Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if self.depth > MAX_JSON_DEPTH {
            return Err(serde::de::Error::custom("JSON exceeds the bounded depth"));
        }
        deserializer.deserialize_any(JsonVisitor {
            budget: self.budget,
            depth: self.depth,
        })
    }
}

struct JsonVisitor {
    budget: std::rc::Rc<std::cell::Cell<usize>>,
    depth: usize,
}

impl JsonVisitor {
    fn charge<E: serde::de::Error>(&self, amount: usize) -> Result<(), E> {
        let next = self
            .budget
            .get()
            .checked_add(amount)
            .ok_or_else(|| E::custom("JSON size overflow"))?;
        if next > MAX_JSON_BYTES {
            return Err(E::custom("JSON exceeds the bounded payload size"));
        }
        self.budget.set(next);
        Ok(())
    }
}

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = serde_json::Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("bounded JSON")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.charge(4)?;
        Ok(serde_json::Value::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.charge(if value { 4 } else { 5 })?;
        Ok(serde_json::Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.charge(8)?;
        Ok(serde_json::json!(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.charge(8)?;
        Ok(serde_json::json!(value))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.charge(8)?;
        Ok(serde_json::json!(value))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.is_empty() || value.len() > MAX_TEXT_BYTES {
            return Err(E::custom("JSON string is empty or exceeds its bound"));
        }
        self.charge(value.len())?;
        Ok(serde_json::Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(&value)
    }

    fn visit_seq<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        self.charge(2)?;
        let mut values = Vec::with_capacity(access.size_hint().unwrap_or(0).min(MAX_CONTEXTS));
        while values.len() < MAX_CONTEXTS {
            let Some(value) = access.next_element_seed(JsonSeed {
                budget: self.budget.clone(),
                depth: self.depth + 1,
            })? else {
                return Ok(serde_json::Value::Array(values));
            };
            values.push(value);
        }
        if access.next_element_seed(JsonSeed {
            budget: self.budget.clone(),
            depth: self.depth + 1,
        })?.is_some() {
            return Err(serde::de::Error::custom("JSON array exceeds the bounded entry count"));
        }
        Ok(serde_json::Value::Array(values))
    }

    fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.charge(2)?;
        let mut values = serde_json::Map::new();
        while values.len() < MAX_STORAGE_ENTRIES {
            let Some((key, value)) = access.next_entry_seed(
                JsonStringSeed,
                JsonSeed { budget: self.budget.clone(), depth: self.depth + 1 },
            )? else {
                return Ok(serde_json::Value::Object(values));
            };
            values.insert(key, value);
        }
        if access.next_entry_seed(
            JsonStringSeed,
            JsonSeed { budget: self.budget.clone(), depth: self.depth + 1 },
        )?.is_some() {
            return Err(serde::de::Error::custom("JSON object exceeds the bounded entry count"));
        }
        Ok(serde_json::Value::Object(values))
    }
}

fn deserialize_bounded_json<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = JsonSeed {
        budget: std::rc::Rc::new(std::cell::Cell::new(0)),
        depth: 0,
    }
    .deserialize(deserializer)?;
    validate_json("script result", &value)
        .map_err(|error| serde::de::Error::custom(error.to_string()))?;
    Ok(value)
}

struct BoundedMapVisitor<K, V, const LIMIT: usize>(std::marker::PhantomData<(K, V)>);

impl<'de, K, V, const LIMIT: usize> Visitor<'de> for BoundedMapVisitor<K, V, LIMIT>
where
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>,
{
    type Value = BTreeMap<K, V>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded map")
    }

    fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = BTreeMap::new();
        while values.len() < LIMIT {
            let Some((key, value)) = access.next_entry()? else {
                return Ok(values);
            };
            values.insert(key, value);
        }
        if access.next_entry::<K, V>()?.is_some() {
            return Err(serde::de::Error::custom("map exceeds the bounded entry count"));
        }
        Ok(values)
    }
}

fn deserialize_bounded_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>,
{
    deserializer.deserialize_map(BoundedMapVisitor::<K, V, MAX_STORAGE_ENTRIES>(
        std::marker::PhantomData,
    ))
}

fn deserialize_bounded_candidates<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::<String, MAX_BACKEND_CANDIDATES>(
        std::marker::PhantomData,
    ))
}
fn deserialize_bounded_capability_map<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<BrowserCapability, CapabilityDescriptor>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_map(BoundedMapVisitor::<
        BrowserCapability,
        CapabilityDescriptor,
        MAX_CAPABILITIES,
    >(std::marker::PhantomData))
}
/// Semantic operations that a backend may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserCapability {
    Navigation,
    Contexts,
    Evidence,
    Action,
    Effects,
    Script,
    Capture,
    Storage,
    Prompts,
    Downloads,
}

impl BrowserCapability {
    pub const ALL: [Self; 10] = [
        Self::Navigation,
        Self::Contexts,
        Self::Evidence,
        Self::Action,
        Self::Effects,
        Self::Script,
        Self::Capture,
        Self::Storage,
        Self::Prompts,
        Self::Downloads,
    ];
}

/// Support is intentionally bounded.  No unknown level is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SupportLevel {
    Available,
    Partial,
    Restricted,
    Unavailable,
}

impl SupportLevel {
    pub fn satisfies(self, required: Self) -> bool {
        match required {
            Self::Available => self == Self::Available,
            Self::Partial => matches!(self, Self::Available | Self::Partial),
            Self::Restricted => matches!(self, Self::Available | Self::Partial | Self::Restricted),
            Self::Unavailable => true,
        }
    }
}

/// Whether a capability can be carried to another backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Portability {
    SemanticPortable,
    SurfacePortable,
    BackendCapabilityDependent,
    BackendSpecific,
    BrowserSpecific,
    NonPortable,
}

/// A capability needed by a semantic operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDependency {
    pub capability: BrowserCapability,
    pub minimum: SupportLevel,
    pub reason: String,
}

impl CapabilityDependency {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("capability dependency reason", &self.reason, MAX_LIMITATION_BYTES)
    }
}

/// One explicit capability declaration.  A missing map entry is not treated as
/// an available default; [`BackendProfile::capability`] reports it as omitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub level: SupportLevel,
    pub portability: Portability,
    #[serde(default)]
    pub dependencies: Vec<CapabilityDependency>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

impl CapabilityDescriptor {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        if self.dependencies.len() > MAX_DEPENDENCIES {
            return Err(invalid("capability dependencies", "too many entries"));
        }
        if self.limitations.len() > MAX_LIMITATIONS {
            return Err(invalid("capability limitations", "too many entries"));
        }
        for dependency in &self.dependencies {
            dependency.validate()?;
        }
        for limitation in &self.limitations {
            validate_text("capability limitation", limitation, MAX_LIMITATION_BYTES)?;
        }
        if self.level == SupportLevel::Unavailable && !self.dependencies.is_empty() {
            return Err(invalid(
                "capability dependencies",
                "unavailable capabilities cannot declare executable dependencies",
            ));
        }
        Ok(())
    }
}

fn unavailable_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor {
        level: SupportLevel::Unavailable,
        portability: Portability::NonPortable,
        dependencies: Vec::new(),
        limitations: vec!["capability was not declared by this backend".into()],
    }
}

/// Backend certification maturity.  The ordering is selection precedence only;
/// it does not make an experimental backend production-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CertificationLevel {
    ProductionCertified,
    Experimental,
    Partial,
    Unsupported,
}

impl CertificationLevel {
    fn rank(self) -> u8 {
        match self {
            Self::ProductionCertified => 4,
            Self::Experimental => 3,
            Self::Partial => 2,
            Self::Unsupported => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserVersionRange {
    pub family: String,
    #[serde(default)]
    pub minimum: Option<String>,
    #[serde(default)]
    pub maximum: Option<String>,
}

impl BrowserVersionRange {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("browser family", &self.family, MAX_BROWSER_FAMILY_BYTES)?;
        if let Some(minimum) = &self.minimum {
            validate_text("browser minimum version", minimum, MAX_VERSION_BYTES)?;
        }
        if let Some(maximum) = &self.maximum {
            validate_text("browser maximum version", maximum, MAX_VERSION_BYTES)?;
        }
        if let (Some(minimum), Some(maximum)) = (&self.minimum, &self.maximum)
            && compare_versions(minimum, maximum) == Ordering::Greater
        {
            return Err(invalid("browser version range", "minimum exceeds maximum"));
        }
        Ok(())
    }

    fn contains(&self, family: &str, version: Option<&str>) -> bool {
        if self.family != family {
            return false;
        }
        let Some(version) = version else {
            return true;
        };
        self.minimum
            .as_deref()
            .is_none_or(|minimum| compare_versions(version, minimum) != Ordering::Less)
            && self
                .maximum
                .as_deref()
                .is_none_or(|maximum| compare_versions(version, maximum) != Ordering::Greater)
    }
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = left.split(['.', '-', '+']).collect::<Vec<_>>();
    let right_parts = right.split(['.', '-', '+']).collect::<Vec<_>>();
    for (left_part, right_part) in left_parts.iter().zip(right_parts.iter()) {
        match (left_part.parse::<u64>(), right_part.parse::<u64>()) {
            (Ok(left), Ok(right)) => match left.cmp(&right) {
                Ordering::Equal => continue,
                order => return order,
            },
            _ => match left_part.cmp(right_part) {
                Ordering::Equal => continue,
                order => return order,
            },
        }
    }
    left_parts.len().cmp(&right_parts.len())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CertificationProfile {
    pub level: CertificationLevel,
    pub glass_version: String,
    #[serde(default, deserialize_with = "deserialize_bounded_vec")]
    pub tested_capabilities: Vec<BrowserCapability>,
    #[serde(default, deserialize_with = "deserialize_bounded_vec")]
    pub limitations: Vec<String>,
}

impl CertificationProfile {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("certification glass version", &self.glass_version, MAX_VERSION_BYTES)?;
        validate_vec_len(
            "certification tested capabilities",
            self.tested_capabilities.len(),
            MAX_CAPABILITIES,
        )?;
        validate_vec_len("certification limitations", self.limitations.len(), MAX_LIMITATIONS)?;
        for limitation in &self.limitations {
            validate_text("certification limitation", limitation, MAX_LIMITATION_BYTES)?;
        }
        if self.level == CertificationLevel::ProductionCertified && self.tested_capabilities.is_empty() {
            return Err(invalid(
                "certification tested capabilities",
                "production certification requires conformance coverage",
            ));
        }
        if self.level == CertificationLevel::Unsupported && self.tested_capabilities.iter().any(|capability| {
            capability != &BrowserCapability::Contexts
        }) {
            return Err(invalid(
                "certification tested capabilities",
                "unsupported backends cannot claim tested semantic capabilities",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendIdentity {
    pub backend_id: String,
    pub version: String,
    pub browser: BrowserVersionRange,
    pub certification: CertificationProfile,
}

impl BackendIdentity {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("backend id", &self.backend_id, MAX_BACKEND_ID_BYTES)?;
        validate_text("backend version", &self.version, MAX_VERSION_BYTES)?;
        self.browser.validate()?;
        self.certification.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendProfile {
    pub schema_version: u32,
    pub identity: BackendIdentity,
    #[serde(default)]
    pub capabilities: BTreeMap<BrowserCapability, CapabilityDescriptor>,
}
impl BackendProfile {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        if self.schema_version != BROWSER_BACKEND_SCHEMA_VERSION {
            return Err(invalid("schema version", "unsupported browser backend schema"));
        }
        self.identity.validate()?;
        validate_vec_len("capabilities", self.capabilities.len(), MAX_CAPABILITIES)?;
        for descriptor in self.capabilities.values() {
            descriptor.validate()?;
        }
        for capability in &self.identity.certification.tested_capabilities {
            let Some(descriptor) = self.capabilities.get(capability) else {
                return Err(invalid(
                    "certification tested capabilities",
                    "every tested capability must be declared",
                ));
            };
            if descriptor.level == SupportLevel::Unavailable {
                return Err(invalid(
                    "certification tested capabilities",
                    "tested capabilities must be supported",
                ));
            }
        }
        for capability in self.capabilities.keys() {
            validate_dependency_closure(self, *capability, &mut BTreeSet::new())?;
        }
        if self.identity.certification.level == CertificationLevel::Unsupported
            && self.capabilities.values().any(|descriptor| descriptor.level != SupportLevel::Unavailable)
        {
            return Err(invalid(
                "capabilities",
                "unsupported certification cannot advertise available capabilities",
            ));
        }
        Ok(())
    }

    pub fn capability(&self, capability: BrowserCapability) -> CapabilityDescriptor {
        self.capabilities
            .get(&capability)
            .cloned()
            .unwrap_or_else(unavailable_descriptor)
    }

    pub fn require(
        &self,
        capability: BrowserCapability,
        minimum: SupportLevel,
    ) -> Result<(), BrowserBackendError> {
        let descriptor = self.capability(capability);
        if descriptor.level.satisfies(minimum) {
            return Ok(());
        }
        Err(BrowserBackendError::CapabilityUnavailable {
            capability,
            required: minimum,
            actual: descriptor.level,
            declared: self.capabilities.contains_key(&capability),
        })
    }
}
fn validate_dependency_closure(
    profile: &BackendProfile,
    capability: BrowserCapability,
    visiting: &mut BTreeSet<BrowserCapability>,
) -> Result<(), BrowserBackendError> {
    if !visiting.insert(capability) {
        return Err(invalid("capability dependencies", "dependency cycle detected"));
    }
    let descriptor = profile
        .capabilities
        .get(&capability)
        .ok_or_else(|| invalid("capability dependencies", "dependency is undeclared"))?;
    for dependency in &descriptor.dependencies {
        let dependency_descriptor = profile
            .capabilities
            .get(&dependency.capability)
            .ok_or_else(|| invalid("capability dependencies", "dependency is undeclared"))?;
        if !dependency_descriptor.level.satisfies(dependency.minimum) {
            return Err(invalid(
                "capability dependencies",
                "dependency is below its required support level",
            ));
        }
        validate_dependency_closure(profile, dependency.capability, visiting)?;
    }
    visiting.remove(&capability);
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackendProfileWire {
    schema_version: u32,
    identity: BackendIdentity,
    #[serde(default, deserialize_with = "deserialize_bounded_capability_map")]
    capabilities: BTreeMap<BrowserCapability, CapabilityDescriptor>,
}

impl<'de> Deserialize<'de> for BackendProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = BackendProfileWire::deserialize(deserializer)?;
        let profile = Self {
            schema_version: wire.schema_version,
            identity: wire.identity,
            capabilities: wire.capabilities,
        };
        profile
            .validate()
            .map_err(|error| serde::de::Error::custom(error.to_string()))?;
        Ok(profile)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityRequirement {
    pub capability: BrowserCapability,
    pub minimum: SupportLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendSelectionRequest {
    pub schema_version: u32,
    pub glass_version: String,
    #[serde(default, deserialize_with = "deserialize_bounded_string_option")]
    pub preferred_backend_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bounded_vec")]
    pub required_capabilities: Vec<CapabilityRequirement>,
    #[serde(default = "default_minimum_certification")]
    pub minimum_certification: CertificationLevel,
    #[serde(default, deserialize_with = "deserialize_bounded_string_option")]
    pub browser_family: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bounded_string_option")]
    pub browser_version: Option<String>,
}

fn default_minimum_certification() -> CertificationLevel {
    CertificationLevel::Partial
}

impl BackendSelectionRequest {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        if self.schema_version != BROWSER_BACKEND_SCHEMA_VERSION {
            return Err(invalid("schema version", "unsupported browser backend schema"));
        }
        validate_text("glass version", &self.glass_version, MAX_VERSION_BYTES)?;
        if let Some(id) = &self.preferred_backend_id {
            validate_text("preferred backend id", id, MAX_BACKEND_ID_BYTES)?;
        }
        validate_vec_len(
            "required capabilities",
            self.required_capabilities.len(),
            MAX_SELECTION_REQUIREMENTS,
        )?;
        for requirement in &self.required_capabilities {
            if requirement.minimum == SupportLevel::Unavailable {
                return Err(invalid("required capability", "minimum cannot be unavailable"));
            }
        }
        if let Some(family) = &self.browser_family {
            validate_text("browser family", family, MAX_BROWSER_FAMILY_BYTES)?;
        }
        if let Some(version) = &self.browser_version {
            if self.browser_family.is_none() {
                return Err(invalid(
                    "browser family",
                    "browser version requires browser family",
                ));
            }
            validate_text("browser version", version, MAX_VERSION_BYTES)?;
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackendSelectionRequestWire {
    schema_version: u32,
    glass_version: String,
    #[serde(default, deserialize_with = "deserialize_bounded_string_option")]
    preferred_backend_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bounded_vec")]
    required_capabilities: Vec<CapabilityRequirement>,
    #[serde(default = "default_minimum_certification")]
    minimum_certification: CertificationLevel,
    #[serde(default, deserialize_with = "deserialize_bounded_string_option")]
    browser_family: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bounded_string_option")]
    browser_version: Option<String>,
}

impl<'de> Deserialize<'de> for BackendSelectionRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = BackendSelectionRequestWire::deserialize(deserializer)?;
        let request = Self {
            schema_version: wire.schema_version,
            glass_version: wire.glass_version,
            preferred_backend_id: wire.preferred_backend_id,
            required_capabilities: wire.required_capabilities,
            minimum_certification: wire.minimum_certification,
            browser_family: wire.browser_family,
            browser_version: wire.browser_version,
        };
        request
            .validate()
            .map_err(|error| serde::de::Error::custom(error.to_string()))?;
        Ok(request)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendSelectionResult {
    pub schema_version: u32,
    pub selected: BackendProfile,
    pub reason: SelectionReason,
    #[serde(deserialize_with = "deserialize_bounded_candidates")]
    pub considered_backend_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionReason {
    ExplicitPreference,
    CertificationThenCapability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionRejection {
    InvalidProfile,
    CertificationBelowMinimum,
    BrowserMismatch,
    MissingCapability(BrowserCapability),
}

/// Selects without iteration-order dependence.  An explicit preference is
/// strict: an unusable preferred backend is an error, never a silent fallback.
pub fn select_backend(
    request: &BackendSelectionRequest,
    profiles: &[BackendProfile],
) -> Result<BackendSelectionResult, BrowserBackendError> {
    request.validate()?;
    validate_vec_len("backend candidates", profiles.len(), MAX_BACKEND_CANDIDATES)?;
    if profiles.is_empty() {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "no backend profiles were provided".into(),
        });
    }
    let mut seen_backend_ids = BTreeSet::new();
    for profile in profiles {
        if !seen_backend_ids.insert(profile.identity.backend_id.as_str()) {
            return Err(invalid("backend profiles", "duplicate backend id"));
        }
    }

    let mut considered_backend_ids = profiles
        .iter()
        .map(|profile| profile.identity.backend_id.clone())
        .collect::<Vec<_>>();
    considered_backend_ids.sort();
    considered_backend_ids.dedup();

    let mut eligible = Vec::new();
    let mut preferred_rejection = None;
    for profile in profiles {
        let id = &profile.identity.backend_id;
        if let Err(error) = profile.validate() {
            if request.preferred_backend_id.as_ref() == Some(id) {
                preferred_rejection = Some(error.to_string());
            }
            continue;
        }
        if profile.identity.certification.glass_version != request.glass_version {
            if request.preferred_backend_id.as_ref() == Some(id) {
                preferred_rejection = Some("backend certification targets a different Glass version".into());
            }
            continue;
        }
        let rejection = if profile.identity.certification.level.rank()
            < request.minimum_certification.rank()
        {
            Some(SelectionRejection::CertificationBelowMinimum)
        } else if let Some(family) = request.browser_family.as_deref()
            && !profile.identity.browser.contains(family, request.browser_version.as_deref())
        {
            Some(SelectionRejection::BrowserMismatch)
        } else {
            request.required_capabilities.iter().find_map(|requirement| {
                (!profile
                    .capability(requirement.capability)
                    .level
                    .satisfies(requirement.minimum))
                .then_some(SelectionRejection::MissingCapability(requirement.capability))
            })
        };
        if let Some(rejection) = rejection {
            if request.preferred_backend_id.as_ref() == Some(id) {
                preferred_rejection = Some(format_selection_rejection(&rejection));
            }
        } else {
            eligible.push(profile);
        }
    }

    if let Some(preferred) = &request.preferred_backend_id {
        let Some(profile) = eligible.iter().find(|profile| &profile.identity.backend_id == preferred) else {
            return Err(BrowserBackendError::SelectionFailed {
                reason: preferred_rejection
                    .unwrap_or_else(|| format!("preferred backend `{preferred}` is not available")),
            });
        };
        return Ok(BackendSelectionResult {
            schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
            selected: (*profile).clone(),
            reason: SelectionReason::ExplicitPreference,
            considered_backend_ids,
        });
    }

    eligible.sort_by(|left, right| {
        right
            .identity
            .certification
            .level
            .rank()
            .cmp(&left.identity.certification.level.rank())
            .then_with(|| capability_score(right).cmp(&capability_score(left)))
            .then_with(|| left.identity.backend_id.cmp(&right.identity.backend_id))
            .then_with(|| left.identity.version.cmp(&right.identity.version))
    });
    let Some(selected) = eligible.first() else {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "no backend satisfies the requested capabilities and policy".into(),
        });
    };
    Ok(BackendSelectionResult {
        schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
        selected: (*selected).clone(),
        reason: SelectionReason::CertificationThenCapability,
        considered_backend_ids,
    })
}

fn capability_score(profile: &BackendProfile) -> u16 {
    BrowserCapability::ALL
        .iter()
        .map(|capability| match profile.capability(*capability).level {
            SupportLevel::Available => 4,
            SupportLevel::Partial => 2,
            SupportLevel::Restricted => 1,
            SupportLevel::Unavailable => 0,
        })
        .sum()
}

fn format_selection_rejection(rejection: &SelectionRejection) -> String {
    match rejection {
        SelectionRejection::InvalidProfile => "invalid backend profile".into(),
        SelectionRejection::CertificationBelowMinimum => "certification is below minimum".into(),
        SelectionRejection::BrowserMismatch => "browser version range does not match".into(),
        SelectionRejection::MissingCapability(capability) => {
            format!("required capability {capability:?} is unavailable")
        }
    }
}

fn invalid(field: &str, reason: &str) -> BrowserBackendError {
    BrowserBackendError::InvalidConfiguration {
        field: field.into(),
        reason: reason.into(),
    }
}

/// Stable failures shared by every backend adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "details", rename_all = "camelCase")]
pub enum BrowserBackendError {
    CapabilityUnavailable {
        capability: BrowserCapability,
        required: SupportLevel,
        actual: SupportLevel,
        declared: bool,
    },
    InvalidConfiguration {
        #[serde(deserialize_with = "deserialize_bounded_string")]
        field: String,
        #[serde(deserialize_with = "deserialize_bounded_string")]
        reason: String,
    },
    Connection {
        #[serde(deserialize_with = "deserialize_bounded_string")]
        operation: String,
        #[serde(deserialize_with = "deserialize_bounded_string")]
        reason: String,
    },
    Lifecycle {
        #[serde(deserialize_with = "deserialize_bounded_string")]
        operation: String,
        #[serde(deserialize_with = "deserialize_bounded_string")]
        state: String,
        #[serde(deserialize_with = "deserialize_bounded_string")]
        reason: String,
    },
    UnsupportedOperation {
        #[serde(deserialize_with = "deserialize_bounded_string")]
        operation: String,
        #[serde(deserialize_with = "deserialize_bounded_string")]
        reason: String,
    },
    SelectionFailed {
        #[serde(deserialize_with = "deserialize_bounded_string")]
        reason: String,
    },
}

impl fmt::Display for BrowserBackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityUnavailable { capability, .. } => {
                write!(formatter, "capability unavailable: {capability:?}")
            }
            Self::InvalidConfiguration { field, reason } => write!(formatter, "invalid {field}: {reason}"),
            Self::Connection { operation, reason } => write!(formatter, "connection failure during {operation}: {reason}"),
            Self::Lifecycle { operation, state, reason } => write!(formatter, "lifecycle failure during {operation} ({state}): {reason}"),
            Self::UnsupportedOperation { operation, reason } => write!(formatter, "unsupported operation {operation}: {reason}"),
            Self::SelectionFailed { reason } => write!(formatter, "backend selection failed: {reason}"),
        }
    }
}

impl std::error::Error for BrowserBackendError {}
impl BackendContract for BrowserBackendError {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        match self {
            Self::CapabilityUnavailable { .. } => Ok(()),
            Self::InvalidConfiguration { field, reason }
            | Self::Connection { operation: field, reason }
            | Self::UnsupportedOperation { operation: field, reason } => {
                validate_text("error field", field, MAX_BACKEND_ID_BYTES)?;
                validate_text("error reason", reason, MAX_DIAGNOSTIC_BYTES)
            }
            Self::Lifecycle { operation, state, reason } => {
                validate_text("error operation", operation, MAX_BACKEND_ID_BYTES)?;
                validate_text("error state", state, MAX_LIMITATION_BYTES)?;
                validate_text("error reason", reason, MAX_DIAGNOSTIC_BYTES)
            }
            Self::SelectionFailed { reason } => {
                validate_text("selection reason", reason, MAX_DIAGNOSTIC_BYTES)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationResult {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub url: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextRequest {
    #[serde(default)]
    pub include_background: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowsingContext {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub url: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub level: EvidenceLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceLevel {
    Compact,
    Deep,
    Screenshot,
    Combined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceResult {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub revision: u64,
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub url: String,
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub title: String,
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub visible_text: String,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub action: SemanticAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticAction {
    Click { #[serde(deserialize_with = "deserialize_bounded_string")] target: String },
    Type {
        #[serde(deserialize_with = "deserialize_bounded_string")]
        target: String,
        #[serde(deserialize_with = "deserialize_bounded_string")]
        text: String,
    },
    KeyPress { #[serde(deserialize_with = "deserialize_bounded_string")] key: String },
    Scroll { delta_x: i32, delta_y: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionResult {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub revision: u64,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectsRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub since_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectsResult {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub revision: u64,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptResult {
    #[serde(deserialize_with = "deserialize_bounded_json")]
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureFormat {
    Png,
    Jpeg,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub format: CaptureFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureResult {
    pub format: CaptureFormat,
    #[serde(deserialize_with = "deserialize_bounded_bytes")]
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageScope {
    Cookies,
    Local,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageOperation {
    Read,
    Write {
        #[serde(deserialize_with = "deserialize_bounded_string")]
        key: String,
        #[serde(deserialize_with = "deserialize_bounded_string")]
        value: String,
    },
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub scope: StorageScope,
    pub operation: StorageOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageResult {
    #[serde(deserialize_with = "deserialize_bounded_map")]
    pub entries: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptDecision {
    Accept,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub decision: PromptDecision,
}

/// Validation shared by transport adapters before dispatch and after
/// deserializing backend responses.
pub trait BackendContract {
    fn validate(&self) -> Result<(), BrowserBackendError>;
}
fn validate_backend_error(
    result: Result<(), BrowserBackendError>,
) -> Result<(), BrowserBackendError> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            error.validate()?;
            Err(error)
        }
    }
}
fn validate_backend_result<T: BackendContract>(
    result: Result<T, BrowserBackendError>,
) -> Result<T, BrowserBackendError> {
    match result {
        Ok(value) => {
            value.validate()?;
            Ok(value)
        }
        Err(error) => {
            error.validate()?;
            Err(error)
        }
    }
}

impl BackendContract for NavigationRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("navigation url", &self.url, MAX_TEXT_BYTES)
    }
}

impl BackendContract for Vec<BrowsingContext> {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_vec_len("contexts", self.len(), MAX_CONTEXTS)?;
        for context in self {
            context.validate()?;
        }
        Ok(())
    }
}

impl BackendContract for NavigationResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("navigation url", &self.url, MAX_TEXT_BYTES)
    }
}

impl BackendContract for BrowsingContext {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)?;
        validate_text("context url", &self.url, MAX_TEXT_BYTES)
    }
}

impl BackendContract for ContextRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        Ok(())
    }
}

impl BackendContract for EvidenceRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)
    }
}

impl BackendContract for EvidenceResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)?;
        validate_text("evidence url", &self.url, MAX_TEXT_BYTES)?;
        validate_text("evidence title", &self.title, MAX_TEXT_BYTES)?;
        validate_text("visible text", &self.visible_text, MAX_TEXT_BYTES)
    }
}

impl BackendContract for SemanticAction {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        match self {
            Self::Click { target } => validate_text("action target", target, MAX_TEXT_BYTES),
            Self::Type { target, text } => {
                validate_text("action target", target, MAX_TEXT_BYTES)?;
                validate_text("action text", text, MAX_TEXT_BYTES)
            }
            Self::KeyPress { key } => validate_text("key", key, MAX_LIMITATION_BYTES),
            Self::Scroll { .. } => Ok(()),
        }
    }
}

impl BackendContract for ActionRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)?;
        self.action.validate()
    }
}

impl BackendContract for ActionResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)
    }
}

impl BackendContract for EffectsRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)
    }
}

impl BackendContract for EffectsResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)
    }
}

impl BackendContract for ScriptRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)?;
        validate_text("script source", &self.source, MAX_TEXT_BYTES)
    }
}

impl BackendContract for ScriptResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_json("script result", &self.value)
    }
}

impl BackendContract for CaptureRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)
    }
}

impl BackendContract for CaptureResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        if self.bytes.len() > MAX_CAPTURE_BYTES {
            return Err(invalid("capture bytes", "capture exceeds the bounded payload size"));
        }
        Ok(())
    }
}

impl BackendContract for StorageRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)?;
        if let StorageOperation::Write { key, value } = &self.operation {
            validate_text("storage key", key, MAX_BACKEND_ID_BYTES)?;
            validate_text("storage value", value, MAX_TEXT_BYTES)?;
        }
        Ok(())
    }
}

impl BackendContract for StorageResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_vec_len("storage entries", self.entries.len(), MAX_STORAGE_ENTRIES)?;
        for (key, value) in &self.entries {
            validate_text("storage key", key, MAX_BACKEND_ID_BYTES)?;
            validate_text("storage value", value, MAX_TEXT_BYTES)?;
        }
        Ok(())
    }
}

impl BackendContract for PromptRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)
    }
}

impl BackendContract for PromptResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        Ok(())
    }
}

impl BackendContract for DownloadRequest {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("context id", &self.context_id, MAX_BACKEND_ID_BYTES)?;
        if let DownloadOperation::Cancel { download_id } = &self.operation {
            validate_text("download id", download_id, MAX_BACKEND_ID_BYTES)?;
        }
        Ok(())
    }
}

impl BackendContract for DownloadResult {
    fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_vec_len("downloads", self.download_ids.len(), MAX_DOWNLOADS)?;
        for id in &self.download_ids {
            validate_text("download id", id, MAX_BACKEND_ID_BYTES)?;
        }
        Ok(())
    }
}

/// Semantic operation names used for capability-gated dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendOperation {
    Navigate,
    Contexts,
    Evidence,
    Action,
    Effects,
    Script,
    Capture,
    Storage,
    Prompt,
    Download,
}

impl BackendOperation {
    pub const fn capability(self) -> BrowserCapability {
        match self {
            Self::Navigate => BrowserCapability::Navigation,
            Self::Contexts => BrowserCapability::Contexts,
            Self::Evidence => BrowserCapability::Evidence,
            Self::Action => BrowserCapability::Action,
            Self::Effects => BrowserCapability::Effects,
            Self::Script => BrowserCapability::Script,
            Self::Capture => BrowserCapability::Capture,
            Self::Storage => BrowserCapability::Storage,
            Self::Prompt => BrowserCapability::Prompts,
            Self::Download => BrowserCapability::Downloads,
        }
    }
}

impl BackendProfile {
    pub fn require_operation(
        &self,
        operation: BackendOperation,
        minimum: SupportLevel,
    ) -> Result<(), BrowserBackendError> {
        self.require(operation.capability(), minimum)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptResult {
    pub handled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadOperation {
    List,
    Cancel {
        #[serde(deserialize_with = "deserialize_bounded_string")]
        download_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadRequest {
    #[serde(deserialize_with = "deserialize_bounded_string")]
    pub context_id: String,
    pub operation: DownloadOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadResult {
    #[serde(deserialize_with = "deserialize_bounded_vec")]
    pub download_ids: Vec<String>,
}

pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BrowserBackendError>> + Send + 'a>>;

/// Object-safe asynchronous semantic backend boundary.
///
/// Implementations MUST call [`BackendProfile::require`] before an operation;
/// this prevents a missing optional capability from silently becoming a
/// weaker operation.  Adapters may use any transport internally.
pub trait BrowserBackend: Send + Sync {
    fn profile(&self) -> &BackendProfile;
    #[doc(hidden)]
    fn initialize_raw<'a>(&'a self) -> BackendFuture<'a, ()>;
    #[doc(hidden)]
    fn close_raw<'a>(&'a self) -> BackendFuture<'a, ()>;
    #[doc(hidden)]
    fn navigate_raw<'a>(&'a self, request: NavigationRequest) -> BackendFuture<'a, NavigationResult>;
    #[doc(hidden)]
    fn contexts_raw<'a>(&'a self, request: ContextRequest) -> BackendFuture<'a, Vec<BrowsingContext>>;
    #[doc(hidden)]
    fn evidence_raw<'a>(&'a self, request: EvidenceRequest) -> BackendFuture<'a, EvidenceResult>;
    #[doc(hidden)]
    fn action_raw<'a>(&'a self, request: ActionRequest) -> BackendFuture<'a, ActionResult>;
    #[doc(hidden)]
    fn effects_raw<'a>(&'a self, request: EffectsRequest) -> BackendFuture<'a, EffectsResult>;
    #[doc(hidden)]
    fn script_raw<'a>(&'a self, request: ScriptRequest) -> BackendFuture<'a, ScriptResult>;
    #[doc(hidden)]
    fn capture_raw<'a>(&'a self, request: CaptureRequest) -> BackendFuture<'a, CaptureResult>;
    #[doc(hidden)]
    fn storage_raw<'a>(&'a self, request: StorageRequest) -> BackendFuture<'a, StorageResult>;
    #[doc(hidden)]
    fn prompt_raw<'a>(&'a self, request: PromptRequest) -> BackendFuture<'a, PromptResult>;
    #[doc(hidden)]
    fn download_raw<'a>(&'a self, request: DownloadRequest) -> BackendFuture<'a, DownloadResult>;
    fn require_operation(
        &self,
        operation: BackendOperation,
        minimum: SupportLevel,
    ) -> Result<(), BrowserBackendError> {
        self.profile().require_operation(operation, minimum)
    }
}
/// Mandatory validation and capability gate for backend calls.
///
/// Adapters implement [`BrowserBackend`] as the transport-facing primitive;
/// callers should retain this dispatcher rather than invoking those primitives
/// directly.  Every method validates inputs, checks the complete profile
/// dependency closure, and validates the backend result before returning it.
pub struct BrowserBackendDispatcher<'a> {
    backend: &'a dyn BrowserBackend,
}

impl<'a> BrowserBackendDispatcher<'a> {
    pub fn new(backend: &'a dyn BrowserBackend) -> Self {
        Self { backend }
    }

    pub fn initialize(&self) -> BackendFuture<'a, ()> {
        let backend = self.backend;
        Box::pin(async move {
            backend.profile().validate()?;
            validate_backend_error(backend.initialize_raw().await)
        })
    }

    pub fn close(&self) -> BackendFuture<'a, ()> {
        let backend = self.backend;
        Box::pin(async move {
            backend.profile().validate()?;
            validate_backend_error(backend.close_raw().await)
        })
    }

    pub fn navigate(&self, request: NavigationRequest) -> BackendFuture<'a, NavigationResult> {
        self.call(BackendOperation::Navigate, request, |backend, request| {
            backend.navigate_raw(request)
        })
    }

    pub fn contexts(&self, request: ContextRequest) -> BackendFuture<'a, Vec<BrowsingContext>> {
        let backend = self.backend;
        Box::pin(async move {
            request.validate()?;
            backend.profile().validate()?;
            backend.require_operation(BackendOperation::Contexts, SupportLevel::Available)?;
            validate_backend_result(backend.contexts_raw(request).await)
        })
    }

    pub fn evidence(&self, request: EvidenceRequest) -> BackendFuture<'a, EvidenceResult> {
        self.call(BackendOperation::Evidence, request, |backend, request| {
            backend.evidence_raw(request)
        })
    }

    pub fn action(&self, request: ActionRequest) -> BackendFuture<'a, ActionResult> {
        self.call(BackendOperation::Action, request, |backend, request| {
            backend.action_raw(request)
        })
    }

    pub fn effects(&self, request: EffectsRequest) -> BackendFuture<'a, EffectsResult> {
        self.call(BackendOperation::Effects, request, |backend, request| {
            backend.effects_raw(request)
        })
    }

    pub fn script(&self, request: ScriptRequest) -> BackendFuture<'a, ScriptResult> {
        self.call(BackendOperation::Script, request, |backend, request| {
            backend.script_raw(request)
        })
    }

    pub fn capture(&self, request: CaptureRequest) -> BackendFuture<'a, CaptureResult> {
        self.call(BackendOperation::Capture, request, |backend, request| {
            backend.capture_raw(request)
        })
    }

    pub fn storage(&self, request: StorageRequest) -> BackendFuture<'a, StorageResult> {
        self.call(BackendOperation::Storage, request, |backend, request| {
            backend.storage_raw(request)
        })
    }

    pub fn prompt(&self, request: PromptRequest) -> BackendFuture<'a, PromptResult> {
        self.call(BackendOperation::Prompt, request, |backend, request| {
            backend.prompt_raw(request)
        })
    }

    pub fn download(&self, request: DownloadRequest) -> BackendFuture<'a, DownloadResult> {
        self.call(BackendOperation::Download, request, |backend, request| {
            backend.download_raw(request)
        })
    }
    fn call<T, R>(
        &self,
        operation: BackendOperation,
        request: T,
        call: impl FnOnce(&'a dyn BrowserBackend, T) -> BackendFuture<'a, R> + Send + 'a,
    ) -> BackendFuture<'a, R>
    where
        T: BackendContract + Send + 'a,
        R: BackendContract + Send + 'a,
    {
        let backend = self.backend;
        Box::pin(async move {
            request.validate()?;
            backend.profile().validate()?;
            backend.require_operation(operation, SupportLevel::Available)?;
            validate_backend_result(call(backend, request).await)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile(id: &str, certification: CertificationLevel) -> BackendProfile {
        let mut capabilities = BTreeMap::new();
        for capability in BrowserCapability::ALL {
            capabilities.insert(
                capability,
                CapabilityDescriptor {
                    level: SupportLevel::Available,
                    portability: Portability::SemanticPortable,
                    dependencies: Vec::new(),
                    limitations: Vec::new(),
                },
            );
        }
        BackendProfile {
            schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
            identity: BackendIdentity {
                backend_id: id.into(),
                version: "1.0.0".into(),
                browser: BrowserVersionRange {
                    family: "chromium".into(),
                    minimum: Some("120.0.0".into()),
                    maximum: Some("160.0.0".into()),
                },
                certification: CertificationProfile {
                    level: certification,
                    glass_version: "0.3.1".into(),
                    tested_capabilities: if certification == CertificationLevel::ProductionCertified {
                        BrowserCapability::ALL.to_vec()
                    } else {
                        Vec::new()
                    },
                    limitations: Vec::new(),
                },
            },
            capabilities,
        }
    }

    #[test]
    fn profile_serialization_is_stable_and_camel_case() {
        let value = serde_json::to_value(profile("cdp", CertificationLevel::ProductionCertified)).unwrap();
        assert_eq!(value["schemaVersion"], json!(1));
        assert_eq!(value["identity"]["backendId"], json!("cdp"));
        assert_eq!(value["identity"]["certification"]["level"], json!("productionCertified"));
        assert_eq!(value["capabilities"]["navigation"]["level"], json!("available"));
        assert!(serde_json::from_value::<BackendProfile>(value).is_ok());
    }

    #[test]
    fn capture_and_bounded_json_round_trip() {
        let capture = CaptureResult {
            format: CaptureFormat::Png,
            bytes: vec![0, 1, 2, 255],
        };
        let encoded = serde_json::to_value(&capture).unwrap();
        assert_eq!(serde_json::from_value::<CaptureResult>(encoded).unwrap(), capture);

        let script = ScriptResult {
            value: json!({"ok": true, "items": [1, 2, 3]}),
        };
        let encoded = serde_json::to_value(&script).unwrap();
        assert_eq!(serde_json::from_value::<ScriptResult>(encoded).unwrap(), script);
    }

    #[test]
    fn selection_precedence_is_explicit_then_certification_then_capability() {
        let production = profile("cdp", CertificationLevel::ProductionCertified);
        let experimental = profile("bidi", CertificationLevel::Experimental);
        let request = BackendSelectionRequest {
            schema_version: 1,
            glass_version: "0.3.1".into(),
            preferred_backend_id: Some("bidi".into()),
            required_capabilities: vec![CapabilityRequirement {
                capability: BrowserCapability::Evidence,
                minimum: SupportLevel::Available,
            }],
            minimum_certification: CertificationLevel::Partial,
            browser_family: Some("chromium".into()),
            browser_version: Some("150.0.0".into()),
        };
        let selected = select_backend(&request, &[production.clone(), experimental]).unwrap();
        assert_eq!(selected.selected.identity.backend_id, "bidi");
        assert_eq!(selected.reason, SelectionReason::ExplicitPreference);

        let mut automatic = request;
        automatic.preferred_backend_id = None;
        let selected = select_backend(&automatic, &[profile("bidi", CertificationLevel::Experimental), production]).unwrap();
        assert_eq!(selected.selected.identity.backend_id, "cdp");
        assert_eq!(selected.reason, SelectionReason::CertificationThenCapability);
    }

    #[test]
    fn omitted_capability_is_explicit_and_fails_closed() {
        let mut backend = profile("partial", CertificationLevel::Partial);
        backend.capabilities.remove(&BrowserCapability::Downloads);
        let descriptor = backend.capability(BrowserCapability::Downloads);
        assert_eq!(descriptor.level, SupportLevel::Unavailable);
        assert!(matches!(
            backend.require(BrowserCapability::Downloads, SupportLevel::Partial),
            Err(BrowserBackendError::CapabilityUnavailable { declared: false, .. })
        ));
    }

    #[test]
    fn production_certification_requires_conformance_evidence() {
        let mut backend = profile("bad", CertificationLevel::ProductionCertified);
        backend.identity.certification.tested_capabilities.clear();
        assert!(matches!(backend.validate(), Err(BrowserBackendError::InvalidConfiguration { field, .. }) if field == "certification tested capabilities"));
    }

    #[test]
    fn typed_errors_round_trip_without_losing_kind() {
        let errors = [
            BrowserBackendError::CapabilityUnavailable {
                capability: BrowserCapability::Evidence,
                required: SupportLevel::Available,
                actual: SupportLevel::Unavailable,
                declared: false,
            },
            BrowserBackendError::InvalidConfiguration { field: "url".into(), reason: "empty".into() },
            BrowserBackendError::Connection { operation: "initialize".into(), reason: "refused".into() },
            BrowserBackendError::Lifecycle { operation: "close".into(), state: "closing".into(), reason: "timeout".into() },
            BrowserBackendError::UnsupportedOperation { operation: "capture".into(), reason: "not implemented".into() },
        ];
        for error in errors {
            let encoded = serde_json::to_value(&error).unwrap();
            assert!(encoded.get("kind").is_some());
            assert_eq!(serde_json::from_value::<BrowserBackendError>(encoded).unwrap(), error);
        }
    }
    #[test]
    fn request_deserialization_rejects_oversized_strings() {
        let value = json!({ "url": "x".repeat(MAX_TEXT_BYTES + 1) });
        assert!(serde_json::from_value::<NavigationRequest>(value).is_err());
        assert!(serde_json::from_value::<NavigationRequest>(json!({ "url": "" })).is_err());
    }

    #[test]
    fn dependency_closure_and_certification_support_are_required() {
        let mut backend = profile("closure", CertificationLevel::ProductionCertified);
        let mut evidence = backend.capabilities[&BrowserCapability::Navigation].clone();
        evidence.dependencies.push(CapabilityDependency {
            capability: BrowserCapability::Evidence,
            minimum: SupportLevel::Available,
            reason: "fresh evidence".into(),
        });
        backend.capabilities.insert(BrowserCapability::Navigation, evidence);
        backend.capabilities.remove(&BrowserCapability::Evidence);
        assert!(backend.validate().is_err());

        let mut unsupported = profile("bad-cert", CertificationLevel::ProductionCertified);
        unsupported.capabilities.insert(
            BrowserCapability::Evidence,
            CapabilityDescriptor {
                level: SupportLevel::Unavailable,
                portability: Portability::NonPortable,
                dependencies: Vec::new(),
                limitations: vec!["not available".into()],
            },
        );
        assert!(unsupported.validate().is_err());
    }

    #[test]
    fn selection_requires_exact_glass_compatibility_and_browser_family() {
        let mut request = BackendSelectionRequest {
            schema_version: 1,
            glass_version: "0.3.1".into(),
            preferred_backend_id: None,
            required_capabilities: Vec::new(),
            minimum_certification: CertificationLevel::Partial,
            browser_family: None,
            browser_version: Some("150.0.0".into()),
        };
        assert!(request.validate().is_err());
        request.browser_family = Some("chromium".into());
        request.glass_version = "0.4.0".into();
        assert!(matches!(
            select_backend(&request, &[profile("cdp", CertificationLevel::Partial)]),
            Err(BrowserBackendError::SelectionFailed { .. })
        ));
    }

    #[test]
    fn operation_dispatch_reports_missing_capability() {
        let mut backend = profile("partial", CertificationLevel::Partial);
        backend.capabilities.remove(&BrowserCapability::Capture);
        assert!(matches!(
            backend.require_operation(BackendOperation::Capture, SupportLevel::Available),
            Err(BrowserBackendError::CapabilityUnavailable {
                capability: BrowserCapability::Capture,
                declared: false,
                ..
            })
        ));
    }

    #[test]
    fn candidate_and_error_diagnostics_are_bounded() {
        let profiles = (0..=MAX_BACKEND_CANDIDATES)
            .map(|index| profile(&format!("backend-{index}"), CertificationLevel::Partial))
            .collect::<Vec<_>>();
        let request = BackendSelectionRequest {
            schema_version: 1,
            glass_version: "0.3.1".into(),
            preferred_backend_id: None,
            required_capabilities: Vec::new(),
            minimum_certification: CertificationLevel::Partial,
            browser_family: None,
            browser_version: None,
        };
        assert!(matches!(
            select_backend(&request, &profiles),
            Err(BrowserBackendError::InvalidConfiguration { field, .. }) if field == "backend candidates"
        ));
        assert!(BrowserBackendError::SelectionFailed {
            reason: "x".repeat(MAX_DIAGNOSTIC_BYTES + 1),
        }
        .validate()
        .is_err());
    }
    #[test]
    fn direct_profile_deserialization_validates_dependency_closure() {
        let mut value = serde_json::to_value(profile("closure-json", CertificationLevel::Partial)).unwrap();
        value["capabilities"]["navigation"]["dependencies"] = json!([{
            "capability": "evidence",
            "minimum": "available",
            "reason": "evidence required"
        }]);
        value["capabilities"]
            .as_object_mut()
            .unwrap()
            .remove("evidence");
        assert!(serde_json::from_value::<BackendProfile>(value).is_err());
    }

    #[test]
    fn nested_deserialization_stops_at_bounded_counts() {
        let requirements = (0..(MAX_CONTEXTS + 1))
            .map(|_| json!({"capability": "evidence", "minimum": "available"}))
            .collect::<Vec<_>>();
        let request = json!({
            "schemaVersion": 1,
            "glassVersion": "0.3.1",
            "requiredCapabilities": requirements
        });
        assert!(serde_json::from_value::<BackendSelectionRequest>(request).is_err());
    }
}
