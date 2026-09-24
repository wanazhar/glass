use std::collections::BTreeMap;
use url::Url;

const MAX_IMPORT_MAP_ENTRIES: usize = 128;
const MAX_IMPORT_MAP_SCOPES: usize = 32;

type NativeModuleSpecifierMap = BTreeMap<String, String>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NativeModuleImportMap {
    imports: NativeModuleSpecifierMap,
    scopes: BTreeMap<String, NativeModuleSpecifierMap>,
}

impl NativeModuleImportMap {
    pub(crate) fn parse(source: &str, base_url: &str) -> Result<Self, &'static str> {
        let base = Url::parse(base_url).map_err(|_| "import map base URL is invalid")?;
        let value: serde_json::Value =
            serde_json::from_str(source).map_err(|_| "import map is not valid JSON")?;
        let object = value
            .as_object()
            .ok_or("import map must be a JSON object")?;
        if object.keys().any(|key| key != "imports" && key != "scopes") {
            return Err("this import-map slice supports only imports and scopes");
        }

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
            for (scope, value) in scope_entries {
                let normalized_scope = resolve_url(&base, scope)?;
                if !normalized_scope.username().is_empty() || normalized_scope.password().is_some()
                {
                    return Err("import-map scopes must not contain credentials");
                }
                let normalized_scope = normalized_scope.to_string();
                let specifiers = parse_specifier_map(value, &base, &mut entry_count)?;
                if scopes.insert(normalized_scope, specifiers).is_some() {
                    return Err("import map contains duplicate normalized scopes");
                }
            }
        }

        Ok(Self { imports, scopes })
    }

    pub(crate) fn merge(&mut self, newer: Self) -> Result<(), &'static str> {
        let mut merged = self.clone();
        let mut entry_count = merged.entry_count();
        merge_specifier_map(&mut merged.imports, newer.imports, &mut entry_count)?;
        for (scope, newer_specifiers) in newer.scopes {
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
        *self = merged;
        Ok(())
    }

    pub(crate) fn resolve(&self, referrer: &str, specifier: &str) -> Result<String, &'static str> {
        let mut referrer_url =
            Url::parse(referrer).map_err(|_| "module referrer URL is invalid")?;
        referrer_url.set_fragment(None);
        let normalized_specifier = if is_url_like(specifier) {
            resolve_url(&referrer_url, specifier)?.to_string()
        } else {
            specifier.to_owned()
        };

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
            if let Some(target) = resolve_imports(scope_imports, &normalized_specifier)? {
                return Ok(target.to_string());
            }
        }

        let target = match resolve_imports(&self.imports, &normalized_specifier)? {
            Some(target) => target,
            None if is_url_like(specifier) => resolve_url(&referrer_url, specifier)?,
            None => return Err("bare module specifiers require an import map"),
        };
        if !target.username().is_empty() || target.password().is_some() {
            return Err("module URL must not contain credentials");
        }
        let mut target = target;
        target.set_fragment(None);
        Ok(target.to_string())
    }

    fn entry_count(&self) -> usize {
        self.imports.len() + self.scopes.values().map(BTreeMap::len).sum::<usize>()
    }
}

fn parse_specifier_map(
    value: &serde_json::Value,
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
    for (specifier, address) in imports {
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
        if normalized
            .insert(normalized_specifier, normalized_address)
            .is_some()
        {
            return Err("import map contains duplicate normalized specifiers");
        }
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
    let mut target = Url::parse(&joined).map_err(|_| "mapped module URL is invalid")?;
    if !target.username().is_empty() || target.password().is_some() {
        return Err("module URL must not contain credentials");
    }
    target.set_fragment(None);
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
    use super::NativeModuleImportMap;

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
    fn rejects_unmapped_bare_specifiers_and_unsupported_map_members() {
        assert_eq!(
            NativeModuleImportMap::default()
                .resolve("https://example.test/app/main.js", "unmapped")
                .unwrap_err(),
            "bare module specifiers require an import map"
        );
        assert!(NativeModuleImportMap::parse(r#"{"integrity":{}}"#, BASE).is_err());
        assert!(NativeModuleImportMap::parse(r#"{"unknown":{}}"#, BASE).is_err());
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
