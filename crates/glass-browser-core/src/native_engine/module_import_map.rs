use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::fmt;
use url::Url;

const MAX_IMPORT_MAP_ENTRIES: usize = 128;
const MAX_IMPORT_MAP_SCOPES: usize = 32;
const MAX_IMPORT_MAP_INTEGRITY_BYTES: usize = 4 * 1024;
const MAX_IMPORT_MAP_RESOLUTIONS: usize = 1024;

type NativeModuleSpecifierMap = BTreeMap<String, String>;
type NativeModuleIntegrityMap = BTreeMap<String, String>;

#[derive(Debug)]
struct OrderedJsonObject<T>(Vec<(String, T)>);

impl<T> OrderedJsonObject<T> {
    fn get(&self, key: &str) -> Option<&T> {
        self.0
            .iter()
            .rev()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value)
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'de, T> Deserialize<'de> for OrderedJsonObject<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OrderedObjectVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> Visitor<'de> for OrderedObjectVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = OrderedJsonObject<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, T>()? {
                    entries.push(entry);
                }
                Ok(OrderedJsonObject(entries))
            }
        }

        deserializer.deserialize_map(OrderedObjectVisitor(std::marker::PhantomData))
    }
}

#[derive(Debug)]
enum OrderedJsonValue {
    Null,
    Bool,
    Number,
    String(String),
    Array,
    Object(OrderedJsonObject<Self>),
}

impl OrderedJsonValue {
    fn as_object(&self) -> Option<&OrderedJsonObject<Self>> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for OrderedJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OrderedValueVisitor;

        impl<'de> Visitor<'de> for OrderedValueVisitor {
            type Value = OrderedJsonValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON value")
            }

            fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Bool)
            }

            fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Number)
            }

            fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Number)
            }

            fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Number)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::String(value))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Null)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(OrderedJsonValue::Null)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                while sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                    // Consume invalid member shapes without allocating a nested tree.
                }
                Ok(OrderedJsonValue::Array)
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, OrderedJsonValue>()? {
                    entries.push(entry);
                }
                Ok(OrderedJsonValue::Object(OrderedJsonObject(entries)))
            }
        }

        deserializer.deserialize_any(OrderedValueVisitor)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeModuleImportMap {
    imports: NativeModuleSpecifierMap,
    scopes: BTreeMap<String, NativeModuleSpecifierMap>,
    integrity: NativeModuleIntegrityMap,
    resolved: BTreeMap<(String, String), String>,
}

impl NativeModuleImportMap {
    pub(crate) fn parse(source: &str, base_url: &str) -> Result<Self, &'static str> {
        let base = Url::parse(base_url).map_err(|_| "import map base URL is invalid")?;
        let value: OrderedJsonValue =
            serde_json::from_str(source).map_err(|_| "import map is not valid JSON")?;
        let object = value
            .as_object()
            .ok_or("import map must be a JSON object")?;
        let mut entry_count = 0;
        let imports = object
            .get("imports")
            .map(|value| parse_specifier_map(value, &base, &mut entry_count))
            .transpose()?
            .unwrap_or_default();

        let mut scopes = BTreeMap::new();
        if let Some(value) = object.get("scopes") {
            let scope_entries = value
                .as_object()
                .ok_or("import map scopes member must be a JSON object")?;
            if scope_entries.len() > MAX_IMPORT_MAP_SCOPES {
                return Err("import map exceeds its scope limit");
            }
            for (scope, value) in &scope_entries.0 {
                let normalized_scope = resolve_url(&base, scope)?;
                if !normalized_scope.username().is_empty() || normalized_scope.password().is_some()
                {
                    return Err("import-map scopes must not contain credentials");
                }
                let normalized_scope = normalized_scope.to_string();
                let specifiers = parse_specifier_map(value, &base, &mut entry_count)?;
                scopes.insert(normalized_scope, specifiers);
            }
        }

