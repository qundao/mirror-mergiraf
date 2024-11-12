use itertools::Itertools;
use regex::Regex;

use crate::{parsed_merge::ParsedMerge, settings::DisplaySettings};

/// A merged file represented as a sequence of sections,
/// some being successfully merged and others being conflicts.
///
/// This is different from [ParsedMerge] because the conflicts
/// don't necessarily need to match line boundaries, and the precise
/// layout of the resulting text is not known yet as it depends on
/// the output settings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MergedText {
    sections: Vec<MergeSection>,
}

/// A part of a merged file to be output
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MergeSection {
    /// Content that is successfully merged
    Merged(String),
    /// A conflict, with contents differing from the revisions
    Conflict {
        base: String,
        left: String,
        right: String,
    },
}

impl MergedText {
    /// Creates an empty merged text
    pub(crate) fn new() -> Self {
        MergedText {
            sections: Vec::new(),
        }
    }

    /// Appends merged text at the end
    pub(crate) fn push_merged(&mut self, contents: String) {
        self.sections.push(MergeSection::Merged(contents))
    }

    /// Appends a conflict at the end
    pub(crate) fn push_conflict(&mut self, base: String, left: String, right: String) {
        if left == right {
            // well that's not really a conflict
            self.push_merged(left)
        } else {
            self.sections
                .push(MergeSection::Conflict { base, left, right })
        }
    }

    /// Appends some text which might contain line-based conflicts.
    /// If the text contains newlines it also gets re-indented to the indentation level supplied.
    pub(crate) fn push_line_based_merge(&mut self, line_based_merge: &str, indentation: &str) {
        let parsed =
            ParsedMerge::parse(line_based_merge).expect("Parsing the line-based merge failed");
        let mut newline_found = false;
        for section in parsed.chunks.into_iter() {
            self.sections.push(match section {
                crate::parsed_merge::MergedChunk::Resolved { contents, .. } => {
                    let result = MergeSection::Merged(Self::reindent_line_based_merge(
                        &contents,
                        indentation,
                        newline_found,
                        true,
                    ));
                    newline_found = newline_found || contents.contains("\n");
                    result
                }
                crate::parsed_merge::MergedChunk::Conflict { left, base, right } => {
                    let result = MergeSection::Conflict {
                        left: Self::reindent_line_based_merge(&left, indentation, false, false),
                        base: Self::reindent_line_based_merge(&base, indentation, false, false),
                        right: Self::reindent_line_based_merge(&right, indentation, false, false),
                    };
                    newline_found = newline_found
                        || left.contains("\n")
                        || right.contains("\n")
                        || base.contains("\n");
                    result
                }
            })
        }
    }

    /// Reindents the contents of a line-based merge
    fn reindent_line_based_merge(
        content: &str,
        indentation: &str,
        reindent_first: bool,
        reindent_last: bool,
    ) -> String {
        let reindented = content
            .split("\n")
            .enumerate()
            .map(|(idx, line)| {
                if line.is_empty() || (idx == 0 && !reindent_first) {
                    line.to_owned()
                } else {
                    format!("{}{}", indentation, line)
                }
            })
            .join("\n");
        if reindent_last && reindented.ends_with('\n') {
            reindented + indentation
        } else {
            reindented
        }
    }

    /// Renders the full file according to the supplied [DisplaySettings]
    pub(crate) fn render(&self, settings: &DisplaySettings) -> String {
        if settings.compact {
            self.render_compact(settings)
        } else {
            self.render_full_lines(settings)
        }
    }

    /// Renders the merged text by expanding conflict boundaries so that they match newlines
    fn render_full_lines(&self, settings: &DisplaySettings) -> String {
        let mut output = String::new();
        let mut base_buffer = String::new();
        let mut left_buffer = String::new();
        let mut right_buffer = String::new();
        let mut gathering_conflict = false;
        for section in self.sections.iter() {
            match section {
                MergeSection::Merged(contents) => {
                    if gathering_conflict {
                        match contents.find("\n") {
                            Some(newline_idx) => {
                                let to_append = &contents[..(newline_idx + 1)];
                                left_buffer.push_str(to_append);
                                base_buffer.push_str(to_append);
                                right_buffer.push_str(to_append);
                                Self::render_conflict(
                                    &base_buffer,
                                    &left_buffer,
                                    &right_buffer,
                                    settings,
                                    &mut output,
                                );
                                output.push_str(&contents[(newline_idx + 1)..]);
                                gathering_conflict = false
                            }
                            None => {
                                left_buffer.push_str(contents);
                                base_buffer.push_str(contents);
                                right_buffer.push_str(contents);
                            }
                        }
                    } else {
                        output.push_str(contents)
                    }
                }
                MergeSection::Conflict { base, left, right } => {
                    if !gathering_conflict {
                        if !output.ends_with("\n") && !output.is_empty() {
                            // we have an unfinished line in the output: let's remove it
                            // and add it to the conflict we are starting to gather
                            let last_newline_index = output.rfind("\n");
                            let last_line = output.split_off(match last_newline_index {
                                Some(idx) => idx + 1,
                                None => 0,
                            });
                            base_buffer = last_line.clone();
                            left_buffer = last_line.clone();
                            right_buffer = last_line;
                        } else {
                            base_buffer = String::new();
                            left_buffer = String::new();
                            right_buffer = String::new();
                        }
                    }
                    base_buffer.push_str(base);
                    left_buffer.push_str(left);
                    right_buffer.push_str(right);
                    let all_end_with_newline = (base_buffer.ends_with("\n")
                        || base_buffer.trim().is_empty())
                        && (left_buffer.ends_with("\n") || left_buffer.trim().is_empty())
                        && (right_buffer.ends_with("\n") || right_buffer.trim().is_empty());
                    if all_end_with_newline {
                        Self::render_conflict(
                            &base_buffer,
                            &left_buffer,
                            &right_buffer,
                            settings,
                            &mut output,
                        );
                    }
                    gathering_conflict = !all_end_with_newline
                }
            }
        }
        if gathering_conflict {
            Self::render_conflict(
                &base_buffer,
                &left_buffer,
                &right_buffer,
                settings,
                &mut output,
            );
        }
        output
    }

