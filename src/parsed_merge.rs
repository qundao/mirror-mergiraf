use std::{collections::HashMap, ops::Range};

use itertools::Itertools;
use regex::Regex;

use crate::{
    line_based::MergeResult,
    matching::Matching,
    pcs::Revision,
    settings::DisplaySettings,
    tree::{Ast, AstNode},
};

pub(crate) const PARSED_MERGE_DIFF2_DETECTED: &str =
    "Mergiraf cannot solve conflicts displayed in the diff2 style";

/// A file which potentially contains merge conflicts, parsed as such.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParsedMerge {
    /// The actual contents of the parsed merge
    pub chunks: Vec<MergedChunk>,
    /// List of correspondences between sections of the reconstructed left revision and the merge output
    left: Vec<OffsetMap>,
    /// List of correspondences between sections of the reconstructed right revision and the merge output
    right: Vec<OffsetMap>,
    /// List of correspondences between sections of the reconstructed base revision and the merge output
    base: Vec<OffsetMap>,
}

/// A chunk in a file with merge conflicts: either a readily merged chunk or a conflict.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MergedChunk {
    /// A readily-merged chunk
    Resolved {
        /// The byte offset at which this merged chunk can be found
        offset: usize,
        /// Its textual contents (including the last newline before any conflict)
        contents: String,
    },
    /// A diff3-style conflict
    Conflict {
        /// The left part of the conflict, including the last newline before the next marker
        left: String,
        /// The base (or ancestor) part of the conflict, including the last newline before the next marker.
        base: String,
        /// The right part of the conflict, including the last newline before the next marker.
        right: String,
    },
}

/// A correspondence between a secstion of a reconstructed revision and the merge output
#[derive(Debug, Clone, Eq, PartialEq)]
struct OffsetMap {
    /// The start of the section in the reconstructed revision
    rev_start: usize,
    /// The start of the section in the original merge output
    merged_start: usize,
    /// The lcommon ength of the section on both sides
    length: usize,
}

impl ParsedMerge {
    /// Parse a file into a series of chunks.
    /// Fails if the conflict markers do not appear in a consistent order.
    pub(crate) fn parse(source: &str) -> Result<ParsedMerge, String> {
        let mut chunks = Vec::new();
        let start_marker = Regex::new(r"(^|\n)<<<<<<<( .*)?\n").unwrap();
        let base_marker = Regex::new(r"\|\|\|\|\|\|\|( [^\n]*)?\r?\n").unwrap();
        let right_marker = Regex::new(r"=======\r?\n").unwrap();
        let end_marker = Regex::new(r">>>>>>>( [^\n]*)?\r?\n").unwrap();

        let mut offset = 0;
        while offset < source.len() {
            let remaining_source = &source[offset..];
            let start_marker = start_marker.find(remaining_source);
            let resolved_end = match start_marker {
                None => remaining_source.len(),
                Some(occurrence) => {
                    if occurrence.as_str().starts_with('\n') {
                        occurrence.start() + 1
                    } else {
                        occurrence.start()
                    }
                }
            };
            if resolved_end > 0 {
                chunks.push(MergedChunk::Resolved {
                    offset,
                    contents: remaining_source[..resolved_end].to_owned(),
                });
            }
            offset += resolved_end;
            if let Some(start_marker) = start_marker {
                let local_offset = start_marker.end();
                let base_match = match base_marker.find(&remaining_source[local_offset..]) {
                    Some(occurrence) => Ok(occurrence),
                    None => {
                        if right_marker
                            .find(&remaining_source[local_offset..])
                            .is_some()
                        {
                            Err(PARSED_MERGE_DIFF2_DETECTED)
                        } else {
                            Err("unexpected end of file before base conflict marker")
                        }
                    }
                }?;
                let left =
                    remaining_source[local_offset..(local_offset + base_match.start())].to_owned();
                let local_offset = local_offset + base_match.end();
                let right_match = right_marker
                    .find(&remaining_source[local_offset..])
                    .ok_or("unexpected end of file before right conflict marker")?;
                let base =
                    remaining_source[local_offset..(local_offset + right_match.start())].to_owned();
                let local_offset = local_offset + right_match.end();
                let end_match = end_marker
                    .find(&remaining_source[local_offset..])
                    .ok_or("unexpected end of file before end conflict marker")?;
                let right =
                    remaining_source[local_offset..(local_offset + end_match.start())].to_owned();
                chunks.push(MergedChunk::Conflict { left, base, right });
                offset += local_offset + end_match.end() - resolved_end;
            }
        }
        Ok(ParsedMerge::new(chunks))
    }

