use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cid_base::result::CidResult;

use crate::persistence::CidStateStore;
use crate::repository::Repository;
use crate::run::Run;
use crate::run_status::RunStatus;

#[derive(Debug, Clone)]
pub struct DockerRunner {
    store: CidStateStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerRunCommand {
    program: String,
    args: Vec<String>,
}

impl DockerRunner {
    pub fn new(store: CidStateStore) -> Self {
        Self { store }
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
        run.start(now_ms());
        let run_id = run.id();

        for step_index in 0..run.steps().len() {
            let step = &run.steps()[step_index];
            let command =
                self.build_command(repository.path().as_path(), step.image(), step.command());
            let step_name = step.name().to_string();
            let output = Command::new(command.program())
                .args(command.args())
                .output();
            let started_at_ms = now_ms();
            run.steps_mut()[step_index].mark_running(started_at_ms);
            let finished_at_ms = now_ms();

            match output {
                Ok(output) => {
                    let mut log_output = String::from_utf8_lossy(&output.stdout).to_string();
                    if !output.stderr.is_empty() {
                        if !log_output.is_empty() {
                            log_output.push('\n');
                        }
                        log_output.push_str(&String::from_utf8_lossy(&output.stderr));
                    }

                    let log_path = self.store.write_step_log(run_id, step_index, &log_output)?;
                    let status = if output.status.success() {
                        RunStatus::Passed
                    } else {
                        RunStatus::Failed
                    };
                    run.steps_mut()[step_index].mark_finished(
                        finished_at_ms,
                        status,
                        output.status.code(),
                        Some(log_path),
                    );
                    run.push_event(
                        finished_at_ms,
                        format!("step `{step_name}` finished with status {}", status.label()),
                    );

                    if status == RunStatus::Failed {
                        run.finish(finished_at_ms, RunStatus::Failed);
                        return Ok(());
                    }
                }
                Err(error) => {
                    let message = format!("failed to start docker for step `{step_name}`: {error}");
                    let log_path = self.store.write_step_log(run_id, step_index, &message)?;
                    run.steps_mut()[step_index].mark_finished(
                        finished_at_ms,
                        RunStatus::Failed,
                        None,
                        Some(log_path),
                    );
                    run.push_event(finished_at_ms, message);
                    run.finish(finished_at_ms, RunStatus::Failed);
                    return Ok(());
                }
            }
        }

        run.finish(now_ms(), RunStatus::Passed);
        Ok(())
    }

    pub fn build_command(
        &self,
        repository_path: &Path,
        image: &str,
        command: &str,
    ) -> DockerRunCommand {
        DockerRunCommand {
            program: "docker".to_string(),
            args: vec![
                "run".to_string(),
                "--rm".to_string(),
                "-v".to_string(),
                format!("{}:/workspace", repository_path.display()),
                "-w".to_string(),
                "/workspace".to_string(),
                image.to_string(),
                "sh".to_string(),
                "-lc".to_string(),
                command.to_string(),
            ],
        }
    }
}

impl DockerRunCommand {
    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use cid_base::file_path::FilePath;

    use crate::persistence::CidStateStore;

    use super::DockerRunner;

    #[test]
    fn docker_command_uses_workspace_mount_and_shell_execution() {
        let runner = DockerRunner::new(CidStateStore::new(FilePath::new("/tmp/cid-state")));

        let command = runner.build_command(Path::new("/repos/cid"), "rust:1.85", "cargo test");

        assert_eq!(command.program(), "docker");
        assert_eq!(
            command.args(),
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
}