        let integrity = object
            .get("integrity")
            .map(|value| parse_integrity_map(value, &base, &mut entry_count))
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            imports,
            scopes,
            integrity,
            resolved: BTreeMap::new(),
        })
    }

    pub(crate) fn merge(&mut self, newer: Self) -> Result<(), &'static str> {
        let mut merged = self.clone();
        let mut entry_count = merged.entry_count();
        let newer_imports: NativeModuleSpecifierMap = newer
            .imports
            .into_iter()
            .filter(|(specifier, _)| !merged.mapping_conflicts_with_resolved(None, specifier))
            .collect();
        merge_specifier_map(&mut merged.imports, newer_imports, &mut entry_count)?;
        for (scope, newer_specifiers) in newer.scopes {
            let newer_specifiers: NativeModuleSpecifierMap = newer_specifiers
                .into_iter()
                .filter(|(specifier, _)| {
                    !merged.mapping_conflicts_with_resolved(Some(&scope), specifier)
                })
                .collect();
            if newer_specifiers.is_empty() && !merged.scopes.contains_key(&scope) {
                continue;
            }
            if !merged.scopes.contains_key(&scope) {
                if merged.scopes.len() >= MAX_IMPORT_MAP_SCOPES {
                    return Err("merged import maps exceed their scope limit");
                }
                merged.scopes.insert(scope.clone(), BTreeMap::new());
            }
            let existing = merged
                .scopes
                .get_mut(&scope)
                .expect("scope was retained or inserted");
            merge_specifier_map(existing, newer_specifiers, &mut entry_count)?;
        }
        merge_integrity_map(&mut merged.integrity, newer.integrity, &mut entry_count)?;
        *self = merged;
        Ok(())
    }

    pub(crate) fn integrity_for_url(&self, url: &str) -> Option<&str> {
        self.integrity.get(url).map(String::as_str)
    }

    pub(crate) fn resolve(&self, referrer: &str, specifier: &str) -> Result<String, &'static str> {
        let (referrer_url, normalized_specifier) = normalize_resolution_input(referrer, specifier)?;
        let resolution_key = (referrer_url.to_string(), normalized_specifier.clone());
        if let Some(resolved) = self.resolved.get(&resolution_key) {
            return Ok(resolved.clone());
        }
        self.resolve_normalized(&referrer_url, specifier, &normalized_specifier)
    }

    pub(crate) fn resolve_and_record(
        &mut self,
        referrer: &str,
        specifier: &str,
    ) -> Result<String, &'static str> {
        let (referrer_url, normalized_specifier) = normalize_resolution_input(referrer, specifier)?;
        let resolution_key = (referrer_url.to_string(), normalized_specifier.clone());
        if let Some(resolved) = self.resolved.get(&resolution_key) {
            return Ok(resolved.clone());
        }
        let resolved = self.resolve_normalized(&referrer_url, specifier, &normalized_specifier)?;
        if self.resolved.len() >= MAX_IMPORT_MAP_RESOLUTIONS {
            return Err("resolved module specifier set exceeds its entry limit");
        }
        self.resolved.insert(resolution_key, resolved.clone());
        Ok(resolved)
    }

    fn resolve_normalized(
        &self,
        referrer_url: &Url,
        original_specifier: &str,
        normalized_specifier: &str,
    ) -> Result<String, &'static str> {
        let serialized_referrer = referrer_url.as_str();

        let mut applicable_scopes = self
            .scopes
            .iter()
            .filter(|(scope, _)| scope_matches(scope, serialized_referrer))
            .collect::<Vec<_>>();
        applicable_scopes.sort_by(|(left, _), (right, _)| {
            right.len().cmp(&left.len()).then_with(|| left.cmp(right))
        });
        for (_, scope_imports) in applicable_scopes {
            if let Some(target) = resolve_imports(scope_imports, normalized_specifier)? {
                return Ok(target.to_string());
            }
        }

        let target = match resolve_imports(&self.imports, normalized_specifier)? {
            Some(target) => target,
            None if is_url_like(original_specifier) => {
                resolve_url(referrer_url, original_specifier)?
            }
            None => return Err("bare module specifiers require an import map"),
        };
        if !target.username().is_empty() || target.password().is_some() {
            return Err("module URL must not contain credentials");
        }
        Ok(target.to_string())
    }

    fn mapping_conflicts_with_resolved(&self, scope: Option<&str>, map_key: &str) -> bool {
        self.resolved.iter().any(|((referrer, specifier), _)| {
            scope.is_none_or(|scope| scope_matches(scope, referrer))
                && mapping_key_affects_specifier(map_key, specifier)
        })
    }

    fn entry_count(&self) -> usize {
        self.imports.len()
            + self.scopes.values().map(BTreeMap::len).sum::<usize>()
            + self.integrity.len()
    }
}

