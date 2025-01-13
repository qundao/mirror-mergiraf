use std::borrow::Cow;

#[allow(clippy::upper_case_acronyms)]
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

/// Renormalize an output file to contain CRLF or just LF by imitating an input file
pub fn imitate_cr_lf_from_input(input_contents: &str, output_contents: &str) -> String {
    let without_crlf = output_contents.replace("\r\n", "\n");
    match infer_cr_lf_from_file(input_contents) {
        LineFeedStyle::LF => without_crlf.replace('\r', "\n"),
        LineFeedStyle::CRLF => without_crlf.replace('\r', "\n").replace('\n', "\r\n"),
        LineFeedStyle::CR => without_crlf.replace('\n', "\r"),
    }
}

pub fn normalize_to_lf<'a>(contents: impl Into<Cow<'a, str>>) -> Cow<'a, str> {
    let contents = contents.into();
    if !contents.contains('\r') {
        contents
    } else {
        let res = contents.replace("\r\n", "\n").replace('\r', "\n");
        Cow::Owned(res)
    }
}

#[cfg(test)]
mod tests {
    use super::imitate_cr_lf_from_input;

    #[test]
    fn normalize_cr_lf_to_lf() {
        let input_contents = "a\nb\nc\nd";
        let output_contents = "A\nB\r\nC\rD";

        let result = imitate_cr_lf_from_input(input_contents, output_contents);

        assert_eq!(result, "A\nB\nC\nD");
    }

    #[test]
    fn normalize_lf_to_cr_lf() {
        let input_contents = "a\r\nb\r\nc\nd";
        let output_contents = "A\nB\r\nC\rD";

        let result = imitate_cr_lf_from_input(input_contents, output_contents);

        assert_eq!(result, "A\r\nB\r\nC\r\nD");
    }

    #[test]
    fn normalize_lf_to_cr() {
        let input_contents = "a\rb\rc\nd";
        let output_contents = "A\rB\r\nC\nD";

        let result = imitate_cr_lf_from_input(input_contents, output_contents);

        assert_eq!(result, "A\rB\rC\rD");
    }
}
