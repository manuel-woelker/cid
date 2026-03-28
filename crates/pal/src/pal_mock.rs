use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use cid_base::RwLock;
use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, OptionExt};
use cid_base::timestamp::Timestamp;
use expect_test::Expect;

use crate::pal::{Pal, ReadSeek};
use crate::process_command::ProcessCommand;
use crate::process_event::ProcessEvent;
use crate::process_event_sink::ProcessEventSink;
use crate::process_result::ProcessResult;

#[derive(Clone)]
pub struct PalMock {
    inner: Arc<RwLock<PalMockInner>>,
}

struct PalMockInner {
    effects_string: String,
    file_map: HashMap<FilePath, Vec<u8>>,
    directories: HashSet<FilePath>,
    process_executions: HashMap<ProcessCommand, (Vec<ProcessEvent>, ProcessResult)>,
    current_timestamp: Timestamp,
    current_system_time: SystemTime,
}

impl PalMock {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(PalMockInner {
                effects_string: String::new(),
                file_map: HashMap::new(),
                directories: HashSet::new(),
                process_executions: HashMap::new(),
                current_timestamp: Timestamp::new(0),
                current_system_time: SystemTime::UNIX_EPOCH,
            })),
        }
    }

    pub fn log_effect(&self, effect: impl AsRef<str>) {
        let mut inner = self.inner.write();
        inner.effects_string.push_str(effect.as_ref());
        inner.effects_string.push('\n');
    }

    pub fn verify_effects(&self, expected: Expect) {
        expected.assert_eq(&self.inner.read().effects_string);
    }

    pub fn clear_effects(&self) {
        self.inner.write().effects_string.clear();
    }

    pub fn set_file(&self, file_path: &str, content: impl Into<Vec<u8>>) {
        self.inner
            .write()
            .file_map
            .insert(FilePath::from(file_path), content.into());
    }

    pub fn set_directory(&self, path: &str) {
        self.inner.write().directories.insert(FilePath::from(path));
    }

    pub fn set_process_execution(
        &self,
        command: ProcessCommand,
        events: Vec<ProcessEvent>,
        result: ProcessResult,
    ) {
        self.inner
            .write()
            .process_executions
            .insert(command, (events, result));
    }

    pub fn set_current_timestamp(&self, timestamp: Timestamp) {
        self.inner.write().current_timestamp = timestamp;
    }

    pub fn set_current_system_time(&self, system_time: SystemTime) {
        self.inner.write().current_system_time = system_time;
    }

    pub fn read_file_string(&self, path: &str) -> Option<String> {
        self.inner
            .read()
            .file_map
            .get(&FilePath::from(path))
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    }
}

impl Default for PalMock {
    fn default() -> Self {
        Self::new()
    }
}

impl Pal for PalMock {
    fn file_exists(&self, path: &FilePath) -> CidResult<bool> {
        Ok(self.inner.read().file_map.contains_key(path)
            || self.inner.read().directories.contains(path))
    }

    fn directory_exists(&self, path: &FilePath) -> CidResult<bool> {
        Ok(self.inner.read().directories.contains(path))
    }

    fn read_file(&self, path: &FilePath) -> CidResult<Box<dyn ReadSeek + 'static>> {
        self.log_effect(format!("READ FILE: {path}"));
        Ok(Box::new(Cursor::new(
            self.inner
                .read()
                .file_map
                .get(path)
                .with_context(|| format!("file `{path}` does not exist"))?
                .clone(),
        )))
    }

    fn create_directory_all(&self, path: &FilePath) -> CidResult<()> {
        self.log_effect(format!("CREATE DIRECTORY: {path}"));
        self.inner.write().directories.insert(path.clone());
        Ok(())
    }

    fn write_file(&self, path: &FilePath, content: &[u8]) -> CidResult<()> {
        self.log_effect(format!(
            "WRITE FILE: {} -> {}",
            path,
            String::from_utf8_lossy(content)
        ));
        if let Some(parent) = path.parent() {
            self.inner.write().directories.insert(parent);
        }
        self.inner
            .write()
            .file_map
            .insert(path.clone(), content.to_vec());
        Ok(())
    }

    fn run_process(
        &self,
        command: &ProcessCommand,
        sink: &mut dyn ProcessEventSink,
    ) -> CidResult<ProcessResult> {
        self.log_effect(format!(
            "RUN PROCESS: {} {}",
            command.executable,
            command
                .arguments
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));

        let (events, result) = self
            .inner
            .read()
            .process_executions
            .get(command)
            .cloned()
            .with_context(|| {
                format!(
                    "no process execution registered for `{}`",
                    command.executable
                )
            })?;

        for event in events {
            sink.handle_event(event)?;
        }

        Ok(result)
    }

    fn now(&self) -> Timestamp {
        self.inner.read().current_timestamp
    }

    fn system_time(&self) -> SystemTime {
        self.inner.read().current_system_time
    }

    fn sleep(&self, duration: Duration) {
        self.log_effect(format!("SLEEP: {}ms", duration.as_millis()));
        self.inner.write().current_system_time += duration;
    }
}

impl Debug for PalMock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalMock").finish()
    }
}