fn normalize_resolution_input(
    referrer: &str,
    specifier: &str,
) -> Result<(Url, String), &'static str> {
    let referrer_url = Url::parse(referrer).map_err(|_| "module referrer URL is invalid")?;
    let normalized_specifier = if is_url_like(specifier) {
        resolve_url(&referrer_url, specifier)?.to_string()
    } else {
        specifier.to_owned()
    };
    Ok((referrer_url, normalized_specifier))
}

fn mapping_key_affects_specifier(map_key: &str, specifier: &str) -> bool {
    if map_key == specifier {
        return true;
    }
    if !map_key.ends_with('/') || !specifier.starts_with(map_key) {
        return false;
    }
    let Ok(specifier_url) = Url::parse(specifier) else {
        return true;
    };
    matches!(
        specifier_url.scheme(),
        "file" | "ftp" | "http" | "https" | "ws" | "wss"
    )
}

fn parse_integrity_map(
    value: &OrderedJsonValue,
    base: &Url,
    entry_count: &mut usize,
) -> Result<NativeModuleIntegrityMap, &'static str> {
    let entries = value
        .as_object()
        .ok_or("import map integrity member must be a JSON object")?;
    *entry_count = entry_count.saturating_add(entries.len());
    if *entry_count > MAX_IMPORT_MAP_ENTRIES {
        return Err("import map exceeds its combined entry limit");
    }

    let mut normalized = BTreeMap::new();
    for (key, value) in &entries.0 {
        if !is_url_like(key) {
            continue;
        }
        let Ok(url) = resolve_url(base, key) else {
            continue;
        };
        if !url.username().is_empty() || url.password().is_some() {
            continue;
        }
        let Some(metadata) = value.as_str() else {
            continue;
        };
        if metadata.len() > MAX_IMPORT_MAP_INTEGRITY_BYTES {
            return Err("import-map integrity metadata exceeds its byte limit");
        }
        normalized.insert(url.to_string(), metadata.to_owned());
    }
    Ok(normalized)
}

fn parse_specifier_map(
    value: &OrderedJsonValue,
    base: &Url,
    entry_count: &mut usize,
) -> Result<NativeModuleSpecifierMap, &'static str> {
    let imports = value
        .as_object()
        .ok_or("import map specifier map must be a JSON object")?;
    *entry_count = entry_count.saturating_add(imports.len());
    if *entry_count > MAX_IMPORT_MAP_ENTRIES {
        return Err("import map exceeds its combined entry limit");
    }

    let mut normalized = BTreeMap::new();
    for (specifier, address) in &imports.0 {
        if specifier.is_empty() {
            return Err("import map specifier keys must not be empty");
        }
        let address = address
            .as_str()
            .ok_or("import map addresses must be strings")?;
        let normalized_specifier = normalize_specifier_key(specifier, base)?;
        let normalized_address = normalize_address(address, base)?;
        if specifier.ends_with('/') && !normalized_address.ends_with('/') {
            return Err("a prefix import-map address must end with a slash");
        }
        normalized.insert(normalized_specifier, normalized_address);
    }
    Ok(normalized)
}

fn merge_specifier_map(
    current: &mut NativeModuleSpecifierMap,
    newer: NativeModuleSpecifierMap,
    entry_count: &mut usize,
) -> Result<(), &'static str> {
    for (specifier, address) in newer {
        if current.contains_key(&specifier) {
            continue;
        }
        if *entry_count >= MAX_IMPORT_MAP_ENTRIES {
            return Err("merged import maps exceed their combined entry limit");
        }
        current.insert(specifier, address);
        *entry_count += 1;
    }
    Ok(())
}

fn merge_integrity_map(
    current: &mut NativeModuleIntegrityMap,
    newer: NativeModuleIntegrityMap,
    entry_count: &mut usize,
) -> Result<(), &'static str> {
    for (url, metadata) in newer {
        if current.contains_key(&url) {
            continue;
        }
        if *entry_count >= MAX_IMPORT_MAP_ENTRIES {
            return Err("merged import maps exceed their combined entry limit");
        }
        current.insert(url, metadata);
        *entry_count += 1;
    }
    Ok(())
}

fn scope_matches(scope: &str, referrer: &str) -> bool {
    scope == referrer || (scope.ends_with('/') && referrer.starts_with(scope))
}

