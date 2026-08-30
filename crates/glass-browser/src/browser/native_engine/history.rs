/// One committed local navigation entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHistoryEntry {
    pub url: String,
    pub revision: u64,
}

/// Bounded history for the single Phase 1 browsing context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHistory {
    entries: Vec<NativeHistoryEntry>,
    current: Option<usize>,
    max_entries: usize,
}

impl NativeHistory {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            current: None,
            max_entries,
        }
    }

    pub(crate) fn push(&mut self, url: String, revision: u64) {
        if let Some(current) = self.current {
            self.entries.truncate(current.saturating_add(1));
        }
        self.entries.push(NativeHistoryEntry { url, revision });
        if self.entries.len() > self.max_entries {
            let overflow = self.entries.len() - self.max_entries;
            self.entries.drain(..overflow);
        }
        self.current = self.entries.len().checked_sub(1);
    }

    pub fn entries(&self) -> &[NativeHistoryEntry] {
        &self.entries
    }

    pub fn current(&self) -> Option<&NativeHistoryEntry> {
        self.current.and_then(|index| self.entries.get(index))
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
