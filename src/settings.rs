use std::borrow::Cow;

use crate::parsed_merge::{MergedChunk, ParsedMerge};

#[derive(Clone, Debug, PartialEq, Eq)]
/// Parameters controlling how the merged tree should be output.
pub struct DisplaySettings<'a> {
    /// Whether to show the base revision in the conflicts (true by default)
    pub diff3: bool,
    /// Whether to show compact conflicts or to expand them to fill an entire line
    pub compact: Option<bool>,
    /// The number of characters for conflict markers (7 by default)
    pub conflict_marker_size: Option<usize>,
    /// The string that identifies the left revision in conflict markers
    ///
    /// It can either:
    /// - miss completely (`<<<<<<<(newline)`), in which case we use "LEFT" as a placeholder.
    /// - be present but empty (`<<<<<<<(space)(newline`) -- a very unlikely case which we ignore.
    /// - be present and non-empty (`<<<<<<<(space)(revision name)(newline)`)
    pub left_revision_name: Option<Cow<'a, str>>,
    /// The string that identifies the base revision in conflict markers
    ///
    /// It can either:
    /// - miss completely (`|||||||(newline)`), in which case we use "BASE" as a placeholder.
    /// - be present but empty (`|||||||(space)(newline`) -- a very unlikely case which we ignore.
    /// - be present and non-empty (`|||||||(space)(revision name)(newline)`)
    pub base_revision_name: Option<Cow<'a, str>>,
    /// The string that identifies the right revision in conflict markers
    ///
    /// It can either:
    /// - miss completely (`>>>>>>>(newline)`), in which case we use "RIGHT" as a placeholder.
    /// - be present but empty (`>>>>>>>(space)(newline`) -- a very unlikely case which we ignore.
    /// - be present and non-empty (`>>>>>>>(space)(revision name)(newline)`)
    pub right_revision_name: Option<Cow<'a, str>>,
}

impl<'a> DisplaySettings<'a> {
    /// The value of `compact` if set, the default value otherwise
    pub fn compact_or_default(&self) -> bool {
        self.compact.unwrap_or(false)
    }

    /// The value of `conflict_marker_size` if set, the default value otherwise
    pub fn conflict_marker_size_or_default(&self) -> usize {
        self.conflict_marker_size.unwrap_or(7)
    }

    /// The value of `left_revision_name` if set, the default value otherwise
    pub fn left_revision_name_or_default(&self) -> &str {
        self.left_revision_name.as_deref().unwrap_or("LEFT")
    }

    /// The value of `base_revision_name` if set, the default value otherwise
    pub fn base_revision_name_or_default(&self) -> &str {
        self.base_revision_name.as_deref().unwrap_or("BASE")
    }

    /// The value of `right_revision_name` if set, the default value otherwise
    pub fn right_revision_name_or_default(&self) -> &str {
        self.right_revision_name.as_deref().unwrap_or("RIGHT")
    }

    /// The marker at the beginning of the "left" (first) part of a conflict.
    /// It does not contain any newline character.
    /// Uses the default values of `conflict_marker_size` and `left_revision_name` if not set
    pub fn left_marker_or_default(&self) -> String {
        format!(
            "{} {}",
            "<".repeat(self.conflict_marker_size_or_default()),
            self.left_revision_name_or_default()
        )
    }

    /// The marker at the beginning of the "base" part of a conflict.
    /// It does not contain any newline character.
    /// Uses the default values of `conflict_marker_size` and `base_revision_name` if not set
    pub fn base_marker_or_default(&self) -> String {
        format!(
            "{} {}",
            "|".repeat(self.conflict_marker_size_or_default()),
            self.base_revision_name_or_default()
        )
    }

    /// The marker at the end of the "right" (last) part of a conflict.
    /// It does not contain any newline character.
    /// Uses the default values of `conflict_marker_size` and `right_revision_name` if not set
    pub fn right_marker_or_default(&self) -> String {
        format!(
            "{} {}",
            ">".repeat(self.conflict_marker_size_or_default()),
            self.right_revision_name_or_default(),
        )
    }

    /// The marker before the beginning of "right" (last) part of a conflict.
    /// It does not contain any newline character.
    /// Uses the default values of `conflict_marker_size` if not set
    pub fn middle_marker_or_default(&self) -> String {
        "=".repeat(self.conflict_marker_size_or_default())
    }

    pub fn default_compact() -> Self {
        Self {
            compact: Some(true),
            ..Default::default()
        }
    }

    /// Update display settings by taking revision names from merge (if there are any conflicts)
    pub fn add_revision_names(&mut self, parsed_merge: &ParsedMerge<'a>) {
        if let Some((left_name, base_name, right_name)) =
            parsed_merge.chunks.iter().find_map(|chunk| match chunk {
                MergedChunk::Resolved { .. } => None,
                MergedChunk::Conflict {
                    left_name,
                    base_name,
                    right_name,
                    ..
                } => Some((*left_name, *base_name, *right_name)),
            })
        {
            self.left_revision_name = left_name.map(Cow::Borrowed);
            self.base_revision_name = base_name.map(Cow::Borrowed);
            self.right_revision_name = right_name.map(Cow::Borrowed);
        }
    }
}

impl Default for DisplaySettings<'_> {
    fn default() -> Self {
        Self {
            diff3: true,
            compact: Some(false),
            conflict_marker_size: None,
            left_revision_name: None,
            base_revision_name: None,
            right_revision_name: None,
        }
    }
}