fn resolve_imports(
    imports: &NativeModuleSpecifierMap,
    specifier: &str,
) -> Result<Option<Url>, &'static str> {
    let address = imports
        .get(specifier)
        .map(|address| (address.as_str(), ""))
        .or_else(|| {
            imports
                .iter()
                .filter(|(key, _)| key.ends_with('/') && specifier.starts_with(key.as_str()))
                .max_by_key(|(key, _)| key.len())
                .map(|(key, address)| (address.as_str(), &specifier[key.len()..]))
        });
    let Some((address, suffix)) = address else {
        return Ok(None);
    };
    let joined = format!("{address}{suffix}");
    let target = Url::parse(&joined).map_err(|_| "mapped module URL is invalid")?;
    if !target.username().is_empty() || target.password().is_some() {
        return Err("module URL must not contain credentials");
    }
    Ok(Some(target))
}

fn normalize_specifier_key(specifier: &str, base: &Url) -> Result<String, &'static str> {
    if is_url_like(specifier) {
        let key = resolve_url(base, specifier)?;
        if !key.username().is_empty() || key.password().is_some() {
            return Err("import-map specifier keys must not contain credentials");
        }
        Ok(key.to_string())
    } else {
        Ok(specifier.to_owned())
    }
}

fn normalize_address(address: &str, base: &Url) -> Result<String, &'static str> {
    if address.is_empty() || !is_url_like(address) {
        return Err("import map addresses must be absolute or URL-like URLs");
    }
    let target = resolve_url(base, address)?;
    if !target.username().is_empty() || target.password().is_some() {
        return Err("import map addresses must not contain credentials");
    }
    Ok(target.to_string())
}

fn is_url_like(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || Url::parse(value).is_ok()
}

