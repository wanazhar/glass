use super::layout::NativePoint;

/// One committed local navigation entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHistoryEntry {
    pub url: String,
    pub revision: u64,
    pub scroll_offset: NativePoint,
}

/// Direction for explicit bounded history traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeHistoryDirection {
    Back,
    Forward,
}

/// Bounded history for the single native browsing context.
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

    pub(crate) fn push(&mut self, url: String, revision: u64, scroll_offset: NativePoint) {
        if let Some(current) = self.current {
            self.entries.truncate(current.saturating_add(1));
        }
        self.entries.push(NativeHistoryEntry {
            url,
            revision,
            scroll_offset,
        });
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

    pub fn can_go_back(&self) -> bool {
        self.current.is_some_and(|index| index > 0)
    }

    pub fn can_go_forward(&self) -> bool {
        self.current
            .is_some_and(|index| index.saturating_add(1) < self.entries.len())
    }

    pub(crate) fn target_index(&self, direction: NativeHistoryDirection) -> Option<usize> {
        let current = self.current?;
        match direction {
            NativeHistoryDirection::Back => current.checked_sub(1),
            NativeHistoryDirection::Forward => current
                .checked_add(1)
                .filter(|index| *index < self.entries.len()),
        }
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&NativeHistoryEntry> {
        self.entries.get(index)
    }

    pub(crate) fn update_current_scroll(&mut self, scroll_offset: NativePoint) {
        if let Some(index) = self.current
            && let Some(entry) = self.entries.get_mut(index)
        {
            entry.scroll_offset = scroll_offset;
        }
    }

    pub(crate) fn activate(&mut self, index: usize, revision: u64) -> Option<NativeHistoryEntry> {
        let entry = self.entries.get_mut(index)?;
        entry.revision = revision;
        self.current = Some(index);
        Some(entry.clone())
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
