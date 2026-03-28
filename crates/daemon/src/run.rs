use cid_base::file_path::FilePath;
use cid_base::shared_string::SharedString;
use serde::{Deserialize, Serialize};

use crate::run_status::RunStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    id: u64,
    repository_id: u64,
    repository_name: SharedString,
    branch: SharedString,
    commit_sha: SharedString,
    status: RunStatus,
    queued_at_ms: u64,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    steps: Vec<RunStep>,
    events: Vec<RunEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStep {
    name: SharedString,
    command: SharedString,
    image: SharedString,
    status: RunStatus,
    exit_code: Option<i32>,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    duration_ms: Option<u64>,
    log_path: Option<FilePath>,
    artifact_paths: Vec<FilePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvent {
    timestamp_ms: u64,
    message: SharedString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    pub total_runs: usize,
    pub queued_runs: usize,
    pub running_runs: usize,
    pub passed_runs: usize,
    pub failed_runs: usize,
    pub canceled_runs: usize,
}

impl Run {
    pub fn new(
        id: u64,
        repository_id: u64,
        repository_name: impl Into<SharedString>,
        branch: impl Into<SharedString>,
        commit_sha: impl Into<SharedString>,
        queued_at_ms: u64,
        steps: Vec<RunStep>,
    ) -> Self {
        let mut run = Self {
            id,
            repository_id,
            repository_name: repository_name.into(),
            branch: branch.into(),
            commit_sha: commit_sha.into(),
            status: RunStatus::Queued,
            queued_at_ms,
            started_at_ms: None,
            finished_at_ms: None,
            steps,
            events: Vec::new(),
        };
        run.push_event(queued_at_ms, "run queued");
        run
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }

    pub fn repository_name(&self) -> &str {
        self.repository_name.as_str()
    }

    pub fn branch(&self) -> &str {
        self.branch.as_str()
    }

    pub fn commit_sha(&self) -> &str {
        self.commit_sha.as_str()
    }

    pub fn status(&self) -> RunStatus {
        self.status
    }

    pub fn queued_at_ms(&self) -> u64 {
        self.queued_at_ms
    }

    pub fn started_at_ms(&self) -> Option<u64> {
        self.started_at_ms
    }

    pub fn finished_at_ms(&self) -> Option<u64> {
        self.finished_at_ms
    }

    pub fn steps(&self) -> &[RunStep] {
        &self.steps
    }

    pub fn events(&self) -> &[RunEvent] {
        &self.events
    }

    pub fn includes_commit(&self, repository_id: u64, branch: &str, commit_sha: &str) -> bool {
        self.repository_id == repository_id
            && self.branch == branch
            && self.commit_sha == commit_sha
    }

    pub(crate) fn cancel(&mut self, timestamp_ms: u64, reason: &str) {
        self.status = RunStatus::Canceled;
        self.finished_at_ms = Some(timestamp_ms);
        self.push_event(timestamp_ms, reason);
    }

    pub(crate) fn start(&mut self, timestamp_ms: u64) {
        self.status = RunStatus::Running;
        self.started_at_ms = Some(timestamp_ms);
        self.push_event(timestamp_ms, "run started");
    }

    pub(crate) fn finish(&mut self, timestamp_ms: u64, status: RunStatus) {
        self.status = status;
        self.finished_at_ms = Some(timestamp_ms);
        self.push_event(
            timestamp_ms,
            format!("run finished with status {}", status.label()),
        );
    }

    pub(crate) fn steps_mut(&mut self) -> &mut [RunStep] {
        &mut self.steps
    }

    pub(crate) fn push_event(&mut self, timestamp_ms: u64, message: impl Into<SharedString>) {
        self.events.push(RunEvent {
            timestamp_ms,
            message: message.into(),
        });
    }
}

impl RunStep {
    pub fn new(
        name: impl Into<SharedString>,
        command: impl Into<SharedString>,
        image: impl Into<SharedString>,
        artifact_paths: Vec<FilePath>,
    ) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            image: image.into(),
            status: RunStatus::Queued,
            exit_code: None,
            started_at_ms: None,
            finished_at_ms: None,
            duration_ms: None,
            log_path: None,
            artifact_paths,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn command(&self) -> &str {
        self.command.as_str()
    }

    pub fn image(&self) -> &str {
        self.image.as_str()
    }

    pub fn status(&self) -> RunStatus {
        self.status
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub fn log_path(&self) -> Option<&FilePath> {
        self.log_path.as_ref()
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    pub fn artifact_paths(&self) -> &[FilePath] {
        &self.artifact_paths
    }

    pub(crate) fn mark_running(&mut self, timestamp_ms: u64) {
        self.status = RunStatus::Running;
        self.started_at_ms = Some(timestamp_ms);
    }

    pub(crate) fn mark_finished(
        &mut self,
        timestamp_ms: u64,
        status: RunStatus,
        exit_code: Option<i32>,
        log_path: Option<FilePath>,
    ) {
        self.status = status;
        self.exit_code = exit_code;
        self.finished_at_ms = Some(timestamp_ms);
        self.duration_ms = self
            .started_at_ms
            .map(|started_at_ms| timestamp_ms.saturating_sub(started_at_ms));
        self.log_path = log_path;
    }
}

impl RunEvent {
    pub fn timestamp_ms(&self) -> u64 {
        self.timestamp_ms
    }

    pub fn message(&self) -> &str {
        self.message.as_str()
    }
}

impl RunSummary {
    pub fn from_runs(runs: &[Run]) -> Self {
        let mut summary = Self {
            total_runs: runs.len(),
            queued_runs: 0,
            running_runs: 0,
            passed_runs: 0,
            failed_runs: 0,
            canceled_runs: 0,
        };

        for run in runs {
            match run.status {
                RunStatus::Queued => summary.queued_runs += 1,
                RunStatus::Running => summary.running_runs += 1,
                RunStatus::Passed => summary.passed_runs += 1,
                RunStatus::Failed => summary.failed_runs += 1,
                RunStatus::Canceled => summary.canceled_runs += 1,
            }
        }

        summary
    }

    pub fn total_runs(&self) -> usize {
        self.total_runs
    }
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;

    use super::{Run, RunStep, RunSummary};
    use crate::run_status::RunStatus;

    #[test]
    fn run_summary_counts_statuses() {
        let runs = vec![
            Run::new(
                1,
                1,
                "cid",
                "main",
                "abc",
                10,
                vec![RunStep::new("test", "cargo test", "alpine", Vec::new())],
            ),
            Run::new(
                2,
                1,
                "cid",
                "main",
                "def",
                20,
                vec![RunStep::new(
                    "test",
                    "cargo test",
                    "alpine",
                    vec![FilePath::new("dist")],
                )],
            ),
        ];
        let summary = RunSummary::from_runs(&runs);
        assert_eq!(summary.total_runs(), 2);
    }

    #[test]
    fn run_step_records_completion_metadata() {
        let mut step = RunStep::new("test", "cargo test", "alpine", Vec::new());

        step.mark_running(100);
        step.mark_finished(
            175,
            RunStatus::Passed,
            Some(0),
            Some(FilePath::new("step.log")),
        );

        assert_eq!(step.status(), RunStatus::Passed);
        assert_eq!(step.exit_code(), Some(0));
        assert_eq!(step.duration_ms(), Some(75));
    }
}
