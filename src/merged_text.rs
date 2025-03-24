use std::borrow::Cow;

use itertools::Itertools;
use regex::Regex;

use crate::{parsed_merge::ParsedMerge, settings::DisplaySettings};

/// A merged file represented as a sequence of sections,
/// some being successfully merged and others being conflicts.
///
/// This is different from [`ParsedMerge`] because the conflicts
/// don't necessarily need to match line boundaries, and the precise
/// layout of the resulting text is not known yet as it depends on
/// the output settings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MergedText<'a> {
    sections: Vec<MergeSection<'a>>,
}

/// A part of a merged file to be output
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MergeSection<'a> {
    /// Content that is successfully merged
    Merged(Cow<'a, str>),
    /// A conflict, with contents differing from the revisions
    Conflict {
        base: Cow<'a, str>,
        left: Cow<'a, str>,
        right: Cow<'a, str>,
    },
}

impl<'a> MergedText<'a> {
    /// Creates an empty merged text
    pub(crate) fn new() -> Self {
        MergedText {
            sections: Vec::new(),
        }
    }

    /// Appends merged text at the end
    pub(crate) fn push_merged(&mut self, contents: Cow<'a, str>) {
        self.sections.push(MergeSection::Merged(contents));
    }

    /// Appends a conflict at the end
    pub(crate) fn push_conflict(
        &mut self,
        base: Cow<'a, str>,
        left: Cow<'a, str>,
        right: Cow<'a, str>,
    ) {
        if left == right {
            // well that's not really a conflict
            self.push_merged(left);
        } else {
            self.sections
                .push(MergeSection::Conflict { base, left, right });
        }
    }

    /// Appends some text which might contain line-based conflicts.
    /// If the text contains newlines it also gets re-indented to the indentation level supplied.
    pub(crate) fn push_line_based_merge(
        &mut self,
        line_based_merge: &str,
        indentation: &str,
        settings: &DisplaySettings,
    ) {
        let parsed = ParsedMerge::parse(line_based_merge, settings)
            .expect("Parsing the line-based merge failed");
        let mut newline_found = false;
        let sections = parsed.chunks.into_iter().map(|section| match section {
            crate::parsed_merge::MergedChunk::Resolved { contents, .. } => {
                let result = MergeSection::Merged(
                    Self::reindent_line_based_merge(contents, indentation, newline_found, true)
                        .into(),
                );
                newline_found = newline_found || contents.contains('\n');
                result
            }
            crate::parsed_merge::MergedChunk::Conflict {
                left, base, right, ..
            } => {
                let result = MergeSection::Conflict {
                    left: Self::reindent_line_based_merge(
                        left.unwrap_or_default(),
                        indentation,
                        false,
                        false,
                    )
                    .into(),
                    base: Self::reindent_line_based_merge(
                        base.unwrap_or_default(),
                        indentation,
                        false,
                        false,
                    )
                    .into(),
                    right: Self::reindent_line_based_merge(
                        right.unwrap_or_default(),
                        indentation,
                        false,
                        false,
                    )
                    .into(),
                };
                newline_found = newline_found
                    || left.unwrap_or_default().contains('\n')
                    || right.unwrap_or_default().contains('\n')
                    || base.unwrap_or_default().contains('\n');
                result
            }
        });
        self.sections.extend(sections);
    }

    /// Reindents the contents of a line-based merge
    fn reindent_line_based_merge(
        content: &str,
        indentation: &str,
        reindent_first: bool,
        reindent_last: bool,
    ) -> String {
        let reindented = content
            .split('\n')
            .enumerate()
            .map(|(idx, line)| {
                if line.is_empty() || (idx == 0 && !reindent_first) {
                    Cow::from(line)
                } else {
                    Cow::from(format!("{indentation}{line}"))
                }
            })
            .join("\n");
        if reindent_last && reindented.ends_with('\n') {
            reindented + indentation
        } else {
            reindented
        }
    }

