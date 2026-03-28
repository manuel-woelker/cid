use std::fmt::Debug;
use std::fs::File;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use cid_base::timestamp::Timestamp;

use crate::pal::{Pal, PalHandle, ReadSeek};
use crate::process_command::ProcessCommand;
use crate::process_event::ProcessEvent;
use crate::process_event_sink::ProcessEventSink;
use crate::process_exited_event::ProcessExitedEvent;
use crate::process_output_event::ProcessOutputEvent;
use crate::process_output_stream::ProcessOutputStream;
use crate::process_result::ProcessResult;
use crate::process_started_event::ProcessStartedEvent;
use crate::process_stream_closed_event::ProcessStreamClosedEvent;

pub struct PalReal {
    reference_instant: Instant,
}

impl PalReal {
    pub fn new_handle() -> PalHandle {
        PalHandle::new(Self::new())
    }

    pub fn new() -> Self {
        Self {
            reference_instant: Instant::now(),
        }
    }

    fn timestamp(&self) -> Timestamp {
        Timestamp::new(self.reference_instant.elapsed().as_nanos())
    }
}

impl Pal for PalReal {
    fn file_exists(&self, path: &FilePath) -> CidResult<bool> {
        Ok(path.as_path().exists())
    }

    fn directory_exists(&self, path: &FilePath) -> CidResult<bool> {
        Ok(path.as_path().is_dir())
    }

    fn read_file(&self, path: &FilePath) -> CidResult<Box<dyn ReadSeek + 'static>> {
        Ok(Box::new(File::open(path).with_context(|| {
            format!("unable to open file `{path}`")
        })?))
    }

    fn create_directory_all(&self, path: &FilePath) -> CidResult<()> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("unable to create directory `{path}`"))?;
        Ok(())
    }

    fn write_file(&self, path: &FilePath, content: &[u8]) -> CidResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent.as_path())
                .with_context(|| format!("unable to create parent directory for `{path}`"))?;
        }
        std::fs::write(path, content).with_context(|| format!("unable to write file `{path}`"))?;
        Ok(())
    }

    fn run_process(
        &self,
        command: &ProcessCommand,
        sink: &mut dyn ProcessEventSink,
    ) -> CidResult<ProcessResult> {
        let started_at = self.timestamp();
        let mut process = Command::new(command.executable.as_str());
        process.args(command.arguments.iter().map(|argument| argument.as_str()));
        process.stdout(Stdio::piped());
        process.stderr(Stdio::piped());

        if let Some(working_directory) = &command.working_directory {
            process.current_dir(working_directory.as_path());
        }

        for variable in &command.environment {
            process.env(variable.name.as_str(), variable.value.as_str());
        }

        let output = process
            .spawn()
            .with_context(|| format!("unable to spawn process `{}`", command.executable))?
            .wait_with_output()
            .with_context(|| format!("unable to wait for process `{}`", command.executable))?;

        sink.handle_event(ProcessEvent::Started(ProcessStartedEvent {
            timestamp: started_at,
            process_id: None,
        }))?;

        if !output.stdout.is_empty() {
            sink.handle_event(ProcessEvent::Output(ProcessOutputEvent {
                timestamp: self.timestamp(),
                stream: ProcessOutputStream::Stdout,
                bytes: output.stdout.clone(),
            }))?;
            sink.handle_event(ProcessEvent::StreamClosed(ProcessStreamClosedEvent {
                timestamp: self.timestamp(),
                stream: ProcessOutputStream::Stdout,
            }))?;
        }

        if !output.stderr.is_empty() {
            sink.handle_event(ProcessEvent::Output(ProcessOutputEvent {
                timestamp: self.timestamp(),
                stream: ProcessOutputStream::Stderr,
                bytes: output.stderr.clone(),
            }))?;
            sink.handle_event(ProcessEvent::StreamClosed(ProcessStreamClosedEvent {
                timestamp: self.timestamp(),
                stream: ProcessOutputStream::Stderr,
            }))?;
        }

        let finished_at = self.timestamp();
        sink.handle_event(ProcessEvent::Exited(ProcessExitedEvent {
            timestamp: finished_at,
            exit_code: output.status.code(),
        }))?;

        Ok(ProcessResult {
            started_at,
            finished_at,
            exit_code: output.status.code(),
        })
    }

    fn now(&self) -> Timestamp {
        self.timestamp()
    }

    fn system_time(&self) -> SystemTime {
        SystemTime::now()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

impl Default for PalReal {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for PalReal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalReal").finish()
    }
}
