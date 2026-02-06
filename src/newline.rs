use std::borrow::Cow;

/// Type of newlines present in a file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewlineStyle {
    /// Only a line-feed character: `\n`
    Lf,
    /// Carriage return followed by line-feed: `\r\n`
    CrLf,
    /// Only carriage return: `\r`
    Cr,
}

/// Guess if we should use CRLF or just LF from an example string
pub fn infer_newline_style(contents: &str) -> NewlineStyle {
    let lf_count = contents.split('\n').count();
    let cr_lf_count = contents.split("\r\n").count();
    let cr_count = contents.split('\r').count();
    if cr_lf_count > lf_count / 2 {
        NewlineStyle::CrLf
    } else if cr_count > lf_count {
        NewlineStyle::Cr
    } else {
        NewlineStyle::Lf
    }
}

/// Renormalize a string to contain CRLF or just LF
pub fn imitate_newline_style(contents: &str, style: NewlineStyle) -> String {
    let without_crlf = contents.replace("\r\n", "\n");
    match style {
        NewlineStyle::Lf => without_crlf.replace('\r', "\n"),
        NewlineStyle::CrLf => without_crlf.replace('\r', "\n").replace('\n', "\r\n"),
        NewlineStyle::Cr => without_crlf.replace('\n', "\r"),
    }
}

/// Normalize a string to only contain newline characters `\n`, no carriage return `\r`
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
    use super::*;

    #[test]
    fn infer_newline_style() {
        let infer = super::infer_newline_style;

        assert_eq!(infer("a\nb\nc\nd"), NewlineStyle::Lf);
        assert_eq!(infer("a\r\nb\r\nc\nd"), NewlineStyle::CrLf);
        assert_eq!(infer("a\rb\rc\nd"), NewlineStyle::Cr);
    }

    #[test]
    fn imitate_newline_style() {
        let imitate = super::imitate_newline_style;

        let result = imitate("A\nB\r\nC\rD", NewlineStyle::Lf);
        assert_eq!(result, "A\nB\nC\nD");

        let result = imitate("A\nB\r\nC\rD", NewlineStyle::CrLf);
        assert_eq!(result, "A\r\nB\r\nC\r\nD");

        let result = imitate("A\rB\r\nC\nD", NewlineStyle::Cr);
        assert_eq!(result, "A\rB\rC\rD");
    }
}
