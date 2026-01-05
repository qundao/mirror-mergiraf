use std::borrow::Cow;

use regex::Regex;

use crate::parsed_merge::{MergedChunk, ParsedMerge};

pub const DEFAULT_CONFLICT_MARKER_SIZE: usize = 7;

/// The regexes for conflicts in diff2 and diff3 format.
///
/// The diff3 format[^1] allows representing conflicts where some (or all) sides may have no final
/// newline. In that case, there will be no newline at the end of the conflict, i.e. after the
/// right marker -- instead, a newline will be added to each side to ensure that the markers
/// coming after them are still placed at the beginning of a line. But the newline that
/// might've been a part of a conflict side is preserved as well.
///
/// [^1]: probably the diff2 format does so as well, but we don't care to parse it thoroughly,
/// as Mergiraf doesn't support it
#[derive(Clone, Debug)]
pub struct ConflictRegexes {
    /// The conflict marker size used in the regexes. Used in debug mode to ensure that the `size`
    /// field of `DisplaySettings` wasn't updated without updating these regexes as well.
    #[cfg(debug_assertions)]
    marker_size: usize,

    /// The regex for diff2 conflict format:
    /// ```txt
    /// <<<<<<< LEFT
    /// left content
    /// ======= BASE
    /// right content
    /// >>>>>>> RIGHT
    /// ```
    pub diff2: Regex,

    /// The regex for diff3 conflict format:
    /// ```txt
    /// <<<<<<< LEFT
    /// left content
    /// |||||||
    /// base content
    /// ======= BASE
    /// right content
    /// >>>>>>> RIGHT
    /// ```
    pub diff3: Regex,

    /// The regex for diff3 conflict format, where the final newline is not present.
    /// See struct's docs for more info
    pub diff3_no_newline: Regex,
}

#[derive(Clone, Debug, derive_more::PartialEq, derive_more::Eq)]
/// Parameters controlling how the merged tree should be output.
pub struct DisplaySettings<'a> {
    /// Whether to show the base revision in the conflicts (true by default)
    pub diff3: bool,
    /// Whether to show compact conflicts or to expand them to fill an entire line
    pub compact: Option<bool>,
    /// The number of characters for conflict markers (7 by default)
    conflict_marker_size: Option<usize>,
    /// The regexes for conflicts with marker lengths of size `conflict_marker_size`
    ///
    /// INVARIANT: the conflict marker size of regexes = `conflict_marker_size`

    // the regexes of two `DisplaySettings` are going to be "equal" iff their `conflict_marker_size`s are equal
    #[eq(skip)]
    conflict_regexes: Box<ConflictRegexes>,
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
    pub fn new(
        compact: Option<bool>,
        conflict_marker_size: Option<usize>,
        base_revision_name: Option<Cow<'a, str>>,
        left_revision_name: Option<Cow<'a, str>>,
        right_revision_name: Option<Cow<'a, str>>,
    ) -> Self {
        let size = conflict_marker_size.unwrap_or(DEFAULT_CONFLICT_MARKER_SIZE);
        Self {
            compact,
            conflict_marker_size,
            conflict_regexes: calculate_regexes(size),
            base_revision_name,
            left_revision_name,
            right_revision_name,
            diff3: true,
        }
    }

    /// The value of `compact` if set, the default value otherwise
    pub fn compact_or_default(&self) -> bool {
        self.compact.unwrap_or(false)
    }

    /// The value of `conflict_marker_size` if set, the default value otherwise
    pub fn conflict_marker_size_or_default(&self) -> usize {
        self.conflict_marker_size
            .unwrap_or(DEFAULT_CONFLICT_MARKER_SIZE)
    }

    pub fn set_conflict_marker_size(&mut self, new_size: usize) {
        if self.conflict_marker_size_or_default() != new_size {
            self.conflict_regexes = calculate_regexes(new_size);
        }
        self.conflict_marker_size = Some(new_size);
    }

    pub fn conflict_regexes(&self) -> &ConflictRegexes {
        // `debug_assert_eq!` will unfortunately not work here, as it uses merely
        // `if cfg!(debug_assertions)`, which doesn't stop the compilation error
        // "no field `marker_size` on type `Box<ConflictRegexes>`" from happening
        #[cfg(debug_assertions)]
        {
            assert_eq!(
                self.conflict_marker_size_or_default(),
                self.conflict_regexes.marker_size
            );
        }
        &self.conflict_regexes
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
        Self::new(Some(false), None, None, None, None)
    }
}

fn calculate_regexes(marker_size: usize) -> Box<ConflictRegexes> {
    let diff2_conflict = Regex::new(&format!(
        r"(?mx)
            ^
            <{{{marker_size}}} (?:\ (.*))? \r?\n
            ((?s:.)*? \r?\n)??
            ={{{marker_size}}}             \r?\n
            ((?s:.)*? \r?\n)??
            >{{{marker_size}}} (?:\ (.*))? \r?\n
            "
    ))
    .unwrap();

    let diff3_conflict = Regex::new(&format!(
        r"(?mx)
            ^
            <{{{marker_size}}}  (?:\ (.*))? \r?\n
            ((?s:.)*? \r?\n)??
            \|{{{marker_size}}} (?:\ (.*))? \r?\n
            ((?s:.)*? \r?\n)??
            ={{{marker_size}}}              \r?\n
            ((?s:.)*? \r?\n)??
            >{{{marker_size}}}  (?:\ (.*))? \r?\n
            "
    ))
    .unwrap();

    let diff3_conflict_no_newline = Regex::new(&format!(
        r"(?mx)
            ^
            <{{{marker_size}}}  (?:\ (.*))? \r?\n
            (?: ( (?s:.)*? )                \r?\n)?? # the newlines before the markers are
            \|{{{marker_size}}} (?:\ (.*))? \r?\n    # no longer part of conflicts sides themselves
            (?: ( (?s:.)*? )                \r?\n)??
            ={{{marker_size}}}              \r?\n
            (?: ( (?s:.)*? )                \r?\n)??
            >{{{marker_size}}}  (?:\ (.*))?     $    # no newline at the end
            "
    ))
    .unwrap();

    Box::new(ConflictRegexes {
        #[cfg(debug_assertions)]
        marker_size,
        diff2: diff2_conflict,
        diff3: diff3_conflict,
        diff3_no_newline: diff3_conflict_no_newline,
    })
}
