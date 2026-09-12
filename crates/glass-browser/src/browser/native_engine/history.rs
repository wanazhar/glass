use super::layout::NativePoint;
use serde_json::Value;
use std::collections::BTreeMap;

/// One committed local navigation entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeHistoryEntry {
    pub url: String,
    pub revision: u64,
    pub scroll_offset: NativePoint,
    /// Per-element scroll offsets captured with this entry. Keys are stable
    /// node indexes within the bounded document representation; traversal
    /// drops entries that no longer resolve to a scroll container.
    pub nested_scroll_offsets: BTreeMap<u32, NativePoint>,
    /// JSON-backed state supplied by the page's History API.
    pub state: Value,
    /// Identity of the document lifecycle that owns this entry. Entries
    /// created by History API or fragment navigation share this identity;
    /// full resource commits allocate a new one.
    pub document_id: u64,
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
    next_document_id: u64,
}

impl NativeHistory {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            current: None,
            max_entries,
            next_document_id: 1,
        }
    }

    pub(crate) fn push(&mut self, url: String, revision: u64, scroll_offset: NativePoint) {
        self.push_with_state(
            url,
            revision,
            scroll_offset,
            &BTreeMap::new(),
            Value::Null,
            false,
        );
    }

    pub(crate) fn push_same_document(
        &mut self,
        url: String,
        revision: u64,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    ) {
        self.push_with_state(
            url,
            revision,
            scroll_offset,
            nested_scroll_offsets,
            Value::Null,
            true,
        );
    }

    pub(crate) fn push_with_state(
        &mut self,
        url: String,
        revision: u64,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
        state: Value,
        same_document: bool,
    ) {
        let document_id = if same_document {
            self.current()
                .map(|entry| entry.document_id)
                .unwrap_or_else(|| self.allocate_document_id())
        } else {
            self.allocate_document_id()
        };
        if let Some(current) = self.current {
            self.entries.truncate(current.saturating_add(1));
        }
        self.entries.push(NativeHistoryEntry {
            url,
            revision,
            scroll_offset,
            nested_scroll_offsets: nested_scroll_offsets.clone(),
            state,
            document_id,
        });
        if self.entries.len() > self.max_entries {
            let overflow = self.entries.len() - self.max_entries;
            self.entries.drain(..overflow);
        }
        self.current = self.entries.len().checked_sub(1);
    }

    pub(crate) fn replace_current(
        &mut self,
        url: String,
        revision: u64,
        scroll_offset: NativePoint,
    ) {
        self.replace_current_with_state(
            url,
            revision,
            scroll_offset,
            &BTreeMap::new(),
            Value::Null,
            false,
        );
    }

    pub(crate) fn replace_current_with_state(
        &mut self,
        url: String,
        revision: u64,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
        state: Value,
        same_document: bool,
    ) {
        let document_id = if same_document {
            self.current()
                .map(|entry| entry.document_id)
                .unwrap_or_else(|| self.allocate_document_id())
        } else {
            self.allocate_document_id()
        };
        let Some(current) = self.current else {
            self.push_with_state(
                url,
                revision,
                scroll_offset,
                nested_scroll_offsets,
                state,
                same_document,
            );
            return;
        };
        if let Some(entry) = self.entries.get_mut(current) {
            *entry = NativeHistoryEntry {
                url,
                revision,
                scroll_offset,
                nested_scroll_offsets: nested_scroll_offsets.clone(),
                state,
                document_id,
            };
        } else {
            self.push_with_state(
                url,
                revision,
                scroll_offset,
                nested_scroll_offsets,
                state,
                same_document,
            );
        }
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

    pub(crate) fn is_same_document(&self, index: usize) -> bool {
        let Some(current) = self.current.and_then(|index| self.entries.get(index)) else {
            return false;
        };
        self.entries
            .get(index)
            .is_some_and(|entry| entry.document_id == current.document_id)
    }

    pub(crate) fn update_current_scroll(
        &mut self,
        scroll_offset: NativePoint,
        nested_scroll_offsets: &BTreeMap<u32, NativePoint>,
    ) {
        if let Some(index) = self.current
            && let Some(entry) = self.entries.get_mut(index)
        {
            entry.scroll_offset = scroll_offset;
            entry.nested_scroll_offsets = nested_scroll_offsets.clone();
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

    fn allocate_document_id(&mut self) -> u64 {
        let id = self.next_document_id;
        self.next_document_id = self.next_document_id.saturating_add(1);
        id
    }
}
