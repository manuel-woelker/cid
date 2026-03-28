use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use cid_pal::pal::PalHandle;

use crate::daemon::DaemonState;

#[derive(Debug, Clone)]
pub struct CidStateStore {
    state_dir: FilePath,
    pal: PalHandle,
}

impl CidStateStore {
    pub fn new(state_dir: FilePath, pal: PalHandle) -> Self {
        Self { state_dir, pal }
    }

    pub fn load(&self) -> CidResult<DaemonState> {
        self.ensure_layout()?;
        let state_path = self.state_file_path();

        if !self.pal.file_exists(&state_path)? {
            return Ok(DaemonState::default());
        }

        let contents = self
            .pal
            .read_file_to_string(&state_path)
            .with_context(|| format!("failed to read daemon state file `{state_path}`"))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse daemon state file `{state_path}`"))
    }

    pub fn save(&self, state: &DaemonState) -> CidResult<()> {
        self.ensure_layout()?;
        let rendered = serde_yaml::to_string(state).context("failed to serialize daemon state")?;
        self.pal
            .write_file(&self.state_file_path(), rendered.as_bytes())
            .context("failed to write daemon state")?;
        Ok(())
    }

    pub fn write_step_log(
        &self,
        run_id: u64,
        step_index: usize,
        contents: &str,
    ) -> CidResult<FilePath> {
        let run_dir = self.logs_dir().join(format!("run-{run_id}"));
        self.pal
            .create_directory_all(&run_dir)
            .with_context(|| format!("failed to create run log directory `{run_dir}`"))?;

        let log_path = run_dir.join(format!("step-{step_index}.log"));
        self.pal
            .write_file(&log_path, contents.as_bytes())
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
        self.pal
            .create_directory_all(&self.logs_dir())
            .with_context(|| format!("failed to create state directory `{}`", self.state_dir))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;
    use cid_pal::pal_mock::PalMock;

    use crate::daemon::DaemonState;
    use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};

    use super::CidStateStore;

    #[test]
    fn persistence_round_trip_restores_state() {
        let pal = PalMock::new();
        let state_dir = FilePath::new("state");
        let store =
            CidStateStore::new(state_dir.clone(), cid_pal::pal::PalHandle::new(pal.clone()));
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
        assert!(pal.read_file_string("state/state.yaml").is_some());
    }

    #[test]
    fn step_logs_are_written_under_run_directories() {
        let pal = PalMock::new();
        let state_dir = FilePath::new("state");
        let store =
            CidStateStore::new(state_dir.clone(), cid_pal::pal::PalHandle::new(pal.clone()));

        let log_path = store.write_step_log(12, 1, "hello log").unwrap();

        assert_eq!(log_path.as_str(), "state/logs/run-12/step-1.log");
        assert_eq!(
            pal.read_file_string("state/logs/run-12/step-1.log")
                .as_deref(),
            Some("hello log")
        );
    }
}
