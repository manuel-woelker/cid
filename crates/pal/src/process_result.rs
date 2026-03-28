use cid_base::timestamp::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessResult {
    pub started_at: Timestamp,
    pub finished_at: Timestamp,
    pub exit_code: Option<i32>,
}
