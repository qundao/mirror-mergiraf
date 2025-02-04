use std::{collections::HashMap, ops::Range};

use regex::Regex;

use crate::{
    matching::Matching,
    pcs::Revision,
    settings::DisplaySettings,
    tree::{Ast, AstNode},
};

pub(crate) const PARSED_MERGE_DIFF2_DETECTED: &str =
    "Mergiraf cannot solve conflicts displayed in the diff2 style";

/// A file which potentially contains merge conflicts, parsed as such.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParsedMerge<'a> {
    /// The actual contents of the parsed merge
    pub chunks: Vec<MergedChunk<'a>>,
    /// List of correspondences between sections of the reconstructed left revision and the merge output
    left: Vec<OffsetMap>,
    /// List of correspondences between sections of the reconstructed right revision and the merge output
    right: Vec<OffsetMap>,
    /// List of correspondences between sections of the reconstructed base revision and the merge output
    base: Vec<OffsetMap>,
}

/// A chunk in a file with merge conflicts: either a readily merged chunk or a conflict.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MergedChunk<'a> {
    /// A readily-merged chunk
    Resolved {
        /// The byte offset at which this merged chunk can be found
        offset: usize,
        /// Its textual contents (including the last newline before any conflict)
        contents: &'a str,
    },
    /// A diff3-style conflict
    ///
    /// The diff3 format allows representing conflicts where some (or all) sides may have no final
    /// newline. In that case, there will be no newline at the end of the conflict, i.e. after the
    /// right marker -- instead, a newline will be added to each side to ensure that the markers
    /// coming after them are still placed at the beginning of a line. But the newline that
    /// might've been a part of a conflict side is preserved as well.
    ///
    /// We recognize this property, and preserve whatever newline was present in the original sides.
    Conflict {
        /// The left part of the conflict, with the final newline preserved (if present)
        left: Option<&'a str>,
        /// The base (or ancestor) part of the conflict, with the final newline preserved (if present)
        base: Option<&'a str>,
        /// The right part of the conflict, with the final newline preserved (if present)
        right: Option<&'a str>,
        /// The name of the left revision (potentially empty)
        left_name: Option<&'a str>,
        /// The name of the base revision (potentially empty)
        base_name: Option<&'a str>,
        /// The name of the right revision (potentially empty)
        right_name: Option<&'a str>,
    },
}

/// A correspondence between a section of a reconstructed revision and the merge output
#[derive(Debug, Clone, Eq, PartialEq)]
struct OffsetMap {
    /// The start of the section in the reconstructed revision
    rev_start: usize,
    /// The start of the section in the original merge output
    merged_start: usize,
    /// The common length of the section on both sides
    length: usize,
}

