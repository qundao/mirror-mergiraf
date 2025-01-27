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
    Conflict {
        /// The left part of the conflict, including the last newline before the next marker
        left: &'a str,
        /// The base (or ancestor) part of the conflict, including the last newline before the next marker.
        base: &'a str,
        /// The right part of the conflict, including the last newline before the next marker.
        right: &'a str,
        /// The name of the left revision (potentially empty)
        left_name: &'a str,
        /// The name of the base revision (potentially empty)
        base_name: &'a str,
        /// The name of the right revision (potentially empty)
        right_name: &'a str,
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

        let left_marker = Regex::new(&format!(r"{left_marker}(?: (.*))?\r?\n")).unwrap();
        let base_marker = Regex::new(&format!(r"{base_marker}(?: (.*))?\r?\n")).unwrap();
        let middle_marker = Regex::new(&format!(r"{middle_marker}\r?\n")).unwrap();
        let right_marker = Regex::new(&format!(r"{right_marker}(?: (.*))?\r?\n",)).unwrap();

        let mut remaining_source = source;
        while !remaining_source.is_empty() {
            let left_captures = &left_marker.captures(remaining_source);
            let resolved_end = match left_captures {
                None => remaining_source.len(),
                Some(occurrence) => occurrence
                    .get(0)
                    .expect("whole match is guaranteed to exist")
                    .start(),
            };
            if resolved_end > 0 {
                // SAFETY: `remaining_source` is derived from `source`
                let offset = unsafe { remaining_source.as_ptr().offset_from(source.as_ptr()) }
                    .try_into()
                    .expect("`remaining_source` points to the _remainder_ of `source`");
                chunks.push(MergedChunk::Resolved {
                    offset,
                    contents: &remaining_source[..resolved_end],
                });
            }
            if let Some(left_captures) = left_captures {
                let left_match = left_captures.get(0).unwrap();
                let left_name = left_captures.get(1).map_or("", |m| m.as_str());
                remaining_source = &remaining_source[left_match.end()..];

                let base_captures = base_marker.captures(remaining_source).ok_or_else(|| {
                    if middle_marker.is_match(remaining_source) {
                        PARSED_MERGE_DIFF2_DETECTED
                    } else {
                        "unexpected end of file before base conflict marker"
                    }
                })?;
                let base_match = base_captures.get(0).unwrap();
                let base_name = base_captures.get(1).map_or("", |m| m.as_str());
                let left = &remaining_source[..base_match.start()];
                remaining_source = &remaining_source[base_match.end()..];

                let middle_match = middle_marker
                    .find(remaining_source)
                    .ok_or("unexpected end of file before middle conflict marker")?;
                let base = &remaining_source[..middle_match.start()];
                remaining_source = &remaining_source[middle_match.end()..];

                let right_captures = right_marker
                    .captures(remaining_source)
                    .ok_or("unexpected end of file before right conflict marker")?;
                let right_match = right_captures.get(0).unwrap();
                let right_name = right_captures.get(1).map_or("", |m| m.as_str());
                let right = &remaining_source[..right_match.start()];
                remaining_source = &remaining_source[right_match.end()..];

                chunks.push(MergedChunk::Conflict {
                    left,
                    base,
                    right,
                    left_name,
                    base_name,
                    right_name,
                });
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
                    left_offset += left.len();
                    base_offset += base.len();
                    right_offset += right.len();
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
                    Revision::Base => base,
                    Revision::Left => left,
                    Revision::Right => right,
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
                    result.push_str(&settings.left_marker_or_default());
                    result.push('\n');
                    result.push_str(left);
                    if settings.diff3 {
                        result.push_str(&settings.base_marker_or_default());
                        result.push('\n');
                        result.push_str(base);
                    }
                    result.push_str(&settings.middle_marker());
                    result.push('\n');
                    result.push_str(right);
                    result.push_str(&settings.right_marker_or_default());
                    result.push('\n');
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
                } => base.len() + left.len() + right.len(),
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
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
                left: "let's go to the left!\n",
                base: "where should we go?\n",
                right: "turn right please!\n",
                left_name: "left",
                base_name: "base",
                right_name: "",
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
                left: "let's go to the left!\n",
                base: "where should we go?\n",
                right: "turn right please!\n",
                left_name: "left",
                base_name: "base",
                right_name: "",
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
                left: "let's go to the left!\n",
                base: "where should we go?\n",
                right: "turn right please!\n",
                left_name: "left",
                base_name: "base",
                right_name: "",
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
                left: "    .foo = 3,\n    .bar = 2,\n",
                base: "    .foo = 3,\n",
                right: "",
                left_name: "LEFT",
                base_name: "BASE",
                right_name: "RIGHT",
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
                left: "left line\n",
                base: "base line\n",
                right: "right line\n",
                left_name: "LEFT",
                base_name: "BASE",
                right_name: "RIGHT",
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
                left_name: "",
                left: "left line\n",
                base: "base line\n",
                right: "right line\n",
                right_name: "",
                base_name: "",
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
                left_revision_name: Some("my_left"),
                base_revision_name: Some("my_base"),
                right_revision_name: Some("my_right"),
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
