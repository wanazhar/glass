use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchKind {
    File,
    BrowserEntity,
    Process,
    Event,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub kind: SearchKind,
    pub label: String,
    pub detail: String,
    pub score: u32,
}

pub fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    let query = query.to_ascii_lowercase();
    let candidate = candidate.to_ascii_lowercase();
    if query.is_empty() {
        return None;
    }
    if let Some(index) = candidate.find(&query) {
        return Some(10_000_u32.saturating_sub(index as u32 * 10 + candidate.len() as u32));
    }
    let mut position = 0;
    let mut gaps = 0_u32;
    for character in query.chars() {
        let offset = candidate[position..].find(character)?;
        gaps = gaps.saturating_add(offset as u32);
        position += offset + character.len_utf8();
    }
    Some(5_000_u32.saturating_sub(gaps * 10 + candidate.len() as u32))
}

pub fn rank(mut hits: Vec<SearchHit>, limit: usize) -> Vec<SearchHit> {
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.label.cmp(&right.label))
    });
    hits.truncate(limit.min(256));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_search_prefers_contiguous_matches() {
        assert!(
            fuzzy_score("checkout", "src/checkout/page.tsx").unwrap()
                > fuzzy_score("checkout", "components/check_out.tsx").unwrap()
        );
        assert!(fuzzy_score("missing", "src/app.rs").is_none());
    }
}
