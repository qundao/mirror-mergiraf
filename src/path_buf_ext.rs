use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// a temporary trait to implement currently unstable methods
pub trait PathBufExt {
    /// https://doc.rust-lang.org/std/path/struct.PathBuf.html#method.leak
    fn leak<'a>(self) -> &'a mut Path;
    /// https://doc.rust-lang.org/std/path/struct.PathBuf.html#method.with_added_extension
    fn with_added_extension<S: AsRef<OsStr>>(self, extension: S) -> PathBuf;
}
impl PathBufExt for PathBuf {
    fn leak<'a>(self) -> &'a mut Path {
        Box::leak(self.into_boxed_path())
    }
    fn with_added_extension<S: AsRef<OsStr>>(self, extension: S) -> PathBuf {
        let mut new_path = self.into_os_string();
        new_path.push(".");
        new_path.push(extension);
        PathBuf::from(new_path)
    }
}