    fn render_conflict(
        base: &str,
        left: &str,
        right: &str,
        settings: &DisplaySettings,
        output: &mut String,
    ) {
        Self::maybe_add_newline(output);
        output.push_str(&settings.left_marker());
        output.push('\n');
        if !left.trim().is_empty() {
            output.push_str(left);
        }
        if settings.diff3 {
            Self::maybe_add_newline(output);
            output.push_str(&settings.base_marker());
            output.push('\n');
            if !base.trim().is_empty() {
                output.push_str(base);
            }
        }
        Self::maybe_add_newline(output);
        output.push_str(&settings.middle_marker());
        output.push('\n');
        if !right.trim().is_empty() {
            output.push_str(right);
        }
        Self::maybe_add_newline(output);
        output.push_str(&settings.right_marker());
        output.push('\n');
    }

    /// Renders the merged text without expanding conflict boundaries so that they match newlines.
    /// Instead, insert newlines around the conflict boundaries directly.
    fn render_compact(&self, settings: &DisplaySettings) -> String {
        let mut output = String::new();
        let mut last_was_conflict = false;
        let leading_whitespace_pattern = Regex::new("^[\t ]*\n").expect("Invalid regex");
        let trailing_whitespace_pattern = Regex::new("[\t ]+$").expect("Invalid regex");
        for section in self.sections.iter() {
            match section {
                MergeSection::Merged(contents) => {
                    if last_was_conflict {
                        output.push_str(&leading_whitespace_pattern.replace(contents, ""))
                    } else {
                        output.push_str(contents);
                    }
                    last_was_conflict = false;
                }
                MergeSection::Conflict { base, left, right } => {
                    if let Some(occurrence) = trailing_whitespace_pattern.find(&output) {
                        let whitespace_to_prepend = output.split_off(occurrence.start());
                        let new_base = if base.is_empty() {
                            base.clone()
                        } else {
                            whitespace_to_prepend.clone() + base
                        };
                        let new_left = if left.is_empty() {
                            left.clone()
                        } else {
                            whitespace_to_prepend.clone() + left
                        };
                        let new_right = if right.is_empty() {
                            right.clone()
                        } else {
                            whitespace_to_prepend + right
                        };
                        Self::render_conflict(
                            &new_base,
                            &new_left,
                            &new_right,
                            settings,
                            &mut output,
                        );
                    } else {
                        Self::render_conflict(base, left, right, settings, &mut output);
                    }
                    last_was_conflict = true;
                }
            }
        }
        output
    }

    fn maybe_add_newline(output: &mut String) {
        if !output.ends_with('\n') && !output.is_empty() {
            output.push('\n')
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged(contents: &str) -> MergeSection {
        MergeSection::Merged(contents.to_owned())
    }

    fn conflict(base: &str, left: &str, right: &str) -> MergeSection {
        MergeSection::Conflict {
            base: base.to_owned(),
            left: left.to_owned(),
            right: right.to_owned(),
        }
    }

    #[test]
    fn test_compact_mode() {
        let merged_text = MergedText {
            sections: vec![
                merged("hello"),
                merged(" world\nhi "),
                conflict("ho base", "ho left", "ho right"),
                merged("  test\n"),
            ],
        };

        let expected_compact = "hello world\nhi\n<<<<<<< LEFT\n ho left\n||||||| BASE\n ho base\n=======\n ho right\n>>>>>>> RIGHT\n  test\n";
        assert_eq!(
            merged_text.render(&DisplaySettings::default_compact()),
            expected_compact
        );

        let expected_full_line = "hello world\n<<<<<<< LEFT\nhi ho left  test\n||||||| BASE\nhi ho base  test\n=======\nhi ho right  test\n>>>>>>> RIGHT\n";
        assert_eq!(
            merged_text.render(&DisplaySettings::default()),
            expected_full_line
        );
    }

    #[test]
    fn test_multiple_conflicts_on_same_line() {
        let merged_text = MergedText {
            sections: vec![
                merged("let's start "),
                conflict("ho", "hi", "ha"),
                merged(" to "),
                conflict("you", "everyone", "me"),
                merged("!"),
            ],
        };
        let expected_full_line = "<<<<<<< LEFT\nlet's start hi to everyone!\n||||||| BASE\nlet's start ho to you!\n=======\nlet's start ha to me!\n>>>>>>> RIGHT\n";
        assert_eq!(
            merged_text.render(&DisplaySettings::default()),
            expected_full_line
        );
    }

    #[test]
    fn test_spurious_conflict() {
        let mut merged_text = MergedText::new();
        merged_text.push_merged("let's start ".to_owned());
        merged_text.push_conflict("tomorrow".to_owned(), "now".to_owned(), "now".to_owned());
        merged_text.push_merged(", as it seems we all agree".to_owned());
        let expected_full_line = "let's start now, as it seems we all agree";

        assert_eq!(
            merged_text.render(&DisplaySettings::default()),
            expected_full_line
        );
    }
}
