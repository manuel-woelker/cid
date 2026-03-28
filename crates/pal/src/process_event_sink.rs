use cid_base::result::CidResult;

use crate::process_event::ProcessEvent;

pub trait ProcessEventSink {
    fn handle_event(&mut self, event: ProcessEvent) -> CidResult<()>;
}
