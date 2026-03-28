use cid_base::logging::info;
use cid_base::result::CidResult;
use cid_pal::pal::PalHandle;
use cid_pal::process_command::ProcessCommand;
use cid_pal::process_event::ProcessEvent;
use cid_pal::process_event_sink::ProcessEventSink;
use cid_pal::process_output_stream::ProcessOutputStream;

use crate::persistence::CidStateStore;
use crate::repository::Repository;
use crate::run::Run;
use crate::run_status::RunStatus;

#[derive(Debug, Clone)]
pub struct DockerRunner {
    store: CidStateStore,
    pal: PalHandle,
}

impl DockerRunner {
    pub fn new(store: CidStateStore, pal: PalHandle) -> Self {
        Self { store, pal }
    }

    pub fn execute_queued_runs(
        &self,
        repositories: &[Repository],
        runs: &mut [Run],
    ) -> CidResult<usize> {
        let mut executed = 0;

        for run in runs {
            if run.status() != RunStatus::Queued {
                continue;
            }

            let Some(repository) = repositories
                .iter()
                .find(|repository| repository.id() == run.repository_id())
            else {
                continue;
            };

            self.execute_run(repository, run)?;
            executed += 1;
        }

        Ok(executed)
    }

    fn execute_run(&self, repository: &Repository, run: &mut Run) -> CidResult<()> {
        let started_at_ms = self.now_ms();
        info!(
            run_id = run.id(),
            repository = run.repository_name(),
            branch = run.branch(),
            commit_sha = run.commit_sha(),
            "run started"
        );
        run.start(started_at_ms);
        let run_id = run.id();

        for step_index in 0..run.steps().len() {
            let step = &run.steps()[step_index];
            let command = self.build_command(repository.path(), step.image(), step.command());
            let step_name = step.name().to_string();
            let mut sink = OutputCollector::default();
            let output = self.pal.run_process(&command, &mut sink);
            let started_at_ms = self.now_ms();
            run.steps_mut()[step_index].mark_running(started_at_ms);
            let finished_at_ms = self.now_ms();

            match output {
                Ok(output) => {
                    let mut log_output = String::from_utf8_lossy(&sink.stdout).to_string();
                    if !sink.stderr.is_empty() {
                        if !log_output.is_empty() {
                            log_output.push('\n');
                        }
                        log_output.push_str(&String::from_utf8_lossy(&sink.stderr));
                    }

                    let log_path = self.store.write_step_log(run_id, step_index, &log_output)?;
                    let status = if output.exit_code == Some(0) {
                        RunStatus::Passed
                    } else {
                        RunStatus::Failed
                    };
                    run.steps_mut()[step_index].mark_finished(
                        finished_at_ms,
                        status,
                        output.exit_code,
                        Some(log_path),
                    );
                    run.push_event(
                        finished_at_ms,
                        format!("step `{step_name}` finished with status {}", status.label()),
                    );

                    if status == RunStatus::Failed {
                        info!(
                            run_id = run.id(),
                            repository = run.repository_name(),
                            branch = run.branch(),
                            commit_sha = run.commit_sha(),
                            status = %RunStatus::Failed.label(),
                            "run completed"
                        );
                        run.finish(finished_at_ms, RunStatus::Failed);
                        return Ok(());
                    }
                }
                Err(error) => {
                    let message = format!(
                        "failed to start docker for step `{step_name}`: {}",
                        error.to_test_string()
                    );
                    let log_path = self.store.write_step_log(run_id, step_index, &message)?;
                    run.steps_mut()[step_index].mark_finished(
                        finished_at_ms,
                        RunStatus::Failed,
                        None,
                        Some(log_path),
                    );
                    run.push_event(finished_at_ms, message);
                    info!(
                        run_id = run.id(),
                        repository = run.repository_name(),
                        branch = run.branch(),
                        commit_sha = run.commit_sha(),
                        status = %RunStatus::Failed.label(),
                        "run completed"
                    );
                    run.finish(finished_at_ms, RunStatus::Failed);
                    return Ok(());
                }
            }
        }

        let finished_at_ms = self.now_ms();
        info!(
            run_id = run.id(),
            repository = run.repository_name(),
            branch = run.branch(),
            commit_sha = run.commit_sha(),
            status = %RunStatus::Passed.label(),
            "run completed"
        );
        run.finish(finished_at_ms, RunStatus::Passed);
        Ok(())
    }