impl<'a> ParsedMerge<'a> {
    /// Parse a file into a series of chunks.
    /// Fails if the conflict markers do not appear in a consistent order.
    pub(crate) fn parse(source: &'a str, settings: &DisplaySettings) -> Result<Self, String> {
        let marker_size = settings.conflict_marker_size_or_default();

        let mut chunks = Vec::new();

        let left_marker = "<".repeat(marker_size);
        let base_marker = r"\|".repeat(marker_size);
        let middle_marker = "=".repeat(marker_size);
        let right_marker = ">".repeat(marker_size);

        let diff2conflict = Regex::new(&format!(
            r"(?mx)
            ^{left_marker} (?:\ (.*))? \r?\n
            ((?s:.)*? \r?\n)??
            {middle_marker}            \r?\n
            ((?s:.)*? \r?\n)??
            {right_marker} (?:\ (.*))? \r?\n
            "
        ))
        .unwrap();

        let diff3conflict = Regex::new(&format!(
            r"(?mx)
            ^{left_marker} (?:\ (.*))? \r?\n
            ((?s:.)*? \r?\n)??
            {base_marker}  (?:\ (.*))? \r?\n
            ((?s:.)*? \r?\n)??
            {middle_marker}            \r?\n
            ((?s:.)*? \r?\n)??
            {right_marker} (?:\ (.*))? \r?\n
            "
        ))
        .unwrap();

        let diff3conflict_no_newline = Regex::new(&format!(
            r"(?mx)
            ^{left_marker} (?:\ (.*))? \r?\n
            (?: ( (?s:.)*? )           \r?\n)?? # the newlines before the markers are
            {base_marker}  (?:\ (.*))? \r?\n    # no longer part of conflicts sides themselves
            (?: ( (?s:.)*? )           \r?\n)??
            {middle_marker}            \r?\n
            (?: ( (?s:.)*? )           \r?\n)??
            {right_marker} (?:\ (.*))?     $    # no newline at the end
            "
        ))
        .unwrap();

        let mut remaining_source = source;
        while !remaining_source.is_empty() {
            let diff3_captures = diff3conflict.captures(remaining_source);
            let diff3_no_newline_captures = diff3conflict_no_newline.captures(remaining_source);

            // the 3 regexes each match more things than the last in this order:
            // 1) diff2            -- by ignoring the base marker and base rev text
            // 2) diff3_no_newline -- by, well, ignoring the final newline
            // 3) diff3            -- only matches a diff3 conflict ending with a newline
            //
            // so we run them in the opposite order:
            // 1) if diff3 matches, take that
            // 2) if diff3_no_newline matches, take that
            // 3) if diff2 matches, then we know that this isn't a misrecognized diff3, and bail out
            let resolved_end = if let Some(occurrence) =
                (diff3_captures.as_ref()).or(diff3_no_newline_captures.as_ref())
            {
                occurrence
                    .get(0)
                    .expect("whole match is guaranteed to exist")
                    .start()
            } else if diff2conflict.is_match(remaining_source) {
                return Err(PARSED_MERGE_DIFF2_DETECTED.to_owned());
            } else {
                remaining_source.len()
            };

            if resolved_end > 0 {
                // SAFETY: `remaining_source` is derived from `source`, so `offset_from` makes sense
                let offset = unsafe { remaining_source.as_ptr().offset_from(source.as_ptr()) }
                    .try_into()
                    .expect("`remaining_source` points to the _remainder_ of `source`, so `offset` is positive");
                chunks.push(MergedChunk::Resolved {
                    offset,
                    contents: &remaining_source[..resolved_end],
                });
            }

            if let Some(captures) = (diff3_captures.as_ref()).or(diff3_no_newline_captures.as_ref())
            {
                chunks.push(MergedChunk::Conflict {
                    left_name: captures.get(1).map(|m| m.as_str()),
                    left: captures.get(2).map(|m| m.as_str()),
                    base_name: captures.get(3).map(|m| m.as_str()),
                    base: captures.get(4).map(|m| m.as_str()),
                    right: captures.get(5).map(|m| m.as_str()),
                    right_name: captures.get(6).map(|m| m.as_str()),
                });

                remaining_source = &remaining_source[captures
                    .get(0)
                    .expect("whole match is guaranteed to exist")
                    .end()..];
            } else {
                remaining_source = &remaining_source[resolved_end..];
            }
        }
        Ok(ParsedMerge::new(chunks))
    }

    /// Construct a parsed merge by indexing the provided chunks
    fn new(chunks: Vec<MergedChunk<'a>>) -> Self {
        let mut left_offset = 0;
        let mut base_offset = 0;
        let mut right_offset = 0;
        let mut left = Vec::new();
        let mut base = Vec::new();
        let mut right = Vec::new();
        for chunk in &chunks {
            match chunk {
                MergedChunk::Resolved { offset, contents } => {
                    let length = contents.len();
                    left.push(OffsetMap {
                        rev_start: left_offset,
                        merged_start: *offset,
                        length,
                    });
                    base.push(OffsetMap {
                        rev_start: base_offset,
                        merged_start: *offset,
                        length,
                    });
                    right.push(OffsetMap {
                        rev_start: right_offset,
                        merged_start: *offset,
                        length,
                    });
                    left_offset += length;
                    base_offset += length;
                    right_offset += length;
                }
                MergedChunk::Conflict {
                    left, base, right, ..
                } => {
                    left_offset += left.map_or(0, str::len);
                    base_offset += base.map_or(0, str::len);
                    right_offset += right.map_or(0, str::len);
                }
            }
        }
        ParsedMerge {
            chunks,
            left,
            right,
            base,
        }
    }

    /// Reconstruct the source of a revision based on the merged output.
    ///
    /// Because some changes from both revisions have likely already been
    /// merged in the non-conflicting sections, this is not the original revision,
    /// but rather a half-merged version of it.
    pub(crate) fn reconstruct_revision(&self, revision: Revision) -> String {
        self.chunks
            .iter()
            .map(|chunk| match *chunk {
                MergedChunk::Resolved { contents, .. } => contents,
                MergedChunk::Conflict {
                    left, base, right, ..
                } => match revision {
                    Revision::Base => base.unwrap_or_default(),
                    Revision::Left => left.unwrap_or_default(),
                    Revision::Right => right.unwrap_or_default(),
                },
            })
            .collect()
    }

