use cid_base::result::{OptionExt, ResultExt};
use expect_test::expect;
use std::io;

/* 📖 # Why keep result formatting tests in a separate file?
These assertions verify `#[track_caller]` output, so the expected file and line numbers are part
of the behavior under test. Keeping them in a dedicated file makes the snapshots stable when
`result.rs` changes for unrelated reasons.
*/
#[test]
fn result_with_context_formats_error_with_caller_location() {
    let result: Result<(), io::Error> =
        Err(io::Error::new(io::ErrorKind::NotFound, "config missing"));
    let error = result.with_context(|| "failed to load config").unwrap_err();

    expect!([r#"
        × error failed to load config
          at crates/base/tests/result_formatting.rs:14:24
        caused by: config missing
             at crates/base/tests/result_formatting.rs:14:24
    "#])
    .assert_eq(&error.to_test_string());
}

#[test]
fn result_context_formats_error_with_caller_location() {
    let result: Result<(), io::Error> =
        Err(io::Error::new(io::ErrorKind::NotFound, "config missing"));
    let error = result.context("failed to load config").unwrap_err();

    expect!([r#"
        × error failed to load config
          at crates/base/tests/result_formatting.rs:29:24
        caused by: config missing
             at crates/base/tests/result_formatting.rs:29:24
    "#])
    .assert_eq(&error.to_test_string());
}

#[test]
fn option_context_formats_error_with_caller_location() {
    let value: Option<i32> = None;
    let error = value.context("missing value").unwrap_err();

    expect!([r#"
        × error missing value
          at crates/base/tests/result_formatting.rs:43:23
    "#])
    .assert_eq(&error.to_test_string());
}
