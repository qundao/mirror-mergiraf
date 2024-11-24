/// Parameters controlling how the merged tree should be output.
pub struct DisplaySettings {
    /// Whether to show the base revision in the conflicts (true by default)
    pub diff3: bool,
    /// Whether to show compact conflicts or to expand them to fill an entire line
    pub compact: bool,
    /// The number of characters for conflict markers (7 by default)
    pub conflict_marker_size: usize,
    /// The string that identifies the left revision in conflict markers
    pub left_revision_name: String,
    /// The string that identifies the base revision in conflict markers
    pub base_revision_name: String,
    /// The string that identifies the right revision in conflict markers
    pub right_revision_name: String,
}

impl DisplaySettings {
    /// The marker at the beginning of the "left" (first) part of a conflict.
    /// It does not contain any newline character.
    pub fn left_marker(&self) -> String {
        format!(
            "{} {}",
            "<".to_string().repeat(self.conflict_marker_size),
            self.left_revision_name
        )
    }

    /// The marker at the beginning of the "base" part of a conflict.
    /// It does not contain any newline character.
    pub fn base_marker(&self) -> String {
        format!(
            "{} {}",
            "|".to_string().repeat(self.conflict_marker_size),
            self.base_revision_name
        )
    }

    /// The marker at the end of the "right" (last) part of a conflict.
    /// It does not contain any newline character.
    pub fn right_marker(&self) -> String {
        format!(
            "{} {}",
            ">".to_string().repeat(self.conflict_marker_size),
            self.right_revision_name
        )
    }

    /// The marker before the beginning of "right" (last) part of a conflict.
    /// It does not contain any newline character.
    pub fn middle_marker(&self) -> String {
        "=".to_string().repeat(self.conflict_marker_size)
    }

    pub fn default_compact() -> Self {
        Self {
            diff3: true,
            compact: true,
            conflict_marker_size: 7,
            left_revision_name: "LEFT".to_string(),
            base_revision_name: "BASE".to_string(),
            right_revision_name: "RIGHT".to_string(),
        }
    }
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            diff3: true,
            compact: false,
            conflict_marker_size: 7,
            left_revision_name: "LEFT".to_string(),
            base_revision_name: "BASE".to_string(),
            right_revision_name: "RIGHT".to_string(),
        }
    }
}

enum LineFeedStyle {
    LF,
    CRLF,
    CR,
}

/// Guess if we should use CRLF or just LF from an example file
fn infer_cr_lf_from_file(contents: &str) -> LineFeedStyle {
    let lf_count = contents.split('\n').count();
    let cr_lf_count = contents.split("\r\n").count();
    let cr_count = contents.split('\r').count();
    if cr_lf_count > lf_count / 2 {
        LineFeedStyle::CRLF
    } else if cr_count > lf_count {
        LineFeedStyle::CR
    } else {
        LineFeedStyle::LF
    }
}

/// Renormalize an output file to contain CRLF or just LF by imitating an input file
pub fn imitate_cr_lf_from_input(input_contents: &str, output_contents: &str) -> String {
    let without_crlf = output_contents.replace("\r\n", "\n");
    match infer_cr_lf_from_file(input_contents) {
        LineFeedStyle::LF => without_crlf.replace('\r', "\n"),
        LineFeedStyle::CRLF => without_crlf.replace('\r', "\n").replace('\n', "\r\n"),
        LineFeedStyle::CR => without_crlf.replace('\n', "\r"),
    }
}

pub fn normalize_to_lf(contents: &str) -> String {
    contents.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::imitate_cr_lf_from_input;

    #[test]
    fn test_normalize_cr_lf_to_lf() {
        let input_contents = "a\nb\nc\nd";
        let output_contents = "A\nB\r\nC\rD";

        let result = imitate_cr_lf_from_input(input_contents, output_contents);

        assert_eq!(result, "A\nB\nC\nD");
    }

    #[test]
    fn test_normalize_lf_to_cr_lf() {
        let input_contents = "a\r\nb\r\nc\nd";
        let output_contents = "A\nB\r\nC\rD";

        let result = imitate_cr_lf_from_input(input_contents, output_contents);

        assert_eq!(result, "A\r\nB\r\nC\r\nD");
    }

    #[test]
    fn test_normalize_lf_to_cr() {
        let input_contents = "a\rb\rc\nd";
        let output_contents = "A\rB\r\nC\nD";

        let result = imitate_cr_lf_from_input(input_contents, output_contents);

        assert_eq!(result, "A\rB\rC\rD");
    }
}
