use super::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
        if path.is_empty() || path.starts_with('/') || start_line == 0 || end_line < start_line {
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
}
