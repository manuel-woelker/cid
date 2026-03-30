use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use cid_base::logging::info;
use cid_base::result::{CidResult, ResultExt};
use cid_pal::pal::PalHandle;
use cid_pal::process_command::ProcessCommand;
use cid_pal::process_event::ProcessEvent;
use cid_pal::process_event_sink::ProcessEventSink;
use cid_pal::process_output_stream::ProcessOutputStream;
use serde::{Deserialize, Serialize};

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

    pub fn execute_next_queued_run(
        &self,
        repositories: &[Repository],
        runs: &mut [Run],
    ) -> CidResult<bool> {
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
            return Ok(true);
        }

        Ok(false)
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

        let repository_path = resolve_repository_path(repository.path())?;
        let fingerprint = self.devcontainer_fingerprint(&repository_path)?;
        let image_tag = self.image_tag(repository, &fingerprint);

        for step_index in 0..run.steps().len() {
            let step = &run.steps()[step_index];
            let step_name = step.name().to_string();
            let started_at_ms = self.now_ms();
            run.steps_mut()[step_index].mark_running(started_at_ms);

            if repository.pipeline().image() == "devcontainer" {
                if let Some((exit_code, log_output)) = self.ensure_devcontainer_built(
                    &repository_path,
                    repository,
                    &fingerprint,
                    &image_tag,
                )? {
                    let finished_at_ms = self.now_ms();
                    let log_path =
                        self.store
                            .write_step_log(repository, run_id, step_index, &log_output)?;
                    run.steps_mut()[step_index].mark_finished(
                        finished_at_ms,
                        RunStatus::Failed,
                        exit_code,
                        Some(log_path),
                    );
                    run.push_event(
                        finished_at_ms,
                        format!("step `{step_name}` finished with status failed"),
                    );
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

            let command = self.build_command(&repository_path, step_name.as_str())?;
            let mut sink = OutputCollector::default();
            let output = self.pal.run_process(&command, &mut sink);
            let finished_at_ms = self.now_ms();

            match output {
                Ok(output) => {
                    let log_output = format_process_output(&sink);

                    let log_path =
                        self.store
                            .write_step_log(repository, run_id, step_index, &log_output)?;
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
                        "failed to start devcontainer execution for step `{step_name}`: {}",
                        error.to_test_string()
                    );
                    let log_path = self
                        .store
                        .write_step_log(repository, run_id, step_index, &message)?;
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
        step_name: &str,
    ) -> CidResult<ProcessCommand> {
        let config_path = repository_path
            .join(".devcontainer")
            .join("devcontainer.json");

        match step_name {
            "ci" => Ok(ProcessCommand {
                executable: "sh".into(),
                arguments: vec![
                    "-lc".into(),
                    format!(
                        "devcontainer up --workspace-folder '{}' --config '{}' --remove-existing-container >/dev/null 2>&1 && devcontainer exec --workspace-folder '{}' --config '{}' /bin/sh -lc './scripts/ci.sh'",
                        repository_path.as_str(),
                        config_path,
                        repository_path.as_str(),
                        config_path,
                    )
                    .into(),
                ],
                working_directory: Some(repository_path.clone()),
                environment: Vec::new(),
            }),
            _ => Err(cid_base::err!("unsupported run step `{step_name}`")),
        }
    }

    pub fn build_devcontainer_build_command(
        &self,
        repository_path: &cid_base::file_path::FilePath,
        image_tag: &str,
    ) -> ProcessCommand {
        let config_path = repository_path
            .join(".devcontainer")
            .join("devcontainer.json");
        ProcessCommand {
            executable: "devcontainer".into(),
            arguments: vec![
                "build".into(),
                "--workspace-folder".into(),
                repository_path.as_str().into(),
                "--config".into(),
                config_path.as_str().into(),
                "--image-name".into(),
                image_tag.into(),
            ],
            working_directory: Some(repository_path.clone()),
            environment: Vec::new(),
        }
    }

    pub fn build_ci_exec_command(
        &self,
        repository_path: &cid_base::file_path::FilePath,
    ) -> ProcessCommand {
        let config_path = repository_path
            .join(".devcontainer")
            .join("devcontainer.json");
        ProcessCommand {
            executable: "devcontainer".into(),
            arguments: vec![
                "exec".into(),
                "--workspace-folder".into(),
                repository_path.as_str().into(),
                "--config".into(),
                config_path.as_str().into(),
                "/bin/sh".into(),
                "-lc".into(),
                "./scripts/ci.sh".into(),
            ],
            working_directory: Some(repository_path.clone()),
            environment: Vec::new(),
        }
    }

    fn ensure_devcontainer_built(
        &self,
        repository_path: &cid_base::file_path::FilePath,
        repository: &Repository,
        fingerprint: &str,
        image_tag: &str,
    ) -> CidResult<Option<(Option<i32>, String)>> {
        if self
            .read_devcontainer_build_metadata(repository)?
            .as_ref()
            .is_some_and(|metadata| metadata.fingerprint == fingerprint)
        {
            return Ok(None);
        }

        let command = self.build_devcontainer_build_command(repository_path, image_tag);
        let mut sink = OutputCollector::default();
        let output = self.pal.run_process(&command, &mut sink);

        match output {
            Ok(output) => {
                if output.exit_code == Some(0) {
                    self.write_devcontainer_build_metadata(repository, fingerprint, image_tag)?;
                    Ok(None)
                } else {
                    Ok(Some((output.exit_code, format_process_output(&sink))))
                }
            }
            Err(error) => Ok(Some((
                None,
                format!(
                    "failed to start devcontainer build: {}",
                    error.to_test_string()
                ),
            ))),
        }
    }

    fn devcontainer_fingerprint(
        &self,
        repository_path: &cid_base::file_path::FilePath,
    ) -> CidResult<String> {
        let devcontainer_path = repository_path
            .join(".devcontainer")
            .join("devcontainer.json");
        let devcontainer_contents = self
            .pal
            .read_file_to_string(&devcontainer_path)
            .with_context(|| format!("failed to read devcontainer config `{devcontainer_path}`"))?;
        let config: DevcontainerConfig = serde_json::from_str(devcontainer_contents.as_str())
            .with_context(|| {
                format!("failed to parse devcontainer config `{devcontainer_path}`")
            })?;

        let mut hasher = DefaultHasher::new();
        devcontainer_contents.hash(&mut hasher);

        if let Some(dockerfile_path) = config.dockerfile_path(repository_path) {
            let dockerfile_contents = self
                .pal
                .read_file_to_string(&dockerfile_path)
                .with_context(|| {
                    format!("failed to read devcontainer dockerfile `{dockerfile_path}`")
                })?;
            dockerfile_contents.hash(&mut hasher);
        }

        Ok(format!("{:016x}", hasher.finish()))
    }

    fn image_tag(&self, repository: &Repository, fingerprint: &str) -> String {
        let slug = slugify(repository.name());
        let short = &fingerprint[..12.min(fingerprint.len())];
        format!("cid-devcontainer-{slug}:{short}")
    }

    fn read_devcontainer_build_metadata(
        &self,
        repository: &Repository,
    ) -> CidResult<Option<DevcontainerBuildMetadata>> {
        let path = self.devcontainer_build_metadata_path(repository);
        if !self.pal.file_exists(&path)? {
            return Ok(None);
        }

        let contents = self
            .pal
            .read_file_to_string(&path)
            .with_context(|| format!("failed to read devcontainer build metadata `{path}`"))?;
        let metadata = serde_json::from_str(contents.as_str())
            .with_context(|| format!("failed to parse devcontainer build metadata `{path}`"))?;
        Ok(Some(metadata))
    }

    fn write_devcontainer_build_metadata(
        &self,
        repository: &Repository,
        fingerprint: &str,
        image_tag: &str,
    ) -> CidResult<()> {
        let path = self.devcontainer_build_metadata_path(repository);
        if let Some(parent) = path.parent() {
            self.pal.create_directory_all(&parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&DevcontainerBuildMetadata {
            fingerprint: fingerprint.to_string(),
            image_tag: image_tag.to_string(),
        })
        .context("failed to serialize devcontainer build metadata")?;
        self.pal.write_file(&path, &bytes)
    }

    fn devcontainer_build_metadata_path(
        &self,
        repository: &Repository,
    ) -> cid_base::file_path::FilePath {
        self.store
            .state_dir()
            .join("devcontainer-cache")
            .join(format!(
                "{}.json",
                slugify(&format!("{}-{}", repository.name(), repository.path()))
            ))
    }

    fn now_ms(&self) -> u64 {
        self.pal
            .system_time()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DevcontainerBuildMetadata {
    fingerprint: String,
    image_tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct DevcontainerConfig {
    #[serde(default)]
    build: Option<DevcontainerBuildConfig>,
    #[serde(default, rename = "dockerFile")]
    docker_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct DevcontainerBuildConfig {
    #[serde(default)]
    dockerfile: Option<String>,
    #[serde(default, rename = "dockerFile")]
    docker_file: Option<String>,
}

impl DevcontainerConfig {
    fn dockerfile_path(
        &self,
        repository_path: &cid_base::file_path::FilePath,
    ) -> Option<cid_base::file_path::FilePath> {
        let dockerfile = self
            .build
            .as_ref()
            .and_then(|build| build.dockerfile.as_ref().or(build.docker_file.as_ref()))
            .or(self.docker_file.as_ref())?;
        Some(repository_path.join(".devcontainer").join(dockerfile))
    }
}

fn resolve_repository_path(
    repository_path: &cid_base::file_path::FilePath,
) -> CidResult<cid_base::file_path::FilePath> {
    if repository_path.is_absolute() {
        return Ok(repository_path.clone());
    }

    let current_dir = std::env::current_dir()?;
    Ok(cid_base::file_path::FilePath::new(
        current_dir.join(repository_path.as_path()),
    ))
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn format_process_output(sink: &OutputCollector) -> String {
    let mut output = String::from_utf8_lossy(&sink.stdout).to_string();
    if !sink.stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&String::from_utf8_lossy(&sink.stderr));
    }
    output
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
    use crate::repository::{BranchRule, Pipeline, Repository};
    use crate::run::{Run, RunStep};
    use crate::run_status::RunStatus;

    use super::{DockerRunner, resolve_repository_path};

    #[test]
    fn build_command_uses_devcontainer_exec_for_ci_step() {
        let pal = PalMock::new();
        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("/tmp/cid-state")),
            PalHandle::new(pal),
        );

        let command = runner
            .build_command(&FilePath::new("/repos/cid"), "ci")
            .unwrap();

        assert_eq!(command.executable.as_str(), "sh");
        assert_eq!(
            command
                .arguments
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>(),
            vec![
                "-lc",
                "devcontainer up --workspace-folder '/repos/cid' --config '/repos/cid/.devcontainer/devcontainer.json' --remove-existing-container >/dev/null 2>&1 && devcontainer exec --workspace-folder '/repos/cid' --config '/repos/cid/.devcontainer/devcontainer.json' /bin/sh -lc './scripts/ci.sh'",
            ]
        );
    }

    #[test]
    fn build_devcontainer_build_command_uses_devcontainer_build() {
        let pal = PalMock::new();
        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("/tmp/cid-state")),
            PalHandle::new(pal),
        );

        let command = runner.build_devcontainer_build_command(
            &FilePath::new("/repos/cid"),
            "cid-devcontainer-cid:abc123",
        );

        assert_eq!(command.executable.as_str(), "devcontainer");
        assert_eq!(
            command
                .arguments
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>(),
            &[
                "build",
                "--workspace-folder",
                "/repos/cid",
                "--config",
                "/repos/cid/.devcontainer/devcontainer.json",
                "--image-name",
                "cid-devcontainer-cid:abc123",
            ]
        );
    }

    #[test]
    fn ensure_devcontainer_built_skips_build_when_fingerprint_matches_cached_metadata() {
        let pal = PalMock::new();
        pal.set_file(
            "/tmp/cid-state/devcontainer-cache/cid-sandboxes-cid-rust-sandbox.json",
            r#"{
  "fingerprint": "abc123",
  "image_tag": "cid-devcontainer-cid:abc123"
}"#,
        );
        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("/tmp/cid-state")),
            PalHandle::new(pal),
        );
        let repository = Repository::new(
            1,
            "cid",
            FilePath::new("sandboxes/cid-rust-sandbox"),
            vec![BranchRule::new("main")],
            Pipeline::for_devcontainer(Vec::new()),
        );

        let result = runner
            .ensure_devcontainer_built(
                &FilePath::new("sandboxes/cid-rust-sandbox"),
                &repository,
                "abc123",
                "cid-devcontainer-cid:abc123",
            )
            .unwrap();

        assert_eq!(result, None);
    }

    #[test]
    fn ci_exec_command_uses_devcontainer_exec() {
        let pal = PalMock::new();
        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("/tmp/cid-state")),
            PalHandle::new(pal),
        );

        let repository_path =
            resolve_repository_path(&FilePath::new("sandboxes/cid-rust-sandbox")).unwrap();
        let command = runner.build_ci_exec_command(&repository_path);

        assert_eq!(command.executable.as_str(), "devcontainer");
        assert_eq!(
            command
                .arguments
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>(),
            vec![
                "exec",
                "--workspace-folder",
                repository_path.as_str(),
                "--config",
                repository_path
                    .join(".devcontainer")
                    .join("devcontainer.json")
                    .as_str(),
                "/bin/sh",
                "-lc",
                "./scripts/ci.sh",
            ]
        );
    }

    #[test]
    fn queued_run_is_executed_through_devcontainer_cli_and_logs_are_persisted() {
        let pal = PalMock::new();
        pal.set_current_system_time(std::time::UNIX_EPOCH + std::time::Duration::from_millis(100));
        let repository_path = FilePath::new("/repos/cid");
        pal.set_file(
            "/repos/cid/.devcontainer/devcontainer.json",
            "{\"image\":\"rust:1.85\"}",
        );
        let repository = Repository::new(
            1,
            "cid",
            repository_path.clone(),
            vec![BranchRule::new("main")],
            Pipeline::for_devcontainer(Vec::new()),
        );
        let state_dir = temp_state_dir("runner-state");

        let runner = DockerRunner::new(
            CidStateStore::new(state_dir.clone()),
            PalHandle::new(pal.clone()),
        );
        let fingerprint = runner.devcontainer_fingerprint(&repository_path).unwrap();
        let image_tag = runner.image_tag(&repository, &fingerprint);

        pal.set_process_execution(
            ProcessCommand {
                executable: "devcontainer".into(),
                arguments: vec![
                    "build".into(),
                    "--workspace-folder".into(),
                    "/repos/cid".into(),
                    "--config".into(),
                    "/repos/cid/.devcontainer/devcontainer.json".into(),
                    "--image-name".into(),
                    image_tag.clone().into(),
                ],
                working_directory: Some(repository_path.clone()),
                environment: Vec::new(),
            },
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(1),
                stream: ProcessOutputStream::Stdout,
                bytes: b"built\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(0),
                finished_at: Timestamp::new(2),
                exit_code: Some(0),
            },
        );
        pal.set_process_execution(
            ProcessCommand {
                executable: "sh".into(),
                arguments: vec![
                    "-lc".into(),
                    "devcontainer up --workspace-folder '/repos/cid' --config '/repos/cid/.devcontainer/devcontainer.json' --remove-existing-container >/dev/null 2>&1 && devcontainer exec --workspace-folder '/repos/cid' --config '/repos/cid/.devcontainer/devcontainer.json' /bin/sh -lc './scripts/ci.sh'".into(),
                ],
                working_directory: Some(repository_path.clone()),
                environment: Vec::new(),
            },
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(5),
                stream: ProcessOutputStream::Stdout,
                bytes: b"ok\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(4),
                finished_at: Timestamp::new(6),
                exit_code: Some(0),
            },
        );

        let mut runs = vec![Run::new(
            1,
            1,
            "cid",
            "main",
            "abc1234",
            100,
            vec![RunStep::new(
                "ci",
                "./scripts/ci.sh",
                "devcontainer",
                Vec::new(),
            )],
        )];
        let executed = runner
            .execute_queued_runs(std::slice::from_ref(&repository), &mut runs)
            .unwrap();

        assert_eq!(executed, 1);
        assert_eq!(runs[0].status(), RunStatus::Passed);
        let ci_log_path = runs[0].steps()[0].log_path().unwrap();
        assert_eq!(
            std::fs::read_to_string(ci_log_path.as_path()).unwrap(),
            "ok\n"
        );
        cleanup(&state_dir);
    }

    fn temp_state_dir(prefix: &str) -> FilePath {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        FilePath::new(std::env::temp_dir().join(format!("cid-{prefix}-{unique}")))
    }

    fn cleanup(path: &FilePath) {
        let _ = std::fs::remove_dir_all(path.as_path());
    }
}
