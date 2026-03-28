use cid_base::timestamp::Timestamp;

use crate::process_output_stream::ProcessOutputStream;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessOutputEvent {
    pub timestamp: Timestamp,
    pub stream: ProcessOutputStream,
    pub bytes: Vec<u8>,
}
