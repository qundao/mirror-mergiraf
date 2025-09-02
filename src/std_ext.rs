//! currently unstable things from stdlib, vendored in

use std::{ffi::OsStr, path::PathBuf};

pub trait PathBufExt {
    /// <https://doc.rust-lang.org/std/path/struct.PathBuf.html#method.with_added_extension>
    fn with_added_extension<S: AsRef<OsStr>>(self, extension: S) -> Self;
}
impl PathBufExt for PathBuf {
    fn with_added_extension<S: AsRef<OsStr>>(self, extension: S) -> Self {
        let mut new_path = self.into_os_string();
        new_path.push(".");
        new_path.push(extension);
        Self::from(new_path)
    }
}

pub trait StrExt {
    /// <https://doc.rust-lang.org/std/primitive.str.html#method.ceil_char_boundary>
    fn ceil_char_boundary(self, index: usize) -> usize;
}

impl StrExt for &'_ str {
    fn ceil_char_boundary(self, index: usize) -> usize {
        let len = self.len();
        if index > len {
            len
        } else {
            (index..=len)
                .find(|&i| self.is_char_boundary(i))
                .expect("`i = len` must be a char boundary") // otherwise `self` wouldn't have been a valid `&str` to begin with
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[allow(unstable_name_collisions)]
    #[test]
    fn char_boundary_at_end() {
        let string = "❤️🧡💛";
        assert_eq!(string.ceil_char_boundary(0), 0);
        assert_eq!(string.ceil_char_boundary(1), 3);
        assert_eq!(string.ceil_char_boundary(3), 3);
        assert_eq!(string.ceil_char_boundary(4), 6);
        assert_eq!(string.ceil_char_boundary(13), string.len());
    }
}
