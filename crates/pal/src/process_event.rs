use crate::process_exited_event::ProcessExitedEvent;
use crate::process_output_event::ProcessOutputEvent;
use crate::process_started_event::ProcessStartedEvent;
use crate::process_stream_closed_event::ProcessStreamClosedEvent;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProcessEvent {
    Started(ProcessStartedEvent),
    Output(ProcessOutputEvent),
    StreamClosed(ProcessStreamClosedEvent),
    Exited(ProcessExitedEvent),
}