fn resolve_url(base: &Url, value: &str) -> Result<Url, &'static str> {
    Url::parse(value)
        .or_else(|_| base.join(value))
        .map_err(|_| "module specifier could not be resolved as a URL")
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_IMPORT_MAP_ENTRIES, MAX_IMPORT_MAP_INTEGRITY_BYTES, MAX_IMPORT_MAP_RESOLUTIONS,
        NativeModuleImportMap,
    };

    const BASE: &str = "https://example.test/app/index.html";

    #[test]
    fn resolves_exact_and_longest_prefix_mappings() {
        let map = NativeModuleImportMap::parse(
            r#"{"imports":{"pkg":"/vendor/pkg.js","@app/":"/modules/","@app/ui/":"/ui/"}}"#,
            BASE,
        )
        .expect("valid import map");

        assert_eq!(
            map.resolve("https://example.test/app/main.js", "pkg")
                .unwrap(),
            "https://example.test/vendor/pkg.js"
        );
        assert_eq!(
            map.resolve("https://example.test/app/main.js", "@app/core.js")
                .unwrap(),
            "https://example.test/modules/core.js"
        );
        assert_eq!(
            map.resolve("https://example.test/app/main.js", "@app/ui/button.js")
                .unwrap(),
            "https://example.test/ui/button.js"
        );
    }

    #[test]
    fn canonicalizes_url_like_keys_and_specifiers() {
        let map = NativeModuleImportMap::parse(
            r#"{"imports":{"./lib/../entry.js":"./build/entry.js"}}"#,
            BASE,
        )
        .expect("valid URL-like mapping");

        assert_eq!(
            map.resolve("https://example.test/app/main.js", "./entry.js")
                .unwrap(),
            "https://example.test/app/build/entry.js"
        );
    }

    #[test]
    fn preserves_existing_mapping_when_import_maps_merge() {
        let mut map =
            NativeModuleImportMap::parse(r#"{"imports":{"pkg":"/first.js"}}"#, BASE).unwrap();
        map.merge(
            NativeModuleImportMap::parse(
                r#"{"imports":{"pkg":"/second.js","other":"/other.js"}}"#,
                BASE,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            map.resolve("https://example.test/app/main.js", "pkg")
                .unwrap(),
            "https://example.test/first.js"
        );
        assert_eq!(
            map.resolve("https://example.test/app/main.js", "other")
                .unwrap(),
            "https://example.test/other.js"
        );
    }

    #[test]
    fn resolves_referrer_scopes_from_most_specific_to_global() {
        let map = NativeModuleImportMap::parse(
            r#"{"imports":{"pkg":"/global/pkg.js","global/":"/global/"},"scopes":{"/vendor/":{"pkg":"/vendor/pkg.js","scoped/":"/vendor/scoped/","parentOnly":"/vendor/parent.js"},"/vendor/nested/":{"pkg":"/vendor/nested/pkg.js"}}}"#,
            BASE,
        )
        .expect("valid scoped import map");

        assert_eq!(
            map.resolve("https://example.test/vendor/nested/entry.js", "pkg")
                .unwrap(),
            "https://example.test/vendor/nested/pkg.js"
        );
        assert_eq!(
            map.resolve("https://example.test/vendor/entry.js", "pkg")
                .unwrap(),
            "https://example.test/vendor/pkg.js"
        );
        assert_eq!(
            map.resolve("https://example.test/vendor/nested/entry.js", "parentOnly")
                .unwrap(),
            "https://example.test/vendor/parent.js"
        );
        assert_eq!(
            map.resolve(
                "https://example.test/vendor/nested/entry.js",
                "scoped/button.js"
            )
            .unwrap(),
            "https://example.test/vendor/scoped/button.js"
        );
        assert_eq!(
            map.resolve("https://example.test/vendorish/entry.js", "pkg")
                .unwrap(),
            "https://example.test/global/pkg.js"
        );
        assert_eq!(
            map.resolve("https://example.test/app/main.js", "global/item.js")
                .unwrap(),
            "https://example.test/global/item.js"
        );
    }

    #[test]
    fn normalizes_scope_urls_and_merges_scope_entries_first_wins() {
        let mut map = NativeModuleImportMap::parse(
            r#"{"scopes":{"./lib/../vendor/":{"pkg":"/first.js"}}}"#,
            BASE,
        )
        .expect("valid relative scope");
        map.merge(
            NativeModuleImportMap::parse(
                r#"{"scopes":{"./vendor/":{"pkg":"/second.js","other":"/other.js"},"/new/":{"pkg":"/new.js"}}}"#,
                BASE,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            map.resolve("https://example.test/app/vendor/main.js", "pkg")
                .unwrap(),
            "https://example.test/first.js"
        );
        assert_eq!(
            map.resolve("https://example.test/app/vendor/main.js", "other")
                .unwrap(),
            "https://example.test/other.js"
        );
        assert_eq!(
            map.resolve("https://example.test/new/main.js", "pkg")
                .unwrap(),
            "https://example.test/new.js"
        );
    }

    #[test]
    fn resolved_module_specifiers_lock_affected_global_and_scoped_rules_only() {
        let mut map = NativeModuleImportMap::parse(
            r#"{"imports":{"pkg/":"/v1/"},"scopes":{"/scoped/":{"scope/":"/scope-v1/"}}}"#,
            BASE,
        )
        .expect("initial import map");
        let global_referrer = "https://example.test/app/main.js";
        let scoped_referrer = "https://example.test/scoped/main.js";

        assert_eq!(
            map.resolve_and_record(global_referrer, "pkg/item.js")
                .unwrap(),
            "https://example.test/v1/item.js"
        );
        assert_eq!(
            map.resolve_and_record(scoped_referrer, "scope/item.js")
                .unwrap(),
            "https://example.test/scope-v1/item.js"
        );
        map.merge(
            NativeModuleImportMap::parse(
                r#"{"imports":{"pkg/item.js":"/v2/item.js","later":"/later.js"},"scopes":{"/scoped/":{"scope/item.js":"/scope-v2/item.js","scope/new.js":"/scope-new.js"},"/other/":{"scope/item.js":"/other-scope.js"}}}"#,
                BASE,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            map.resolve_and_record(global_referrer, "pkg/item.js")
                .unwrap(),
            "https://example.test/v1/item.js"
        );
        assert_eq!(
            map.resolve(global_referrer, "later").unwrap(),
            "https://example.test/later.js"
        );
        assert_eq!(
            map.resolve(scoped_referrer, "scope/item.js").unwrap(),
            "https://example.test/scope-v1/item.js"
        );
        assert_eq!(
            map.resolve(scoped_referrer, "scope/new.js").unwrap(),
            "https://example.test/scope-new.js"
        );
        assert_eq!(
            map.resolve("https://example.test/other/main.js", "scope/item.js")
                .unwrap(),
            "https://example.test/other-scope.js"
        );
    }

    #[test]
    fn later_url_prefix_rules_cannot_rewrite_a_resolved_module_specifier() {
        let mut map = NativeModuleImportMap::default();
        let referrer = "https://example.test/app/main.js";
        assert_eq!(
            map.resolve_and_record(referrer, "./assets/module.js?variant=one")
                .unwrap(),
            "https://example.test/app/assets/module.js?variant=one"
        );

        map.merge(
            NativeModuleImportMap::parse(
                r#"{"imports":{"./assets/":"/replacement/","later":"/later.js"}}"#,
                BASE,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            map.resolve(referrer, "./assets/module.js?variant=one")
                .unwrap(),
            "https://example.test/app/assets/module.js?variant=one"
        );
        assert_eq!(
            map.resolve(referrer, "later").unwrap(),
            "https://example.test/later.js"
        );
    }

    #[test]
    fn preserves_fragments_in_mapped_targets_and_resolution_referrers() {
        let map = NativeModuleImportMap::parse(
            r#"{"imports":{"fragmented":"/modules/dep.js?variant=one#mapped"}}"#,
            BASE,
        )
        .expect("valid fragment-bearing import map");
        assert_eq!(
            map.resolve("https://example.test/app/main.js", "fragmented")
                .unwrap(),
            "https://example.test/modules/dep.js?variant=one#mapped"
        );

        let mut map = NativeModuleImportMap::default();
        assert_eq!(
            map.resolve_and_record("https://example.test/app/main.js#first", "./dep.js#first")
                .unwrap(),
            "https://example.test/app/dep.js#first"
        );
        assert_eq!(
            map.resolve_and_record("https://example.test/app/main.js#second", "./dep.js#second")
                .unwrap(),
            "https://example.test/app/dep.js#second"
        );
        assert_eq!(map.resolved.len(), 2);
    }

    #[test]
    fn resolved_module_specifier_records_fail_closed_at_their_bound() {
        let mut map = NativeModuleImportMap::default();
        for index in 0..MAX_IMPORT_MAP_RESOLUTIONS {
            map.resolve_and_record(
                "https://example.test/app/main.js",
                &format!("./module-{index}.js"),
            )
            .unwrap();
        }

        assert_eq!(
            map.resolve_and_record("https://example.test/app/main.js", "./module-overflow.js")
                .unwrap_err(),
            "resolved module specifier set exceeds its entry limit"
        );
    }

    #[test]
    fn ignores_unknown_top_level_members_and_non_url_integrity_entries() {
        let map = NativeModuleImportMap::parse(
            r#"{"imports":{"pkg":"/pkg.js"},"integrity":{"bare-relative":"sha256-ignored","./typed.js":7},"futureExtension":{}}"#,
            BASE,
        )
        .expect("unknown extension keys and invalid integrity entries are ignored");

        assert_eq!(
            map.resolve("https://example.test/app/main.js", "pkg")
                .unwrap(),
            "https://example.test/pkg.js"
        );
        assert_eq!(
            map.integrity_for_url("https://example.test/app/typed.js"),
            None
        );
        assert!(NativeModuleImportMap::parse(r#"{"integrity":[]}"#, BASE).is_err());
        assert_eq!(
            NativeModuleImportMap::default()
                .resolve("https://example.test/app/main.js", "unmapped")
                .unwrap_err(),
            "bare module specifiers require an import map"
        );
    }

    #[test]
    fn normalizes_and_merges_integrity_entries_first_wins() {
        let mut map =
            NativeModuleImportMap::parse(r#"{"integrity":{"./pkg.js":"sha384-first"}}"#, BASE)
                .expect("valid integrity map");
        map.merge(
            NativeModuleImportMap::parse(
                r#"{"integrity":{"https://example.test/app/pkg.js":"sha384-second","/other.js":"sha256-other"}}"#,
                BASE,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            map.integrity_for_url("https://example.test/app/pkg.js"),
            Some("sha384-first")
        );
        assert_eq!(
            map.integrity_for_url("https://example.test/other.js"),
            Some("sha256-other")
        );
    }

    #[test]
    fn normalized_import_map_key_collisions_follow_json_source_order() {
        let map = NativeModuleImportMap::parse(
            r#"{"imports":{"https://example.test/app/module.js":"/first.js","./module.js":"/second.js"},"scopes":{"https://example.test/app/":{"pkg":"/first-scoped.js"},"./":{"pkg":"/second-scoped.js"}},"integrity":{"https://example.test/app/module.js":"sha256-first","./module.js":"sha384-second"}}"#,
            BASE,
        )
        .expect("normalized duplicate keys retain the last source entry");

        assert_eq!(
            map.resolve("https://example.test/app/main.js", "./module.js")
                .unwrap(),
            "https://example.test/second.js"
        );
        assert_eq!(
            map.resolve("https://example.test/app/main.js", "pkg")
                .unwrap(),
            "https://example.test/second-scoped.js"
        );
        assert_eq!(
            map.integrity_for_url("https://example.test/app/module.js"),
            Some("sha384-second")
        );
    }

    #[test]
    fn integrity_entries_obey_metadata_and_combined_map_bounds_atomically() {
        let oversized_metadata = "x".repeat(MAX_IMPORT_MAP_INTEGRITY_BYTES + 1);
        let oversized = format!("{{\"integrity\":{{\"/module.js\":\"{oversized_metadata}\"}}}}");
        assert!(NativeModuleImportMap::parse(&oversized, BASE).is_err());

        let entries = (0..129)
            .map(|index| format!("\"/module{index}.js\":\"sha256-hash\""))
            .collect::<Vec<_>>()
            .join(",");
        assert!(
            NativeModuleImportMap::parse(&format!("{{\"integrity\":{{{entries}}}}}"), BASE)
                .is_err()
        );

        let imports = (0..MAX_IMPORT_MAP_ENTRIES - 1)
            .map(|index| format!("\"pkg{index}\":\"/pkg{index}.js\""))
            .collect::<Vec<_>>()
            .join(",");
        let initial =
            format!("{{\"imports\":{{{imports}}},\"integrity\":{{\"/one.js\":\"sha256-one\"}}}}");
        let mut map = NativeModuleImportMap::parse(&initial, BASE).unwrap();
        let original = map.clone();
        let newer = NativeModuleImportMap::parse(r#"{"integrity":{"/two.js":"sha256-two"}}"#, BASE)
            .unwrap();
        assert!(map.merge(newer).is_err());
        assert_eq!(map, original);
    }

    #[test]
    fn rejects_invalid_prefixes_credentials_and_oversized_maps() {
        assert!(
            NativeModuleImportMap::parse(r#"{"imports":{"pkg/":"/vendor/pkg"}}"#, BASE).is_err()
        );
        assert!(
            NativeModuleImportMap::parse(
                r#"{"imports":{"pkg":"https://user:secret@example.test/pkg.js"}}"#,
                BASE
            )
            .is_err()
        );
        assert!(
            NativeModuleImportMap::parse(
                r#"{"scopes":{"https://user:secret@example.test/":{"pkg":"/pkg.js"}}}"#,
                BASE
            )
            .is_err()
        );

        let entries = (0..129)
            .map(|index| format!("\"pkg{index}\":\"/pkg{index}.js\""))
            .collect::<Vec<_>>()
            .join(",");
        assert!(
            NativeModuleImportMap::parse(&format!("{{\"imports\":{{{entries}}}}}"), BASE).is_err()
        );
    }

    #[test]
    fn scope_and_entry_limits_are_combined_and_merge_is_atomic() {
        let entries = (0..128)
            .map(|index| format!("\"pkg{index}\":\"/pkg{index}.js\""))
            .collect::<Vec<_>>()
            .join(",");
        let with_one_scoped_entry = format!(
            "{{\"imports\":{{{entries}}},\"scopes\":{{\"/scope/\":{{\"extra\":\"/extra.js\"}}}}}}"
        );
        assert!(NativeModuleImportMap::parse(&with_one_scoped_entry, BASE).is_err());

        let scopes = (0..33)
            .map(|index| format!("\"/scope{index}/\":{{}}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(
            NativeModuleImportMap::parse(&format!("{{\"scopes\":{{{scopes}}}}}"), BASE).is_err()
        );

        let mut map =
            NativeModuleImportMap::parse(&format!("{{\"imports\":{{{entries}}}}}"), BASE).unwrap();
        let original = map.clone();
        let newer =
            NativeModuleImportMap::parse(r#"{"scopes":{"/scope/":{"extra":"/extra.js"}}}"#, BASE)
                .unwrap();
        assert!(map.merge(newer).is_err());
        assert_eq!(map, original);
    }
}
