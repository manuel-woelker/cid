use serde::{Deserialize, Serialize};

/// Broad runtime state for the local cid daemon.
///
/// This is intentionally small for now. The crate boundary matters more than
/// a fully detailed domain model at this stage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CidDaemon {
    started: bool,
}

impl CidDaemon {
    /// Creates a daemon value in the stopped state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the daemon as started.
    pub fn start(&mut self) {
        self.started = true;
    }

    /// Returns whether the daemon has been started.
    pub fn is_started(&self) -> bool {
        self.started
    }
}

#[cfg(test)]
mod tests {
    use super::CidDaemon;

    #[test]
    fn daemon_starts_from_stopped_state() {
        let daemon = CidDaemon::new();

        assert!(!daemon.is_started());
    }

    #[test]
    fn daemon_reports_started_after_start() {
        let mut daemon = CidDaemon::new();

        daemon.start();

        assert!(daemon.is_started());
    }
}
