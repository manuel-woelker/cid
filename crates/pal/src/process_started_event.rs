use cid_base::timestamp::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessStartedEvent {
    pub timestamp: Timestamp,
    pub process_id: Option<u32>,
}
