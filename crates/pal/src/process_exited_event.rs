use cid_base::timestamp::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessExitedEvent {
    pub timestamp: Timestamp,
    pub exit_code: Option<i32>,
}
