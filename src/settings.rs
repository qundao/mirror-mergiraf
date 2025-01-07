use crate::parsed_merge::{MergedChunk, ParsedMerge};

#[derive(Clone, Debug, PartialEq, Eq)]
/// Parameters controlling how the merged tree should be output.
pub struct DisplaySettings<'a> {
    /// Whether to show the base revision in the conflicts (true by default)
    pub diff3: bool,
    /// Whether to show compact conflicts or to expand them to fill an entire line
    pub compact: bool,
    /// The number of characters for conflict markers (7 by default)
    pub conflict_marker_size: usize,
    /// The string that identifies the left revision in conflict markers
    pub left_revision_name: &'a str,
    /// The string that identifies the base revision in conflict markers
    pub base_revision_name: &'a str,
    /// The string that identifies the right revision in conflict markers
    pub right_revision_name: &'a str,
}

impl<'a> DisplaySettings<'a> {
    /// The marker at the beginning of the "left" (first) part of a conflict.
    /// It does not contain any newline character.
    pub fn left_marker(&self) -> String {
        format!(
            "{} {}",
            "<".repeat(self.conflict_marker_size),
            self.left_revision_name
        )
    }

    /// The marker at the beginning of the "base" part of a conflict.
    /// It does not contain any newline character.
    pub fn base_marker(&self) -> String {
        format!(
            "{} {}",
            "|".repeat(self.conflict_marker_size),
            self.base_revision_name
        )
    }

    /// The marker at the end of the "right" (last) part of a conflict.
    /// It does not contain any newline character.
    pub fn right_marker(&self) -> String {
        format!(
            "{} {}",
            ">".repeat(self.conflict_marker_size),
            self.right_revision_name
        )
    }

    /// The marker before the beginning of "right" (last) part of a conflict.
    /// It does not contain any newline character.
    pub fn middle_marker(&self) -> String {
        "=".repeat(self.conflict_marker_size)
    }

    pub fn default_compact() -> Self {
        Self {
            diff3: true,
            compact: true,
            conflict_marker_size: 7,
            left_revision_name: "LEFT",
            base_revision_name: "BASE",
            right_revision_name: "RIGHT",
        }
    }

    /// Update display settings by taking revision names from merge (if there are any conflicts)
    pub fn add_revision_names(&mut self, parsed_merge: &ParsedMerge<'a>) {
        match parsed_merge.chunks.iter().find_map(|chunk| match chunk {
            MergedChunk::Resolved { .. } => None,
            MergedChunk::Conflict {
                left_name,
                base_name,
                right_name,
                ..
            } => Some((*left_name, *base_name, *right_name)),
        }) {
            Some((left_name, base_name, right_name))
                if !left_name.is_empty() && !base_name.is_empty() && !right_name.is_empty() =>
            {
                self.left_revision_name = left_name;
                self.base_revision_name = base_name;
                self.right_revision_name = right_name;
            }
            _ => {}
        }
    }
}

impl Default for DisplaySettings<'_> {
    fn default() -> Self {
        Self {
            diff3: true,
            compact: false,
            conflict_marker_size: 7,
            left_revision_name: "LEFT",
            base_revision_name: "BASE",
            right_revision_name: "RIGHT",
        }
    }
}
