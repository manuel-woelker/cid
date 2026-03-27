use crate::error::CidError;
use crate::result::CidResult;
use std::fmt::Write as _;
use std::process::ExitCode;

/// What does `try_main` provide for CLI binaries?
///
/// It runs a fallible entrypoint, prints a readable error report on failure,
/// and converts the outcome into a process exit code.
pub fn try_main(run: impl FnOnce() -> CidResult<()>) -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{}", format_cli_error("operation failed", &error));
            ExitCode::FAILURE
        }
    }
}

/// What does `try_main_with_headline` provide?
///
/// It behaves like [`try_main`], but lets a binary choose a more specific
/// headline for its top-level error report.
pub fn try_main_with_headline(headline: &str, run: impl FnOnce() -> CidResult<()>) -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{}", format_cli_error(headline, &error));
            ExitCode::FAILURE
        }
    }
}

/// What does `format_cli_error` return?
///
/// It returns a stable, human-readable rendering of a [`CidError`] that is
/// suitable for printing from a command-line binary.
pub fn format_cli_error(headline: &str, error: &CidError) -> String {
    let mut rendered = String::new();
    let _ = writeln!(&mut rendered, "\u{1b}[1;31m━━ {}\u{1b}[0m", headline);
    if error.write_to(&mut rendered).is_err() {
        let _ = writeln!(
            &mut rendered,
            "\u{1b}[1;31m× error\u{1b}[0m failed to render detailed error output"
        );
    }

    let mut causes = Vec::new();
    let mut current = error.source();
    while let Some(cause) = current {
        causes.push((cause.kind().to_string(), cause.location()));
        current = cause.source();
    }

    if !causes.is_empty() {
        let simple_causes: Vec<_> = causes
            .iter()
            .filter(|(cause, _)| !cause.contains('\n'))
            .collect();
        if simple_causes.is_empty() {
            return rendered;
        }

        rendered.push('\n');
        rendered.push_str("\u{1b}[1;33m━━ cause chain\u{1b}[0m\n");
        for (cause, location) in simple_causes {
            let _ = writeln!(&mut rendered, "  • {}", cause);
            let _ = writeln!(
                &mut rendered,
                "    at {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            );
        }
    }

    rendered
}

#[cfg(test)]
mod tests {
    use crate::error::CidError;
    use expect_test::expect;

    use super::format_cli_error;

    #[test]
    fn format_cli_error_renders_headline_and_cause_chain() {
        let error = CidError::message("failed to verify")
            .with_source(CidError::message("missing reference output"));

        expect!([r#"
            ━━ verification failed
            × error failed to verify
              at crates/base/src/cli.rs:90:21
            caused by: missing reference output
                 at crates/base/src/cli.rs:91:26

            ━━ cause chain
              • missing reference output
                at crates/base/src/cli.rs:91:26
        "#])
        .assert_eq(&crate::unansi(&format_cli_error(
            "verification failed",
            &error,
        )));
    }

    #[test]
    fn format_cli_error_skips_cause_chain_for_multiline_cause() {
        let error = CidError::message("failed to load recipe")
            .with_source(CidError::message("line one\nline two"));

        expect!([r#"
            ━━ recipe failed
            × error failed to load recipe
              at crates/base/src/cli.rs:112:21
            caused by:
               line one
               line two
                 at crates/base/src/cli.rs:113:26
        "#])
        .assert_eq(&crate::unansi(&format_cli_error("recipe failed", &error)));
    }
}