    pub fn build_command(
        &self,
        repository_path: &cid_base::file_path::FilePath,
        image: &str,
        command: &str,
    ) -> ProcessCommand {
        ProcessCommand {
            executable: "docker".into(),
            arguments: vec![
                "run".into(),
                "--rm".into(),
                "-v".into(),
                format!("{}:/workspace", repository_path.as_path().display()).into(),
                "-w".into(),
                "/workspace".into(),
                image.into(),
                "sh".into(),
                "-lc".into(),
                command.into(),
            ],
            working_directory: None,
            environment: Vec::new(),
        }
    }

    fn now_ms(&self) -> u64 {
        self.pal
            .system_time()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[derive(Default)]
struct OutputCollector {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl ProcessEventSink for OutputCollector {
    fn handle_event(&mut self, event: ProcessEvent) -> cid_base::result::CidResult<()> {
        if let ProcessEvent::Output(output) = event {
            match output.stream {
                ProcessOutputStream::Stdout => self.stdout.extend(output.bytes),
                ProcessOutputStream::Stderr => self.stderr.extend(output.bytes),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;
    use cid_base::timestamp::Timestamp;
    use cid_pal::pal::PalHandle;
    use cid_pal::pal_mock::PalMock;
    use cid_pal::process_command::ProcessCommand;
    use cid_pal::process_event::ProcessEvent;
    use cid_pal::process_output_event::ProcessOutputEvent;
    use cid_pal::process_output_stream::ProcessOutputStream;
    use cid_pal::process_result::ProcessResult;

    use crate::persistence::CidStateStore;
    use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};
    use crate::run::{Run, RunStep};
    use crate::run_status::RunStatus;

    use super::DockerRunner;

    #[test]
    fn docker_command_uses_workspace_mount_and_shell_execution() {
        let pal = PalMock::new();
        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("/tmp/cid-state"), PalHandle::new(pal.clone())),
            PalHandle::new(pal),
        );

        let command = runner.build_command(&FilePath::new("/repos/cid"), "rust:1.85", "cargo test");

        assert_eq!(command.executable.as_str(), "docker");
        assert_eq!(
            command
                .arguments
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>(),
            &[
                "run",
                "--rm",
                "-v",
                "/repos/cid:/workspace",
                "-w",
                "/workspace",
                "rust:1.85",
                "sh",
                "-lc",
                "cargo test",
            ]
        );
    }

    #[test]
    fn queued_run_is_executed_through_pal_and_logs_are_persisted() {
        let pal = PalMock::new();
        pal.set_current_system_time(std::time::UNIX_EPOCH + std::time::Duration::from_millis(100));
        pal.set_process_execution(
            ProcessCommand {
                executable: "docker".into(),
                arguments: vec![
                    "run".into(),
                    "--rm".into(),
                    "-v".into(),
                    "/repos/cid:/workspace".into(),
                    "-w".into(),
                    "/workspace".into(),
                    "rust:1.85".into(),
                    "sh".into(),
                    "-lc".into(),
                    "cargo test".into(),
                ],
                working_directory: None,
                environment: Vec::new(),
            },
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(1),
                stream: ProcessOutputStream::Stdout,
                bytes: b"ok\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(0),
                finished_at: Timestamp::new(2),
                exit_code: Some(0),
            },
        );

        let repository = Repository::new(
            1,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::new(
                "rust:1.85",
                vec![PipelineStep::new("test", "cargo test")],
                Vec::new(),
            ),
        );
        let mut runs = vec![Run::new(
            1,
            1,
            "cid",
            "main",
            "abc1234",
            100,
            vec![RunStep::new("test", "cargo test", "rust:1.85", Vec::new())],
        )];

        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("state"), PalHandle::new(pal.clone())),
            PalHandle::new(pal.clone()),
        );
        let executed = runner
            .execute_queued_runs(std::slice::from_ref(&repository), &mut runs)
            .unwrap();

        assert_eq!(executed, 1);
        assert_eq!(runs[0].status(), RunStatus::Passed);
        assert_eq!(
            pal.read_file_string("state/logs/run-1/step-0.log")
                .as_deref(),
            Some("ok\n")
        );
    }
}
