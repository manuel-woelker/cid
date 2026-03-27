use serde::{Deserialize, Serialize};

/// Lifecycle state for a daemon-managed run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Canceled,
}