    /// Construct a parsed merge by indexing the provided chunks
    fn new(chunks: Vec<MergedChunk>) -> Self {
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
                MergedChunk::Conflict { left, base, right } => {
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
            .map(|chunk| match chunk {
                MergedChunk::Resolved { contents, .. } => contents,
                MergedChunk::Conflict { left, base, right } => match revision {
                    Revision::Base => base,
                    Revision::Left => left,
                    Revision::Right => right,
                },
            })
            .join("")
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
        Some(Range {
            start: matched_start,
            end: matched_start + length,
        })
    }

    /// Generate a matching between the ASTs of two revisions generated by this parsed merge,
    /// by matching elements whenever they correspond to the same merged range.
    pub(crate) fn generate_matching<'a>(
        &self,
        first_revision: Revision,
        second_revision: Revision,
        first_tree: &'a Ast<'a>,
        second_tree: &'a Ast<'a>,
    ) -> Matching<'a> {
        let first_index = self.index_tree_by_merged_ranges(first_revision, first_tree);
        let second_index = self.index_tree_by_merged_ranges(second_revision, second_tree);
        let mut matching = Matching::new();
        for (range, first_node) in first_index.iter() {
            if let Some(second_node) = second_index.get(range) {
                matching.add(first_node, second_node);
            }
        }
        matching
    }

    fn index_tree_by_merged_ranges<'a>(
        &self,
        revision: Revision,
        tree: &'a Ast<'a>,
    ) -> HashMap<Range<usize>, &'a AstNode<'a>> {
        let mut map = HashMap::new();
        self.recursively_index_node(revision, tree.root(), &mut map);
        map
    }

    fn recursively_index_node<'a>(
        &self,
        revision: Revision,
        node: &'a AstNode<'a>,
        map: &mut HashMap<Range<usize>, &'a AstNode<'a>>,
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
        for chunk in self.chunks.iter() {
            match chunk {
                MergedChunk::Resolved { contents, .. } => result.push_str(contents),
                MergedChunk::Conflict { left, base, right } => {
                    result.push_str(&settings.left_marker());
                    result.push('\n');
                    result.push_str(left);
                    if settings.diff3 {
                        result.push_str(&settings.base_marker());
                        result.push('\n');
                        result.push_str(base);
                    }
                    result.push_str(&settings.middle_marker());
                    result.push('\n');
                    result.push_str(right);
                    result.push_str(&settings.right_marker());
                    result.push('\n');
                }
            }
        }
        result
    }

    /// Converts to a merge result
    pub(crate) fn to_merge_result(&self, settings: &DisplaySettings) -> MergeResult {
        MergeResult {
            contents: self.render(settings),
            conflict_count: self.conflict_count(),
            conflict_mass: self.conflict_mass(),
            method: "original",
            has_additional_issues: false,
        }
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
            .map(|chunk| match chunk {
                MergedChunk::Resolved { .. } => 0,
                MergedChunk::Conflict { .. } => 1,
            })
            .sum()
    }

    // Number of bytes of conflicting content, which is an attempt
    // at quantifying the effort it takes to resolve the conflicts.
    pub fn conflict_mass(&self) -> usize {
        self.chunks
            .iter()
            .map(|chunk| match chunk {
                MergedChunk::Resolved { .. } => 0,
                MergedChunk::Conflict { base, left, right } => {
                    base.len() + left.len() + right.len()
                }
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::ctx;

    use super::*;

    #[test]
    fn test_parse() {
        let source = "\nwe reached a junction.\n<<<<<<< left\nlet's go to the left!\n||||||| base\nwhere should we go?\n=======\nturn right please!\n>>>>>>>\nrest of file\n";
        let parsed = ParsedMerge::parse(source).expect("unexpected parse error");

        let expected_parse = ParsedMerge::new(vec![
            MergedChunk::Resolved {
                offset: 0,
                contents: "\nwe reached a junction.\n".to_owned(),
            },
            MergedChunk::Conflict {
                left: "let's go to the left!\n".to_owned(),
                base: "where should we go?\n".to_owned(),
                right: "turn right please!\n".to_owned(),
            },
            MergedChunk::Resolved {
                offset: 127,
                contents: "rest of file\n".to_owned(),
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
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 45, end: 49 }, Revision::Base),
            Some(Range {
                start: 128,
                end: 132
            })
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 47, end: 49 }, Revision::Left),
            Some(Range {
                start: 128,
                end: 130
            })
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(&Range { start: 45, end: 48 }, Revision::Right),
            Some(Range {
                start: 129,
                end: 132
            })
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(
                &Range {
                    start: 180,
                    end: 183
                },
                Revision::Base
            ),
            None
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(
                &Range {
                    start: 190,
                    end: 193
                },
                Revision::Left
            ),
            None
        );
        assert_eq!(
            parsed.rev_range_to_merged_range(
                &Range {
                    start: 200,
                    end: 203
                },
                Revision::Right
            ),
            None
        );
    }

    #[test]
    fn test_parse_start_with_conflict() {
        let source = "<<<<<<< left\nlet's go to the left!\n||||||| base\nwhere should we go?\n=======\nturn right please!\n>>>>>>>\nrest of file\n";
        let parsed = ParsedMerge::parse(source).expect("unexpected parse error");

        let expected_parse = ParsedMerge::new(vec![
            MergedChunk::Conflict {
                left: "let's go to the left!\n".to_owned(),
                base: "where should we go?\n".to_owned(),
                right: "turn right please!\n".to_owned(),
            },
            MergedChunk::Resolved {
                offset: 103,
                contents: "rest of file\n".to_owned(),
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
    fn test_parse_end_with_conflict() {
        let source = "\nwe reached a junction.\n<<<<<<< left\nlet's go to the left!\n||||||| base\nwhere should we go?\n=======\nturn right please!\n>>>>>>>\n";
        let parsed = ParsedMerge::parse(source).expect("unexpected parse error");

        let expected_parse = ParsedMerge::new(vec![
            MergedChunk::Resolved {
                offset: 0,
                contents: "\nwe reached a junction.\n".to_owned(),
            },
            MergedChunk::Conflict {
                left: "let's go to the left!\n".to_owned(),
                base: "where should we go?\n".to_owned(),
                right: "turn right please!\n".to_owned(),
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
    fn test_parse_diffy() {
        let source = r#"my_struct_t instance = {
<<<<<<< LEFT
    .foo = 3,
    .bar = 2,
||||||| BASE
    .foo = 3,
=======
>>>>>>> RIGHT
};
"#;

        let parsed = ParsedMerge::parse(source).expect("could not parse!");

        let expected_parse = ParsedMerge::new(vec![
            MergedChunk::Resolved {
                offset: 0,
                contents: "my_struct_t instance = {\n".to_owned(),
            },
            MergedChunk::Conflict {
                left: "    .foo = 3,\n    .bar = 2,\n".to_owned(),
                base: "    .foo = 3,\n".to_owned(),
                right: "".to_owned(),
            },
            MergedChunk::Resolved {
                offset: 115,
                contents: "};\n".to_owned(),
            },
        ]);

        assert_eq!(parsed, expected_parse);
        assert_eq!(parsed.conflict_count(), 1);
        assert_eq!(parsed.conflict_mass(), 42);

        // render the parsed conflict and check it's equal to the source
        let rendered = parsed.render(&DisplaySettings {
            diff3: true,
            compact: false,
            conflict_marker_size: 7,
            left_revision_name: "LEFT".to_owned(),
            base_revision_name: "BASE".to_owned(),
            right_revision_name: "RIGHT".to_owned(),
        });

        assert_eq!(rendered, source);
    }

    #[test]
    fn test_parse_diff2() {
        let source = r#"my_struct_t instance = {
<<<<<<< LEFT
    .foo = 3,
    .bar = 2,
=======
>>>>>>> RIGHT
};
"#;

        match ParsedMerge::parse(source) {
            Ok(_) => panic!("expected a parse failure for diff2 conflicts"),
            Err(e) => assert_eq!(e, PARSED_MERGE_DIFF2_DETECTED),
        };
    }

    #[test]
    fn test_matching() {
        let ctx = ctx();
        let source = r#"struct MyType {
    field: bool,
<<<<<<< LEFT
    foo: int,
    bar: String,
||||||| BASE
    foo: String,
=======
>>>>>>> RIGHT
};
"#;
        let parsed = ParsedMerge::parse(source).expect("could not parse!");

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
}
