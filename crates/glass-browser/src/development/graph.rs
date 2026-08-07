use super::{DevelopmentError, DevelopmentResult, MAX_FILE_BYTES};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
}

impl SourceLocation {
    pub fn new(path: impl Into<String>, start_line: u32, end_line: u32) -> DevelopmentResult<Self> {
        let path = path.into();
        let candidate = Path::new(&path);
        if path.is_empty()
            || candidate.is_absolute()
            || candidate.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || start_line == 0
            || end_line < start_line
        {
            return Err(DevelopmentError::InvalidInput(
                "source location requires a relative path and ordered one-based lines".into(),
            ));
        }
        Ok(Self {
            path,
            start_line,
            end_line,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LinkProvenance {
    ExplicitMarker,
    RuntimeObservation,
    StaticAnalysis,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LinkEvidence {
    pub provenance: LinkProvenance,
    pub detail: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLink {
    pub entity_id: String,
    pub source: SourceLocation,
    pub evidence: LinkEvidence,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentGraph {
    pub links: BTreeMap<String, Vec<RuntimeLink>>,
}

impl DevelopmentGraph {
    pub fn load(path: &Path) -> DevelopmentResult<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let metadata = fs::metadata(path)?;
        if metadata.len() > 2 * 1024 * 1024 {
            return Err(DevelopmentError::InvalidInput(
                "development graph exceeds the 2 MiB storage limit".into(),
            ));
        }
        let candidate: Self = serde_json::from_slice(&fs::read(path)?)?;
        let mut validated = Self::default();
        for links in candidate.links.into_values() {
            for link in links {
                validated.link(link)?;
            }
        }
        Ok(validated)
    }

    pub fn save(&self, path: &Path) -> DevelopmentResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(DevelopmentError::InvalidInput(
                "development graph exceeds the 2 MiB storage limit".into(),
            ));
        }
        let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
        fs::write(&temporary, bytes)?;
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(())
    }

    pub fn link(&mut self, link: RuntimeLink) -> DevelopmentResult<()> {
        if link.entity_id.is_empty() || link.entity_id.len() > 256 {
            return Err(DevelopmentError::InvalidInput(
                "runtime entity ID must be 1-256 bytes".into(),
            ));
        }
        if !link.evidence.confidence.is_finite()
            || !(0.0..=1.0).contains(&link.evidence.confidence)
            || link.evidence.detail.len() > 2048
        {
            return Err(DevelopmentError::InvalidInput(
                "link evidence must have finite confidence in [0,1] and bounded detail".into(),
            ));
        }
        let links = self.links.entry(link.entity_id.clone()).or_default();
        if let Some(existing) = links
            .iter_mut()
            .find(|existing| existing.source == link.source)
        {
            *existing = link;
        } else {
            if links.len() >= 16 {
                return Err(DevelopmentError::InvalidInput(
                    "one runtime entity cannot have more than 16 source links".into(),
                ));
            }
            links.push(link);
        }
        Ok(())
    }

    pub fn links_for(&self, entity_id: &str) -> &[RuntimeLink] {
        self.links.get(entity_id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn best_link(&self, entity_id: &str) -> Option<&RuntimeLink> {
        self.links_for(entity_id).iter().max_by(|left, right| {
            left.evidence
                .confidence
                .total_cmp(&right.evidence.confidence)
        })
    }

    pub fn entities_for_source(&self, path: &str, line: Option<u32>) -> Vec<&RuntimeLink> {
        let mut links = self
            .links
            .values()
            .flatten()
            .filter(|link| {
                link.source.path == path
                    && line.is_none_or(|line| {
                        (link.source.start_line..=link.source.end_line).contains(&line)
                    })
            })
            .collect::<Vec<_>>();
        links.sort_by(|left, right| {
            right
                .evidence
                .confidence
                .total_cmp(&left.evidence.confidence)
                .then_with(|| left.entity_id.cmp(&right.entity_id))
        });
        links
    }

    pub fn discover_explicit_markers(
        &mut self,
        root: &Path,
    ) -> DevelopmentResult<Vec<RuntimeLink>> {
        let mut discovered = Vec::new();
        discover_in_directory(root, root, &mut discovered)?;
        for link in &discovered {
            self.link(link.clone())?;
        }
        Ok(discovered)
    }
}

fn discover_in_directory(
    root: &Path,
    directory: &Path,
    discovered: &mut Vec<RuntimeLink>,
) -> DevelopmentResult<()> {
    if discovered.len() >= 512 {
        return Ok(());
    }
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if matches!(
            name.to_str(),
            Some(".git" | "target" | "node_modules" | ".glass")
        ) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            discover_in_directory(root, &path, discovered)?;
        } else if metadata.is_file() && metadata.len() <= MAX_FILE_BYTES as u64 {
            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if !matches!(
                extension,
                "rs" | "js" | "jsx" | "ts" | "tsx" | "vue" | "svelte" | "html"
            ) {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            for (index, line) in content.lines().enumerate() {
                for marker in ["data-glass-entity=\"", "glass:entity="] {
                    let Some(start) = line.find(marker).map(|start| start + marker.len()) else {
                        continue;
                    };
                    let tail = &line[start..];
                    let entity = tail
                        .split(|character: char| {
                            character.is_whitespace()
                                || matches!(character, '\"' | '\'' | '>' | ')' | ']' | '}')
                        })
                        .next()
                        .unwrap_or("");
                    if entity.is_empty() || entity.len() > 256 {
                        continue;
                    }
                    let relative = path
                        .strip_prefix(root)
                        .map_err(|_| DevelopmentError::PathOutsideWorkspace(path.clone()))?
                        .to_string_lossy()
                        .into_owned();
                    discovered.push(RuntimeLink {
                        entity_id: entity.into(),
                        source: SourceLocation::new(relative, index as u32 + 1, index as u32 + 1)?,
                        evidence: LinkEvidence {
                            provenance: LinkProvenance::ExplicitMarker,
                            detail: format!("source marker `{marker}`"),
                            confidence: 1.0,
                        },
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_link_preserves_provenance_instead_of_fabricating_certainty() {
        let source = SourceLocation::new("src/app.ts", 10, 12).unwrap();
        let mut graph = DevelopmentGraph::default();
        graph
            .link(RuntimeLink {
                entity_id: "button.submit".into(),
                source,
                evidence: LinkEvidence {
                    provenance: LinkProvenance::Inferred,
                    detail: "matched explicit entity marker".into(),
                    confidence: 0.61,
                },
            })
            .unwrap();
        assert_eq!(
            graph
                .best_link("button.submit")
                .unwrap()
                .evidence
                .confidence,
            0.61
        );
        assert_eq!(
            graph
                .best_link("button.submit")
                .unwrap()
                .evidence
                .provenance,
            LinkProvenance::Inferred
        );
    }

    #[test]
    fn explicit_markers_create_bidirectional_links() {
        let root = std::env::temp_dir().join(format!("glass-graph-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/button.tsx"),
            "<button data-glass-entity=\"action.checkout.submit\">Pay</button>\n",
        )
        .unwrap();
        let mut graph = DevelopmentGraph::default();
        graph.discover_explicit_markers(&root).unwrap();
        assert_eq!(
            graph
                .best_link("action.checkout.submit")
                .unwrap()
                .source
                .start_line,
            1
        );
        assert_eq!(
            graph.entities_for_source("src/button.tsx", Some(1)).len(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }
}
