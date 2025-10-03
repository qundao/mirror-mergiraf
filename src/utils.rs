use std::error::Error;

pub(crate) trait InternalError {
    fn debug_panic(self) -> Self;
}

impl<V, E: Error> InternalError for Result<V, E> {
    /// Panic if this result is an error and we are in debug mode.
    /// This is useful for internal errors that are meant to be never reached,
    /// but that we want to be able to gracefully recover from in release mode.
    #[track_caller]
    #[inline]
    fn debug_panic(self) -> Self {
        if cfg!(debug_assertions) {
            Ok(self.unwrap())
        } else {
            self
        }
    }
}
