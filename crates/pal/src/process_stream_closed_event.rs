use cid_base::timestamp::Timestamp;

use crate::process_output_stream::ProcessOutputStream;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessStreamClosedEvent {
    pub timestamp: Timestamp,
    pub stream: ProcessOutputStream,
}
