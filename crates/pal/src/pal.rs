use std::fmt::Debug;
use std::io::{Read, Seek};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use cid_base::file_path::FilePath;
use cid_base::result::CidResult;
use cid_base::shared_string::SharedString;
use cid_base::timestamp::Timestamp;

use crate::process_command::ProcessCommand;
use crate::process_event_sink::ProcessEventSink;
use crate::process_result::ProcessResult;

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

pub trait Pal: Debug + Sync + Send + 'static {
    fn file_exists(&self, path: &FilePath) -> CidResult<bool>;
    fn directory_exists(&self, path: &FilePath) -> CidResult<bool>;
    fn read_file(&self, path: &FilePath) -> CidResult<Box<dyn ReadSeek + 'static>>;

    fn read_file_to_string(&self, path: &FilePath) -> CidResult<SharedString> {
        let mut buffer = Vec::new();
        self.read_file(path)?.read_to_end(&mut buffer)?;
        SharedString::from_utf8(&buffer)
    }

    fn create_directory_all(&self, path: &FilePath) -> CidResult<()>;
    fn write_file(&self, path: &FilePath, content: &[u8]) -> CidResult<()>;
    fn run_process(
        &self,
        command: &ProcessCommand,
        sink: &mut dyn ProcessEventSink,
    ) -> CidResult<ProcessResult>;
    fn now(&self) -> Timestamp;
    fn system_time(&self) -> SystemTime;
    fn sleep(&self, duration: Duration);
}

#[derive(Debug, Clone)]
pub struct PalHandle(Arc<dyn Pal>);

impl PalHandle {
    pub fn new(pal: impl Pal + 'static) -> Self {
        Self(Arc::new(pal))
    }
}

impl std::ops::Deref for PalHandle {
    type Target = dyn Pal;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
