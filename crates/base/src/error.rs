use std::error::Error as StdError;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use tracing_error::{SpanTrace, SpanTraceStatus};

use crate::shared_string::SharedString;
use crate::unansi;

#[derive(Debug)]
pub enum ErrorKind {
    Message(SharedString),
    Std(Box<dyn StdError + Send + Sync + 'static>),
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::Std(error) => Display::fmt(error, f),
        }
    }
}

#[derive(Debug)]
pub struct CidError {
    kind: ErrorKind,
    source: Option<Box<CidError>>,
    location: &'static Location<'static>,
    span_trace: SpanTrace,
}

impl CidError {
    #[track_caller]
    pub fn new(kind: ErrorKind) -> Self {
        Self::at_location(kind, Location::caller())
    }

    pub fn at_location(kind: ErrorKind, location: &'static Location<'static>) -> Self {
        Self {
            kind,
            source: None,
            location,
            span_trace: SpanTrace::capture(),
        }
    }

    #[track_caller]
    pub fn message(s: impl Into<SharedString>) -> Self {
        Self::message_at_location(s, Location::caller())
    }

    pub fn message_at_location(
        s: impl Into<SharedString>,
        location: &'static Location<'static>,
    ) -> Self {
        Self::at_location(ErrorKind::Message(s.into()), location)
    }

    #[track_caller]
    pub fn std(error: impl StdError + Send + Sync + 'static) -> Self {
        Self::std_at_location(error, Location::caller())
    }

    pub fn std_at_location(
        error: impl StdError + Send + Sync + 'static,
        location: &'static Location<'static>,
    ) -> Self {
        Self::at_location(ErrorKind::Std(Box::new(error)), location)
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn source(&self) -> Option<&CidError> {
        self.source.as_deref()
    }

    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }

    pub fn span_trace(&self) -> &SpanTrace {
        &self.span_trace
    }

    pub fn with_source(mut self, source: impl Into<CidError>) -> Self {
        self.source = Some(Box::new(source.into()));
        self
    }

    #[track_caller]
    pub fn with_std_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(CidError::std_at_location(
            source,
            Location::caller(),
        )));
        self
    }

    pub fn with_std_source_at_location(
        mut self,
        source: impl StdError + Send + Sync + 'static,
        location: &'static Location<'static>,
    ) -> Self {
        self.source = Some(Box::new(CidError::std_at_location(source, location)));
        self
    }

    pub fn write_to(&self, write: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(write, "{} {}", style("1;31", "× error"), self.kind)?;
        self.write_details(write, "")?;
        Ok(())
    }

    pub fn to_test_string(&self) -> String {
        let mut test_string = String::new();
        if self.write_to(&mut test_string).is_err() {
            test_string.push_str("failed to render error");
        }
        unansi(&test_string)
    }
}

impl CidError {
    fn write_details(&self, write: &mut dyn std::fmt::Write, prefix: &str) -> std::fmt::Result {
        let show_span_trace = self.source.is_none();

        writeln!(
            write,
            "{}{} {}:{}:{}",
            prefix,
            style("2;37", "  at"),
            self.location.file(),
            self.location.line(),
            self.location.column()
        )?;

        if show_span_trace && self.span_trace.status() == SpanTraceStatus::CAPTURED {
            writeln!(write, "{}{}", prefix, style("36", "  span trace:"))?;
            write_span_trace(write, prefix, &self.span_trace)?;
        }

        if let Some(source) = self.source.as_deref() {
            write_rendered_cause(
                write,
                prefix,
                &style("33", "caused by:"),
                &source.kind.to_string(),
            )?;
            source.write_child_details(write, &format!("{prefix}   "))?;
        }

        Ok(())
    }

    fn write_child_details(
        &self,
        write: &mut dyn std::fmt::Write,
        prefix: &str,
    ) -> std::fmt::Result {
        let show_span_trace = self.source.is_none();

        writeln!(
            write,
            "{}{} {}:{}:{}",
            prefix,
            style("2;37", "  at"),
            self.location.file(),
            self.location.line(),
            self.location.column()
        )?;

        if show_span_trace && self.span_trace.status() == SpanTraceStatus::CAPTURED {
            writeln!(write, "{}{}", prefix, style("36", "  span trace:"))?;
            write_span_trace(write, prefix, &self.span_trace)?;
        }

        if let Some(source) = self.source.as_deref() {
            write_rendered_cause(
                write,
                prefix,
                &style("33", "caused by:"),
                &source.kind.to_string(),
            )?;
            source.write_child_details(write, &format!("{prefix}   "))?;
        }

        Ok(())
    }
}

fn write_rendered_cause(
    write: &mut dyn std::fmt::Write,
    prefix: &str,
    label: &str,
    rendered: &str,
) -> std::fmt::Result {
    if rendered.contains('\n') {
        writeln!(write, "{prefix}{label}")?;
        for line in rendered.lines() {
            writeln!(write, "{prefix}   {line}")?;
        }
    } else {
        writeln!(write, "{prefix}{label} {rendered}")?;
    }

    Ok(())
}

fn write_span_trace(
    write: &mut dyn std::fmt::Write,
    prefix: &str,
    span_trace: &SpanTrace,
) -> std::fmt::Result {
    let mut result = Ok(());
    let mut span_index = 0;

    span_trace.with_spans(|metadata, fields| {
        if span_index > 0 && writeln!(write).is_err() {
            result = Err(std::fmt::Error);
            return false;
        }

        if writeln!(
            write,
            "{}    {}: {}::{}",
            prefix,
            span_index,
            metadata.target(),
            metadata.name()
        )
        .is_err()
        {
            result = Err(std::fmt::Error);
            return false;
        }

        if !fields.is_empty()
            && writeln!(
                write,
                "{}       {}",
                prefix,
                format_span_trace_fields(fields)
            )
            .is_err()
        {
            result = Err(std::fmt::Error);
            return false;
        }

        if let Some((file, line)) = metadata
            .file()
            .and_then(|file| metadata.line().map(|line| (file, line)))
            && writeln!(write, "{}       at {}:{}", prefix, file, line).is_err()
        {
            result = Err(std::fmt::Error);
            return false;
        }

        span_index += 1;
        true
    });

    result
}

fn format_span_trace_fields(fields: &str) -> String {
    let mut formatted = String::new();

    for (index, field) in fields.split_whitespace().enumerate() {
        if index > 0 {
            formatted.push(' ');
        }

        if let Some((key, value)) = field.split_once('=') {
            formatted.push_str(key);
            formatted.push(':');
            formatted.push(' ');
            formatted.push_str(&style("1;97", value));
        } else {
            formatted.push_str(field);
        }
    }

    formatted
}

fn style(code: &str, text: &str) -> String {
    format!("\u{1b}[{code}m{text}\u{1b}[0m")
}

impl<T> From<T> for CidError
where
    T: StdError + Send + Sync + 'static,
{
    #[track_caller]
    fn from(value: T) -> Self {
        Self::std(value)
    }
}

#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => {
        $crate::error::CidError::message(format!($($arg)*))
    };
}
pub use err;

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::err!($($arg)*))
    };
}
pub use bail;

#[cfg(test)]
mod tests {
    use super::format_span_trace_fields;

    #[test]
    fn test_format_span_trace_fields() {
        let rendered = format_span_trace_fields(
            "sources_dir=verification/sources output_dir=verification/output/cid",
        );
        let rendered = crate::unansi(&rendered);

        assert_eq!(
            rendered,
            "sources_dir: verification/sources output_dir: verification/output/cid"
        );
    }
}