    /// Find out at which index of the merged file a byte range in the reconstructed revision can be found.
    ///
    /// The returned index will only be returned if the entire range of the reconstructed
    /// revision lies in a fully merged part of the merged file (without overlapping any conflict).
    pub(crate) fn rev_range_to_merged_range(
        &self,
        range: &Range<usize>,
        revision: Revision,
    ) -> Option<Range<usize>> {
        let length = range.end - range.start;
        let start = range.start;
        let matched_start = match revision {
            Revision::Base => Self::binary_search(&self.base, start, length),
            Revision::Left => Self::binary_search(&self.left, start, length),
            Revision::Right => Self::binary_search(&self.right, start, length),
        }?;
        Some(matched_start..matched_start + length)
    }

    /// Generate a matching between the ASTs of two revisions generated by this parsed merge,
    /// by matching elements whenever they correspond to the same merged range.
    pub(crate) fn generate_matching<'b>(
        &self,
        first_revision: Revision,
        second_revision: Revision,
        first_tree: &'b Ast<'b>,
        second_tree: &'b Ast<'b>,
    ) -> Matching<'b> {
        let first_index = self.index_tree_by_merged_ranges(first_revision, first_tree);
        let second_index = self.index_tree_by_merged_ranges(second_revision, second_tree);
        let mut matching = Matching::new();
        for (range, first_node) in &first_index {
            if let Some(second_node) = second_index.get(range) {
                matching.add(first_node, second_node);
            }
        }
        matching
    }

    fn index_tree_by_merged_ranges<'b>(
        &self,
        revision: Revision,
        tree: &'b Ast<'b>,
    ) -> HashMap<Range<usize>, &'b AstNode<'b>> {
        let mut map = HashMap::new();
        self.recursively_index_node(revision, tree.root(), &mut map);
        map
    }

    fn recursively_index_node<'b>(
        &self,
        revision: Revision,
        node: &'b AstNode<'b>,
        map: &mut HashMap<Range<usize>, &'b AstNode<'b>>,
    ) {
        match self.rev_range_to_merged_range(&node.byte_range, revision) {
            Some(range) => {
                map.insert(range, node);
            }
            None => {
                node.children
                    .iter()
                    .for_each(|child| self.recursively_index_node(revision, child, map));
            }
        };
    }

    /// Render the parsed merge back to a string representation
    pub(crate) fn render(&self, settings: &DisplaySettings) -> String {
        let mut result = String::new();
        for chunk in &self.chunks {
            match chunk {
                MergedChunk::Resolved { contents, .. } => result.push_str(contents),
                MergedChunk::Conflict {
                    left, base, right, ..
                } => {
                    // we check whether all 3 sides of the conflict[^1] used ot end with a newline.
                    // If any of them didn't, then the conflict should be rendered in a special way:
                    // - a newline is added to all three sides (even if the particular side used to
                    //   have a newline already)
                    // - *no* newline is added after the right marker, i.e. at the end of conflict
                    //
                    // [^1]: the ones that weren't empty, anyway
                    let add_after_right_marker = if let (None, None, None) = (base, left, right) {
                        unreachable!("wouldn't have been a conflict in the first place")
                    } else {
                        left.is_none_or(|l| l.ends_with('\n'))
                            && base.is_none_or(|b| b.ends_with('\n'))
                            && right.is_none_or(|r| r.ends_with('\n'))
                    };
                    let add_after_lines = !add_after_right_marker;

                    result.push_str(&settings.left_marker_or_default());
                    result.push('\n');
                    result.push_str(left.unwrap_or_default());
                    if add_after_lines {
                        result.push('\n');
                    }

                    if settings.diff3 {
                        result.push_str(&settings.base_marker_or_default());
                        result.push('\n');
                        result.push_str(base.unwrap_or_default());
                        if add_after_lines {
                            result.push('\n');
                        }
                    }

                    result.push_str(&settings.middle_marker());
                    result.push('\n');

                    result.push_str(right.unwrap_or_default());
                    if add_after_lines {
                        result.push('\n');
                    }
                    result.push_str(&settings.right_marker_or_default());
                    if add_after_right_marker {
                        result.push('\n');
                    }
                }
            }
        }
        result
    }

    fn binary_search(slice: &[OffsetMap], start: usize, length: usize) -> Option<usize> {
        let mut left = 0;
        let mut right = slice.len();
        while left < right {
            let guess = (left + right) / 2;
            let offset = slice
                .get(guess)
                .expect("Programming error in binary search, oops!");
            if offset.rev_start <= start && start + length <= offset.rev_start + offset.length {
                return Some(offset.merged_start + start - offset.rev_start);
            } else if left + 1 == right {
                break;
            }
            if offset.rev_start <= start {
                left = guess;
            }
            if offset.rev_start >= start {
                right = guess;
            }
        }
        None
    }

    // Number of conflicts in this merge
    pub fn conflict_count(&self) -> usize {
        self.chunks
            .iter()
            .filter(|chunk| matches!(chunk, MergedChunk::Conflict { .. }))
            .count()
    }

    // Number of bytes of conflicting content, which is an attempt
    // at quantifying the effort it takes to resolve the conflicts.
    pub fn conflict_mass(&self) -> usize {
        self.chunks
            .iter()
            .map(|chunk| match chunk {
                MergedChunk::Resolved { .. } => 0,
                MergedChunk::Conflict {
                    base, left, right, ..
                } => {
                    base.map_or(0, str::len) + left.map_or(0, str::len) + right.map_or(0, str::len)
                }
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn parse() {
        let source = "\nwe reached a junction.\n<<<<<<< left\nlet's go to the left!\n||||||| base\nwhere should we go?\n=======\nturn right please!\n>>>>>>>\nrest of file\n";
        let parsed =
            ParsedMerge::parse(source, &Default::default()).expect("unexpected parse error");

        let expected_parse = ParsedMerge::new(vec![
            MergedChunk::Resolved {
                offset: 0,
                contents: "\nwe reached a junction.\n",
            },
            MergedChunk::Conflict {
                left: Some("let's go to the left!\n"),
                base: Some("where should we go?\n"),
                right: Some("turn right please!\n"),
                left_name: Some("left"),
                base_name: Some("base"),
                right_name: None,
            },
            MergedChunk::Resolved {
                offset: 127,
                contents: "rest of file\n",
            },
        ]);

        assert_eq!(parsed, expected_parse);
        assert_eq!(
            parsed.reconstruct_revision(Revision::Base),
            "\nwe reached a junction.\nwhere should we go?\nrest of file\n"
        );
        assert_eq!(
            parsed.reconstruct_revision(Revision::Left),
            "\nwe reached a junction.\nlet's go to the left!\nrest of file\n"
        );
        assert_eq!(
            parsed.reconstruct_revision(Revision::Right),
            "\nwe reached a junction.\nturn right please!\nrest of file\n"
        );

        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 1, end: 11 }, Revision::Base),
            Some(Range { start: 1, end: 11 })
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 1, end: 11 }, Revision::Left),
            Some(Range { start: 1, end: 11 })
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 1, end: 11 }, Revision::Right),
            Some(Range { start: 1, end: 11 })
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 11, end: 41 }, Revision::Base),
            None
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 11, end: 41 }, Revision::Left),
            None
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 11, end: 41 }, Revision::Right),
            None
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 25, end: 28 }, Revision::Base),
            None
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 25, end: 28 }, Revision::Left),
            None
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 25, end: 28 }, Revision::Right),
            None
        );
        #[rustfmt::skip]
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 45, end: 49 }, Revision::Base),
            Some(Range { start: 128, end: 132 })
        );
        #[rustfmt::skip]
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 47, end: 49 }, Revision::Left),
            Some(Range { start: 128, end: 130 })
        );
        #[rustfmt::skip]
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 45, end: 48 }, Revision::Right),
            Some(Range { start: 129, end: 132 })
        );
        #[rustfmt::skip]
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 180, end: 183 }, Revision::Base),
            None
        );
        #[rustfmt::skip]
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 190, end: 193 }, Revision::Left),
            None
        );
        #[rustfmt::skip]
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 200, end: 203 }, Revision::Right),
            None
        );
    }

    #[test]
    fn parse_start_with_conflict() {
        let source = "<<<<<<< left\nlet's go to the left!\n||||||| base\nwhere should we go?\n=======\nturn right please!\n>>>>>>>\nrest of file\n";
        let parsed =
            ParsedMerge::parse(source, &Default::default()).expect("unexpected parse error");

        let expected_parse = ParsedMerge::new(vec![
            MergedChunk::Conflict {
                left: Some("let's go to the left!\n"),
                base: Some("where should we go?\n"),
                right: Some("turn right please!\n"),
                left_name: Some("left"),
                base_name: Some("base"),
                right_name: None,
            },
            MergedChunk::Resolved {
                offset: 103,
                contents: "rest of file\n",
            },
        ]);

        assert_eq!(parsed, expected_parse);
        assert_eq!(
            parsed.reconstruct_revision(Revision::Base),
            "where should we go?\nrest of file\n"
        );
        assert_eq!(
            parsed.reconstruct_revision(Revision::Left),
            "let's go to the left!\nrest of file\n"
        );
        assert_eq!(
            parsed.reconstruct_revision(Revision::Right),
            "turn right please!\nrest of file\n"
        );
    }

    #[test]
    fn parse_end_with_conflict() {
        let source = "\nwe reached a junction.\n<<<<<<< left\nlet's go to the left!\n||||||| base\nwhere should we go?\n=======\nturn right please!\n>>>>>>>\n";
        let parsed =
            ParsedMerge::parse(source, &Default::default()).expect("unexpected parse error");

        let expected_parse = ParsedMerge::new(vec![
            MergedChunk::Resolved {
                offset: 0,
                contents: "\nwe reached a junction.\n",
            },
            MergedChunk::Conflict {
                left: Some("let's go to the left!\n"),
                base: Some("where should we go?\n"),
                right: Some("turn right please!\n"),
                left_name: Some("left"),
                base_name: Some("base"),
                right_name: None,
            },
        ]);

        assert_eq!(parsed, expected_parse);
        assert_eq!(
            parsed.reconstruct_revision(Revision::Base),
            "\nwe reached a junction.\nwhere should we go?\n"
        );
        assert_eq!(
            parsed.reconstruct_revision(Revision::Left),
            "\nwe reached a junction.\nlet's go to the left!\n"
        );
        assert_eq!(
            parsed.reconstruct_revision(Revision::Right),
            "\nwe reached a junction.\nturn right please!\n"
        );
    }

    #[test]
    fn parse_diffy_imara() {
        let source = "my_struct_t instance = {\n<<<<<<< LEFT\n    .foo = 3,\n    .bar = 2,\n||||||| BASE\n    .foo = 3,\n=======\n>>>>>>> RIGHT\n};\n";

        let parsed = ParsedMerge::parse(source, &Default::default()).expect("could not parse!");

        let expected_parse = ParsedMerge::new(vec![
            MergedChunk::Resolved {
                offset: 0,
                contents: "my_struct_t instance = {\n",
            },
            MergedChunk::Conflict {
                left: Some("    .foo = 3,\n    .bar = 2,\n"),
                base: Some("    .foo = 3,\n"),
                right: None,
                left_name: Some("LEFT"),
                base_name: Some("BASE"),
                right_name: Some("RIGHT"),
            },
            MergedChunk::Resolved {
                offset: 115,
                contents: "};\n",
            },
        ]);

        assert_eq!(parsed, expected_parse);
        assert_eq!(parsed.conflict_count(), 1);
        assert_eq!(parsed.conflict_mass(), 42);

        // render the parsed conflict and check it's equal to the source
        let rendered = parsed.render(&DisplaySettings::default());

        assert_eq!(rendered, source);
    }

    #[test]
    fn parse_diff2() {
        let source = "my_struct_t instance = {\n<<<<<<< LEFT\n    .foo = 3,\n    .bar = 2,\n=======\n>>>>>>> RIGHT\n};\n";

        let parse_err = ParsedMerge::parse(source, &Default::default())
            .expect_err("expected a parse failure for diff2 conflicts");

        assert_eq!(parse_err, PARSED_MERGE_DIFF2_DETECTED);
    }

    #[test]
    fn parse_non_standard_conflict_marker_size() {
        let parsed_expected = ParsedMerge::new(vec![
            MergedChunk::Resolved {
                offset: 0,
                contents: "resolved line\n",
            },
            MergedChunk::Conflict {
                left: Some("left line\n"),
                base: Some("base line\n"),
                right: Some("right line\n"),
                left_name: Some("LEFT"),
                base_name: Some("BASE"),
                right_name: Some("RIGHT"),
            },
        ]);

        let conflict_with_4 = "resolved line\n<<<< LEFT\nleft line\n|||| BASE\nbase line\n====\nright line\n>>>> RIGHT\n";
        let parsed_with_4 = ParsedMerge::parse(
            conflict_with_4,
            &DisplaySettings {
                conflict_marker_size: Some(4),
                ..Default::default()
            },
        )
        .expect("could not parse a conflict with `conflict_marker_size=4`");
        assert_eq!(parsed_with_4, parsed_expected);

        let conflict_with_9 = "resolved line\n<<<<<<<<< LEFT\nleft line\n||||||||| BASE\nbase line\n=========\nright line\n>>>>>>>>> RIGHT\n";
        let parsed_with_9 = ParsedMerge::parse(
            conflict_with_9,
            &DisplaySettings {
                conflict_marker_size: Some(9),
                ..Default::default()
            },
        )
        .expect("could not parse a conflict with `conflict_marker_size=9`");
        assert_eq!(parsed_with_9, parsed_expected);
    }

    #[test]
    fn parse_left_marker_not_at_line_start() {
        let source = "my_struct_t instance = {\n <<<<<<< LIAR LEFT\n    .foo = 3,\n    .bar = 2,\n||||||| BASE\n    .foo = 3,\n=======\n>>>>>>> RIGHT\n};\n";
        let parsed = ParsedMerge::parse(source, &Default::default())
            .expect("should just not see this conflict at all");

        let expected_parse = ParsedMerge::new(vec![MergedChunk::Resolved {
            offset: 0,
            contents: source,
        }]);

        assert_eq!(parsed, expected_parse);
    }

    #[test]
    fn parse_base_marker_not_at_line_start() {
        let source = "my_struct_t instance = {\n<<<<<<< LEFT\n    .foo = 3,\n    .bar = 2,\n ||||||| LIAR BASE\n    .foo = 3,\n=======\n>>>>>>> RIGHT\n};\n";
        let parse_err = ParsedMerge::parse(source, &Default::default()).expect_err(
            "because of the missing base marker, this should like a diff2-style conflict",
        );

        assert_eq!(parse_err, PARSED_MERGE_DIFF2_DETECTED);
    }

    #[test]
    fn parse_middle_marker_not_at_line_start() {
        let source = "my_struct_t instance = {\n<<<<<<< LEFT\n    .foo = 3,\n    .bar = 2,\n||||||| BASE\n    .foo = 3,\n =======\n>>>>>>> RIGHT\n};\n";
        let parsed = ParsedMerge::parse(source, &Default::default())
            .expect("should ignore the malformed conflict");

        let expected = ParsedMerge::new(vec![MergedChunk::Resolved {
            offset: 0,
            contents: source,
        }]);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_right_marker_not_at_line_start() {
        let source = "my_struct_t instance = {\n<<<<<<< LEFT\n    .foo = 3,\n    .bar = 2,\n||||||| BASE\n    .foo = 3,\n=======\n >>>>>>> LIAR RIGHT\n};\n";
        let parsed = ParsedMerge::parse(source, &Default::default())
            .expect("should ignore the malformed conflict");

        let expected = ParsedMerge::new(vec![MergedChunk::Resolved {
            offset: 0,
            contents: source,
        }]);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_diff3_then_diff3_is_lazy() {
        let source = "<<<<<<< LEFT\n// a comment\n||||||| BASE\n=======\n// hi\n>>>>>>> RIGHT\n<<<<<<< LEFT\nuse bytes;\n||||||| BASE\nuse io;\n=======\nuse os;\n>>>>>>> RIGHT\n";

        let parsed = ParsedMerge::parse(source, &Default::default()).expect("could not parse!");

        let unwanted_non_lazy = ParsedMerge::new(vec![MergedChunk::Conflict {
            left_name: Some("LEFT"),
            left: Some("// a comment\n"),
            base_name: Some("BASE"),
            base: Some(
                "=======\n// hi\n>>>>>>> RIGHT\n<<<<<<< LEFT\nuse bytes;\n||||||| BASE\nuse io;\n",
            ),
            right: Some("use os;\n"),
            right_name: Some("RIGHT"),
        }]);

        assert_ne!(
            parsed, unwanted_non_lazy,
            "the regex is greedy -- it should've stopped after the first 'RIGHT'!"
        );

        let expected = ParsedMerge::new(vec![
            MergedChunk::Conflict {
                left_name: Some("LEFT"),
                left: Some("// a comment\n"),
                base_name: Some("BASE"),
                base: None,
                right: Some("// hi\n"),
                right_name: Some("RIGHT"),
            },
            MergedChunk::Conflict {
                left_name: Some("LEFT"),
                left: Some("use bytes;\n"),
                base_name: Some("BASE"),
                base: Some("use io;\n"),
                right: Some("use os;\n"),
                right_name: Some("RIGHT"),
            },
        ]);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_diff3_then_diff3_wo_newline() {
        let source = "<<<<<<< LEFT\n// a comment\n||||||| BASE\n=======\n// hi\n>>>>>>> RIGHT\n<<<<<<< LEFT\nuse bytes;\n||||||| BASE\nuse io;\n=======\nuse os;\n>>>>>>> RIGHT";
        let parsed = ParsedMerge::parse(source, &Default::default()).expect("could not parse!");

        let expected = ParsedMerge::new(vec![
            MergedChunk::Conflict {
                left_name: Some("LEFT"),
                left: Some("// a comment\n"),
                base_name: Some("BASE"),
                base: None,
                right: Some("// hi\n"),
                right_name: Some("RIGHT"),
            },
            MergedChunk::Conflict {
                left_name: Some("LEFT"),
                left: Some("use bytes;"),
                base_name: Some("BASE"),
                base: Some("use io;"),
                right: Some("use os;"),
                right_name: Some("RIGHT"),
            },
        ]);

        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_diff3_is_with_final_newline_when_possible() {
        let source = "<<<<<<< left\nlet's go to the left!\n||||||| base\nwhere should we go?\n=======\nturn right please!\n>>>>>>>\n";

        let parsed = ParsedMerge::parse(source, &Default::default()).expect("could not parse!");

        let unwanted_wo_final_newline = ParsedMerge::new(vec![
            MergedChunk::Conflict {
                left_name: Some("left"),
                left: Some("let's go to the left!"),
                base_name: Some("base"),
                base: Some("where should we go?"),
                right: Some("turn right please!"),
                right_name: None,
            },
            MergedChunk::Resolved {
                offset: 102,
                contents: "\n",
            },
        ]);

        assert_ne!(parsed, unwanted_wo_final_newline);
    }

    #[test]
    fn matching() {
        let ctx = ctx();
        let source = "struct MyType {\n    field: bool,\n<<<<<<< LEFT\n    foo: int,\n    bar: String,\n||||||| BASE\n    foo: String,\n=======\n>>>>>>> RIGHT\n};\n";
        let parsed = ParsedMerge::parse(source, &Default::default()).expect("could not parse!");

        let left_rev = parsed.reconstruct_revision(Revision::Left);
        let right_rev = parsed.reconstruct_revision(Revision::Right);

        let parsed_left = ctx.parse_rust(&left_rev);
        let parsed_right = ctx.parse_rust(&right_rev);

        let matching =
            parsed.generate_matching(Revision::Left, Revision::Right, &parsed_left, &parsed_right);

        let mytype_left = parsed_left.root().child(0).unwrap().child(1).unwrap();
        let mytype_right = parsed_right.root().child(0).unwrap().child(1).unwrap();
        let closing_bracket_left = parsed_left
            .root()
            .child(0)
            .unwrap()
            .child(2)
            .unwrap()
            .child(7)
            .unwrap();
        let closing_bracket_right = parsed_right
            .root()
            .child(0)
            .unwrap()
            .child(2)
            .unwrap()
            .child(3)
            .unwrap();

        assert!(matching.are_matched(mytype_left, mytype_right));
        assert!(matching.are_matched(closing_bracket_left, closing_bracket_right));

        assert_eq!(matching.len(), 7);
    }

    #[test]
    fn render_non_standard_conflict_marker_size() {
        let merge = ParsedMerge::new(vec![
            MergedChunk::Resolved {
                offset: 0,
                contents: "resolved line\n",
            },
            MergedChunk::Conflict {
                left_name: None,
                left: Some("left line\n"),
                base: Some("base line\n"),
                right: Some("right line\n"),
                right_name: None,
                base_name: None,
            },
        ]);

        let rendered_with_4 = merge.render(&DisplaySettings {
            conflict_marker_size: Some(4),
            ..Default::default()
        });
        let expected_with_4 = "resolved line\n<<<< LEFT\nleft line\n|||| BASE\nbase line\n====\nright line\n>>>> RIGHT\n";
        assert_eq!(rendered_with_4, expected_with_4);

        let rendered_with_9 = merge.render(&DisplaySettings {
            conflict_marker_size: Some(9),
            ..Default::default()
        });
        let expected_with_9 = "resolved line\n<<<<<<<<< LEFT\nleft line\n||||||||| BASE\nbase line\n=========\nright line\n>>>>>>>>> RIGHT\n";
        assert_eq!(rendered_with_9, expected_with_9);
    }

    #[test]
    fn render_no_final_newline() {
        // meanings of the used shortenings:
        // - wo              - without final newline
        // - w               - with final newline
        // - expected_w_wo_w - expected from base_w, left_wo, right_w

        let base_wo = "base";
        let base_w = "base\n";

        let left_wo = "left";
        let left_w = "left\n";

        let right_wo = "right";
        let right_w = "right\n";

        fn chunk(base: &str, left: &str, right: &str) -> String {
            ParsedMerge::new(vec![MergedChunk::Conflict {
                left: Some(left),
                base: Some(base),
                right: Some(right),
                left_name: None,
                base_name: None,
                right_name: None,
            }])
            .render(&Default::default())
        }

        let expected_wo_wo_wo =
            "<<<<<<< LEFT\nleft\n||||||| BASE\nbase\n=======\nright\n>>>>>>> RIGHT";
        let rendered = chunk(base_wo, left_wo, right_wo);
        assert_eq!(rendered, expected_wo_wo_wo);

        let expected_wo_w_wo =
            "<<<<<<< LEFT\nleft\n\n||||||| BASE\nbase\n=======\nright\n>>>>>>> RIGHT";
        let rendered = chunk(base_wo, left_w, right_wo);
        assert_eq!(rendered, expected_wo_w_wo);

        // wo_wo_w case should be symmetrical to wo_w_wo

        let expected_wo_w_w =
            "<<<<<<< LEFT\nleft\n\n||||||| BASE\nbase\n=======\nright\n\n>>>>>>> RIGHT";
        let rendered = chunk(base_wo, left_w, right_w);
        assert_eq!(rendered, expected_wo_w_w);

        let expected_w_wo_wo =
            "<<<<<<< LEFT\nleft\n||||||| BASE\nbase\n\n=======\nright\n>>>>>>> RIGHT";
        let rendered = chunk(base_w, left_wo, right_wo);
        assert_eq!(rendered, expected_w_wo_wo);

        let expected_w_w_wo =
            "<<<<<<< LEFT\nleft\n\n||||||| BASE\nbase\n\n=======\nright\n>>>>>>> RIGHT";
        let rendered_w_w_wo = chunk(base_w, left_w, right_wo);
        assert_eq!(rendered_w_w_wo, expected_w_w_wo);

        // w_wo_w should be symmetrical to w_w_wo
    }

    #[test]
    fn add_revision_names_to_settings() {
        let source = "<<<<<<< my_left\nlet's go to the left!\n||||||| my_base\nwhere should we go?\n=======\nturn right please!\n>>>>>>> my_right\nrest of file\n";
        let parsed =
            ParsedMerge::parse(source, &Default::default()).expect("unexpected parse error");

        let initial_settings = DisplaySettings::default();

        let mut enriched_settings = initial_settings.clone();
        enriched_settings.add_revision_names(&parsed);

        assert_eq!(
            enriched_settings,
            DisplaySettings {
                left_revision_name: Some(Cow::Borrowed("my_left")),
                base_revision_name: Some(Cow::Borrowed("my_base")),
                right_revision_name: Some(Cow::Borrowed("my_right")),
                ..initial_settings
            }
        );
    }

    #[test]
    fn add_revision_names_to_settings_no_names() {
        let source = "<<<<<<<\nlet's go to the left!\n|||||||\nwhere should we go?\n=======\nturn right please!\n>>>>>>>\nrest of file\n";
        let parsed =
            ParsedMerge::parse(source, &Default::default()).expect("unexpected parse error");

        let initial_settings = DisplaySettings::default();

        let mut enriched_settings = initial_settings.clone();
        enriched_settings.add_revision_names(&parsed);

        assert_eq!(enriched_settings, initial_settings);
    }

    #[test]
    fn add_revision_names_to_settings_no_conflict() {
        let source = "start of file\nrest of file\n";
        let parsed =
            ParsedMerge::parse(source, &Default::default()).expect("unexpected parse error");

        let initial_settings = DisplaySettings::default();

        let mut enriched_settings = initial_settings.clone();
        enriched_settings.add_revision_names(&parsed);

        assert_eq!(enriched_settings, initial_settings);
    }
}