    /// Renders the full file according to the supplied [`DisplaySettings`]
    pub(crate) fn render(&self, settings: &DisplaySettings) -> String {
        // if all the chunks are `Merged`, just concatenate them all
        if let Some(contents) = self
            .sections
            .iter()
            .map(|section| {
                if let MergeSection::Merged(contents) = section {
                    Some(contents.as_ref())
                } else {
                    None
                }
            })
            .collect()
        {
            return contents;
        }

        if settings.compact_or_default() {
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
        for section in &self.sections {
            match section {
                MergeSection::Merged(contents) => {
                    if gathering_conflict {
                        if let Some(newline_idx) = contents.find('\n') {
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
                            gathering_conflict = false;
                        } else if contents.trim().is_empty() {
                            // the content being added is just whitespace (but no newlines,
                            // checked above), so something that separates an element from the next.
                            // therefore, we only want to add it a side, if the latter actually
                            // has an element in it (and not just indentation/nothing at all)
                            if !base_buffer.trim().is_empty() {
                                base_buffer.push_str(contents);
                            }
                            if !left_buffer.trim().is_empty() {
                                left_buffer.push_str(contents);
                            }
                            if !right_buffer.trim().is_empty() {
                                right_buffer.push_str(contents);
                            }
                        } else {
                            base_buffer.push_str(contents);
                            left_buffer.push_str(contents);
                            right_buffer.push_str(contents);
                        }
                    } else {
                        output.push_str(contents);
                    }
                }
                MergeSection::Conflict { base, left, right } => {
                    if !gathering_conflict {
                        if !output.ends_with('\n') && !output.is_empty() {
                            // we have an unfinished line in the output: let's remove it
                            // and add it to the conflict we are starting to gather
                            let last_newline_index = output.rfind('\n');
                            let last_line = output.split_off(match last_newline_index {
                                Some(idx) => idx + 1,
                                None => 0,
                            });
                            base_buffer.clone_from(&last_line);
                            left_buffer.clone_from(&last_line);
                            right_buffer = last_line;
                        } else {
                            base_buffer.clear();
                            left_buffer.clear();
                            right_buffer.clear();
                        }
                    }
                    base_buffer.push_str(base);
                    left_buffer.push_str(left);
                    right_buffer.push_str(right);
                    let all_end_with_newline = (base_buffer.ends_with('\n')
                        || base_buffer.trim().is_empty())
                        && (left_buffer.ends_with('\n') || left_buffer.trim().is_empty())
                        && (right_buffer.ends_with('\n') || right_buffer.trim().is_empty());
                    if all_end_with_newline {
                        Self::render_conflict(
                            &base_buffer,
                            &left_buffer,
                            &right_buffer,
                            settings,
                            &mut output,
                        );
                    }
                    gathering_conflict = !all_end_with_newline;
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
        output.push_str(&settings.left_marker_or_default());
        output.push('\n');
        if !left.trim().is_empty() {
            output.push_str(left);
        }
        if settings.diff3 {
            Self::maybe_add_newline(output);
            output.push_str(&settings.base_marker_or_default());
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
        output.push_str(&settings.right_marker_or_default());
        output.push('\n');
    }

    /// Renders the merged text without expanding conflict boundaries so that they match newlines.
    /// Instead, insert newlines around the conflict boundaries directly.
    fn render_compact(&self, settings: &DisplaySettings) -> String {
        let mut output = String::new();
        let mut last_was_conflict = false;
        let leading_whitespace_pattern = Regex::new("^[\t ]*\n").expect("Invalid regex");
        let trailing_whitespace_pattern = Regex::new("[\t ]+$").expect("Invalid regex");
        for section in &self.sections {
            match section {
                MergeSection::Merged(contents) => {
                    if last_was_conflict {
                        output.push_str(&leading_whitespace_pattern.replace(contents, ""));
                    } else {
                        output.push_str(contents);
                    }
                    last_was_conflict = false;
                }
                MergeSection::Conflict { base, left, right } => {
                    if let Some(occurrence) = trailing_whitespace_pattern.find(&output) {
                        let whitespace_to_prepend = output.split_off(occurrence.start());
                        let new_base = if base.is_empty() {
                            base
                        } else {
                            &(whitespace_to_prepend.clone() + base).into()
                        };
                        let new_left = if left.is_empty() {
                            left
                        } else {
                            &(whitespace_to_prepend.clone() + left).into()
                        };
                        let new_right = if right.is_empty() {
                            right
                        } else {
                            &(whitespace_to_prepend + right).into()
                        };
                        Self::render_conflict(new_base, new_left, new_right, settings, &mut output);
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
            output.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged(contents: &str) -> MergeSection {
        MergeSection::Merged(contents.into())
    }

    fn conflict<'a>(base: &'a str, left: &'a str, right: &'a str) -> MergeSection<'a> {
        MergeSection::Conflict {
            base: base.into(),
            left: left.into(),
            right: right.into(),
        }
    }

    #[test]
    fn compact_mode() {
        let merged_text = MergedText {
            sections: vec![
                merged("hello"),
                merged(" world\nhi "),
                conflict("ho base", "ho left", "ho right"),
                merged("  test\n"),
            ],
        };

        let expected_compact = "\
hello world
hi
<<<<<<< LEFT
 ho left
||||||| BASE
 ho base
=======
 ho right
>>>>>>> RIGHT
  test
";
        assert_eq!(
            merged_text.render(&DisplaySettings::default_compact()),
            expected_compact
        );

        let expected_full_line = "\
hello world
<<<<<<< LEFT
hi ho left  test
||||||| BASE
hi ho base  test
=======
hi ho right  test
>>>>>>> RIGHT
";
        assert_eq!(
            merged_text.render(&DisplaySettings::default()),
            expected_full_line
        );
    }

    #[test]
    fn multiple_conflicts_on_same_line() {
        let merged_text = MergedText {
            sections: vec![
                merged("let's start "),
                conflict("ho", "hi", "ha"),
                merged(" to "),
                conflict("you", "everyone", "me"),
                merged("!"),
            ],
        };
        let expected_full_line = "\
<<<<<<< LEFT
let's start hi to everyone!
||||||| BASE
let's start ho to you!
=======
let's start ha to me!
>>>>>>> RIGHT
";
        assert_eq!(
            merged_text.render(&DisplaySettings::default()),
            expected_full_line
        );
    }

    #[test]
    fn spurious_conflict() {
        let mut merged_text = MergedText::new();
        merged_text.push_merged("let's start ".into());
        merged_text.push_conflict("tomorrow".into(), "now".into(), "now".into());
        merged_text.push_merged(", as it seems we all agree".into());
        let expected_full_line = "let's start now, as it seems we all agree";

        assert_eq!(
            merged_text.render(&DisplaySettings::default()),
            expected_full_line
        );
    }

    #[test]
    fn space_after_conflict_base_empty() {
        // the space shouldn't be pulled into the conflict
        let merged_text = MergedText {
            sections: vec![conflict("", "here", "there"), merged(" "), merged("we go")],
        };

        let expected = "\
<<<<<<< LEFT
here we go
||||||| BASE
we go
=======
there we go
>>>>>>> RIGHT
";

        assert_eq!(merged_text.render(&DisplaySettings::default()), expected);
    }

    #[test]
    fn space_after_conflict_base_empty_next_incorrect() {
        // this is not something we expect to get -- normally, the whitespace should come
        // separately from the actual elements. therefore, the "incorrect" output is okay
        let merged_text = MergedText {
            sections: vec![conflict("", "here", "there"), merged(" we go")],
        };

        let expected_wrong = "\
<<<<<<< LEFT
here we go
||||||| BASE
 we go
=======
there we go
>>>>>>> RIGHT
";

        assert_eq!(
            merged_text.render(&DisplaySettings::default()),
            expected_wrong
        );
    }

    #[test]
    fn space_after_conflict_base_empty_all_indented() {
        // the sides may be indented
        let merged_text = MergedText {
            sections: vec![
                merged("    "),
                conflict("", "here", "there"),
                merged(" "),
                merged("we go"),
            ],
        };

        let expected = "\
<<<<<<< LEFT
    here we go
||||||| BASE
    we go
=======
    there we go
>>>>>>> RIGHT
";

        assert_eq!(merged_text.render(&DisplaySettings::default()), expected);
    }

    #[test]
    fn space_after_conflict_left_empty() {
        // left or right revision may be empty as well
        let merged_text = MergedText {
            sections: vec![conflict("here", "", "there"), merged(" "), merged("we go")],
        };

        let expected = "\
<<<<<<< LEFT
we go
||||||| BASE
here we go
=======
there we go
>>>>>>> RIGHT
";

        assert_eq!(merged_text.render(&DisplaySettings::default()), expected);
    }
    #[test]
    fn space_after_conflict_left_empty_all_indented() {
        // left or right revision may be empty as well
        let merged_text = MergedText {
            sections: vec![
                merged("    "),
                conflict("here", "", "there"),
                merged(" "),
                merged("we go"),
            ],
        };

        let expected = "\
<<<<<<< LEFT
    we go
||||||| BASE
    here we go
=======
    there we go
>>>>>>> RIGHT
";

        assert_eq!(merged_text.render(&DisplaySettings::default()), expected);
    }
}
