use crate::error::CidError;
use crate::shared_string::SharedString;
use std::error::Error as StdError;
use std::panic::Location;

pub type CidResult<T> = Result<T, CidError>;

pub trait ResultExt<T> {
    #[track_caller]
    fn context(self, context: impl Into<SharedString>) -> CidResult<T>;

    #[track_caller]
    fn with_context<C, S>(self, context: C) -> CidResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>;
}

pub trait OptionExt<T> {
    #[track_caller]
    fn context(self, context: impl Into<SharedString>) -> CidResult<T>;

    #[track_caller]
    fn with_context<C, S>(self, context: C) -> CidResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: StdError + Send + Sync + 'static,
{
    #[track_caller]
    fn context(self, context: impl Into<SharedString>) -> CidResult<T> {
        let caller = Location::caller();
        self.map_err(|error| {
            CidError::message_at_location(context.into(), caller)
                .with_std_source_at_location(error, caller)
        })
    }

    #[track_caller]
    fn with_context<C, S>(self, context: C) -> CidResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>,
    {
        let caller = Location::caller();
        self.map_err(|error| {
            CidError::message_at_location(context(), caller)
                .with_std_source_at_location(error, caller)
        })
    }
}

impl<T> ResultExt<T> for Result<T, CidError> {
    #[track_caller]
    fn context(self, context: impl Into<SharedString>) -> CidResult<T> {
        let caller = Location::caller();
        self.map_err(|error| {
            CidError::message_at_location(context.into(), caller).with_source(error)
        })
    }

    #[track_caller]
    fn with_context<C, S>(self, context: C) -> CidResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>,
    {
        let caller = Location::caller();
        self.map_err(|error| CidError::message_at_location(context(), caller).with_source(error))
    }
}

impl<T> OptionExt<T> for Option<T> {
    #[track_caller]
    fn context(self, context: impl Into<SharedString>) -> CidResult<T> {
        let caller = Location::caller();
        self.ok_or_else(|| CidError::message_at_location(context.into(), caller))
    }

    #[track_caller]
    fn with_context<C, S>(self, context: C) -> CidResult<T>
    where
        C: FnOnce() -> S,
        S: Into<SharedString>,
    {
        let caller = Location::caller();
        self.ok_or_else(|| CidError::message_at_location(context(), caller))
    }
}

#[cfg(test)]
mod tests {
    use crate::result::{OptionExt, ResultExt};
    use std::io;

    #[test]
    fn test_with_context_is_lazy_for_ok_results() {
        use std::cell::Cell;

        let context_called = Cell::new(false);
        let result: Result<i32, io::Error> = Ok(123);
        let value = result
            .with_context(|| {
                context_called.set(true);
                "should not be used"
            })
            .unwrap();
        assert_eq!(value, 123);
        assert!(!context_called.get());
    }

    #[test]
    fn test_option_with_context_is_lazy_for_some_results() {
        use std::cell::Cell;

        let context_called = Cell::new(false);
        let value = Some(123)
            .with_context(|| {
                context_called.set(true);
                "should not be used"
            })
            .unwrap();
        assert_eq!(value, 123);
        assert!(!context_called.get());
    }

    #[test]
    fn test_with_context_wraps_cid_error_results() {
        let result: crate::result::CidResult<i32> = Err(crate::err!("root cause"));
        let error = result.with_context(|| "outer context").unwrap_err();
        let rendered = error.to_test_string();

        assert!(rendered.contains("outer context"));
        assert!(rendered.contains("root cause"));
    }
}
