use std::fs;

use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};

use crate::daemon::DaemonState;

#[derive(Debug, Clone)]
pub struct CidStateStore {
    state_dir: FilePath,
}

impl CidStateStore {
    pub fn new(state_dir: FilePath) -> Self {
        Self { state_dir }
    }

    pub fn load(&self) -> CidResult<DaemonState> {
        self.ensure_layout()?;
        let state_path = self.state_file_path();

        if !state_path.as_path().exists() {
            return Ok(DaemonState::default());
        }

        let contents = fs::read_to_string(&state_path)
            .with_context(|| format!("failed to read daemon state file `{state_path}`"))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse daemon state file `{state_path}`"))
    }

    pub fn save(&self, state: &DaemonState) -> CidResult<()> {
        self.ensure_layout()?;
        let rendered = serde_yaml::to_string(state).context("failed to serialize daemon state")?;
        fs::write(self.state_file_path(), rendered).context("failed to write daemon state")?;
        Ok(())
    }

    pub fn write_step_log(
        &self,
        run_id: u64,
        step_index: usize,
        contents: &str,
    ) -> CidResult<FilePath> {
        let run_dir = self.logs_dir().join(format!("run-{run_id}"));
        fs::create_dir_all(&run_dir)
            .with_context(|| format!("failed to create run log directory `{run_dir}`"))?;

        let log_path = run_dir.join(format!("step-{step_index}.log"));
        fs::write(&log_path, contents)
            .with_context(|| format!("failed to write step log `{log_path}`"))?;
        Ok(log_path)
    }

    pub fn state_file_path(&self) -> FilePath {
        self.state_dir.join("state.yaml")
    }

    pub fn state_dir(&self) -> &FilePath {
        &self.state_dir
    }

    fn logs_dir(&self) -> FilePath {
        self.state_dir.join("logs")
    }

    fn ensure_layout(&self) -> CidResult<()> {
        fs::create_dir_all(self.logs_dir())
            .with_context(|| format!("failed to create state directory `{}`", self.state_dir))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use cid_base::file_path::FilePath;

    use crate::daemon::DaemonState;
    use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};

    use super::CidStateStore;

    fn temp_dir(name: &str) -> FilePath {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        FilePath::new(std::env::temp_dir().join(format!("cid-{name}-{unique}")))
    }

    #[test]
    fn persistence_round_trip_restores_state() {
        let state_dir = temp_dir("state-store");
        let store = CidStateStore::new(state_dir.clone());
        let state = DaemonState {
            repositories: vec![Repository::new(
                1,
                "cid",
                FilePath::new("/repos/cid"),
                vec![BranchRule::new("main")],
                Pipeline::new(
                    "rust:1.85",
                    vec![PipelineStep::new("test", "cargo test")],
                    Vec::new(),
                ),
            )],
            ..DaemonState::default()
        };

        store.save(&state).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(loaded.repositories(), state.repositories());
        fs::remove_dir_all(state_dir.as_path()).unwrap();
    }

    #[test]
    fn step_logs_are_written_under_run_directories() {
        let state_dir = temp_dir("step-log");
        let store = CidStateStore::new(state_dir.clone());

        let log_path = store.write_step_log(12, 1, "hello log").unwrap();

        assert!(log_path.as_path().exists());
        assert_eq!(fs::read_to_string(log_path.as_path()).unwrap(), "hello log");
        fs::remove_dir_all(state_dir.as_path()).unwrap();
    }
}
