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

    pub fn execute_claimed_run(&self, repository: &Repository, mut run: Run) -> Run {
        if let Err(error) = self.execute_run(repository, &mut run) {
            self.mark_run_failed(repository, &mut run, &error.to_test_string());
        }

        run
    }

    fn execute_run(&self, repository: &Repository, run: &mut Run) -> CidResult<()> {
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

            let (output, log_output) =
                if repository.pipeline().image() == "devcontainer" && step_name == "ci" {
                    let id_labels = self.devcontainer_id_labels(repository, &fingerprint);
                    self.execute_devcontainer_ci_step(&repository_path, &id_labels)?
                } else {
                    let command = self.build_command(&repository_path, step_name.as_str())?;
                    let mut sink = OutputCollector::default();
                    let output = self.pal.run_process(&command, &mut sink);
                    (output, format_process_output(&sink))
                };
            let finished_at_ms = self.now_ms();

            match output {
                Ok(output) => {
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

    fn mark_run_failed(&self, repository: &Repository, run: &mut Run, message: &str) {
        let finished_at_ms = self.now_ms();
        let failed_step_index = run
            .steps()
            .iter()
            .enumerate()
            .find_map(|(index, step)| (!step.status().is_finished()).then_some(index));

        if let Some(step_index) = failed_step_index {
            let log_path = self
                .store
                .write_step_log(repository, run.id(), step_index, message)
                .ok();
            run.steps_mut()[step_index].mark_finished(
                finished_at_ms,
                RunStatus::Failed,
                None,
                log_path,
            );
        }

        run.push_event(finished_at_ms, message.to_string());
        run.finish(finished_at_ms, RunStatus::Failed);
    }

    pub fn build_command(
        &self,
        _repository_path: &cid_base::file_path::FilePath,
        step_name: &str,
    ) -> CidResult<ProcessCommand> {
        match step_name {
            "ci" => Err(cid_base::err!(
                "devcontainer ci commands require repository identity"
            )),
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
        id_labels: &[String],
    ) -> ProcessCommand {
        let config_path = repository_path
            .join(".devcontainer")
            .join("devcontainer.json");
        let mut arguments = vec![
            "exec".into(),
            "--workspace-folder".into(),
            repository_path.as_str().into(),
            "--config".into(),
            config_path.as_str().into(),
        ];
        for id_label in id_labels {
            arguments.push("--id-label".into());
            arguments.push(id_label.clone().into());
        }
        arguments.extend(vec![
            "/bin/sh".into(),
            "-lc".into(),
            "./scripts/ci.sh".into(),
        ]);
        ProcessCommand {
            executable: "devcontainer".into(),
            arguments,
            working_directory: Some(repository_path.clone()),
            environment: Vec::new(),
        }
    }

    pub fn build_devcontainer_up_command(
        &self,
        repository_path: &cid_base::file_path::FilePath,
        id_labels: &[String],
    ) -> ProcessCommand {
        let config_path = repository_path
            .join(".devcontainer")
            .join("devcontainer.json");
        let mut arguments = vec![
            "up".into(),
            "--workspace-folder".into(),
            repository_path.as_str().into(),
            "--config".into(),
            config_path.as_str().into(),
        ];
        for id_label in id_labels {
            arguments.push("--id-label".into());
            arguments.push(id_label.clone().into());
        }
        ProcessCommand {
            executable: "devcontainer".into(),
            arguments,
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

    fn devcontainer_id_labels(&self, repository: &Repository, fingerprint: &str) -> Vec<String> {
        vec![
            format!("cid.repository={}", slugify(repository.name())),
            format!("cid.devcontainer-fingerprint={fingerprint}"),
        ]
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

    fn execute_devcontainer_ci_step(
        &self,
        repository_path: &cid_base::file_path::FilePath,
        id_labels: &[String],
    ) -> CidResult<(CidResult<cid_pal::process_result::ProcessResult>, String)> {
        let exec_command = self.build_ci_exec_command(repository_path, id_labels);
        let mut exec_sink = OutputCollector::default();
        let exec_output = self.pal.run_process(&exec_command, &mut exec_sink);
        let exec_log_output = format_process_output(&exec_sink);

        if should_retry_devcontainer_exec(&exec_output, &exec_log_output) {
            let up_command = self.build_devcontainer_up_command(repository_path, id_labels);
            let mut up_sink = OutputCollector::default();
            let up_output = self.pal.run_process(&up_command, &mut up_sink);
            let up_log_output = format_process_output(&up_sink);

            match up_output {
                Ok(up_output) if up_output.exit_code == Some(0) => {
                    let mut retry_exec_sink = OutputCollector::default();
                    let retry_exec_output =
                        self.pal.run_process(&exec_command, &mut retry_exec_sink);
                    let retry_exec_log_output = format_process_output(&retry_exec_sink);
                    let combined_log_output =
                        join_log_output(&up_log_output, &retry_exec_log_output);
                    Ok((retry_exec_output, combined_log_output))
                }
                Ok(up_output) => Ok((Ok(up_output), up_log_output)),
                Err(error) => Ok((Err(error), up_log_output)),
            }
        } else {
            Ok((exec_output, exec_log_output))
        }
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

fn join_log_output(first: &str, second: &str) -> String {
    match (first.is_empty(), second.is_empty()) {
        (true, true) => String::new(),
        (false, true) => first.to_string(),
        (true, false) => second.to_string(),
        (false, false) => format!("{first}\n{second}"),
    }
}

fn should_retry_devcontainer_exec(
    output: &CidResult<cid_pal::process_result::ProcessResult>,
    log_output: &str,
) -> bool {
    if output.is_err() {
        return true;
    }

    let Some(result) = output.as_ref().ok() else {
        return false;
    };

    if result.exit_code == Some(0) {
        return false;
    }

    let lower = log_output.to_ascii_lowercase();
    let stale_runtime_markers = [
        "container is not running",
        "container is stopped",
        "container state improper",
        "no such container",
        "container does not exist",
        "shell server terminated",
        "not found",
    ];

    stale_runtime_markers
        .iter()
        .any(|marker| lower.contains(marker))
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
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use cid_base::file_path::FilePath;
    use cid_base::result::CidResult;
    use cid_base::timestamp::Timestamp;
    use cid_pal::pal::{Pal, PalHandle, ReadSeek};
    use cid_pal::pal_mock::PalMock;
    use cid_pal::process_command::ProcessCommand;
    use cid_pal::process_event::ProcessEvent;
    use cid_pal::process_event_sink::ProcessEventSink;
    use cid_pal::process_output_event::ProcessOutputEvent;
    use cid_pal::process_output_stream::ProcessOutputStream;
    use cid_pal::process_result::ProcessResult;

    use crate::persistence::CidStateStore;
    use crate::repository::{BranchRule, Pipeline, Repository};
    use crate::run::{Run, RunStep};
    use crate::run_status::RunStatus;

    use super::{DockerRunner, resolve_repository_path};

    type QueuedProcessExecutions =
        Arc<Mutex<HashMap<ProcessCommand, VecDeque<(Vec<ProcessEvent>, ProcessResult)>>>>;

    #[derive(Clone, Debug)]
    struct SequencedPal {
        inner: PalMock,
        queued_executions: QueuedProcessExecutions,
    }

    impl SequencedPal {
        fn new(inner: PalMock) -> Self {
            Self {
                inner,
                queued_executions: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn set_file(&self, path: &str, content: impl Into<Vec<u8>>) {
            self.inner.set_file(path, content);
        }

        fn set_process_execution(
            &self,
            command: ProcessCommand,
            events: Vec<ProcessEvent>,
            result: ProcessResult,
        ) {
            self.inner.set_process_execution(command, events, result);
        }

        fn push_process_execution(
            &self,
            command: ProcessCommand,
            events: Vec<ProcessEvent>,
            result: ProcessResult,
        ) {
            self.queued_executions
                .lock()
                .unwrap()
                .entry(command)
                .or_default()
                .push_back((events, result));
        }

        fn set_current_system_time(&self, system_time: SystemTime) {
            self.inner.set_current_system_time(system_time);
        }
    }

    impl Pal for SequencedPal {
        fn file_exists(&self, path: &FilePath) -> CidResult<bool> {
            self.inner.file_exists(path)
        }

        fn directory_exists(&self, path: &FilePath) -> CidResult<bool> {
            self.inner.directory_exists(path)
        }

        fn read_file(&self, path: &FilePath) -> CidResult<Box<dyn ReadSeek + 'static>> {
            self.inner.read_file(path)
        }

        fn create_directory_all(&self, path: &FilePath) -> CidResult<()> {
            self.inner.create_directory_all(path)
        }

        fn write_file(&self, path: &FilePath, content: &[u8]) -> CidResult<()> {
            self.inner.write_file(path, content)
        }

        fn run_process(
            &self,
            command: &ProcessCommand,
            sink: &mut dyn ProcessEventSink,
        ) -> CidResult<ProcessResult> {
            if let Some((events, result)) = self
                .queued_executions
                .lock()
                .unwrap()
                .get_mut(command)
                .and_then(VecDeque::pop_front)
            {
                for event in events {
                    sink.handle_event(event)?;
                }
                return Ok(result);
            }

            self.inner.run_process(command, sink)
        }

        fn now(&self) -> Timestamp {
            self.inner.now()
        }

        fn system_time(&self) -> SystemTime {
            self.inner.system_time()
        }

        fn sleep(&self, duration: std::time::Duration) {
            self.inner.sleep(duration)
        }
    }

    #[test]
    fn build_command_uses_devcontainer_exec_for_ci_step() {
        let pal = PalMock::new();
        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("/tmp/cid-state")),
            PalHandle::new(pal),
        );
        let repository = Repository::new(
            1,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::for_devcontainer(Vec::new()),
        );
        let id_labels = runner.devcontainer_id_labels(&repository, "abc123");

        let command = runner.build_ci_exec_command(&FilePath::new("/repos/cid"), &id_labels);

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
                "/repos/cid",
                "--config",
                "/repos/cid/.devcontainer/devcontainer.json",
                "--id-label",
                "cid.repository=cid",
                "--id-label",
                "cid.devcontainer-fingerprint=abc123",
                "/bin/sh",
                "-lc",
                "./scripts/ci.sh",
            ]
        );
    }

    #[test]
    fn build_devcontainer_up_command_uses_devcontainer_up_without_forced_recreation() {
        let pal = PalMock::new();
        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("/tmp/cid-state")),
            PalHandle::new(pal),
        );
        let repository = Repository::new(
            1,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::for_devcontainer(Vec::new()),
        );
        let id_labels = runner.devcontainer_id_labels(&repository, "abc123");

        let command =
            runner.build_devcontainer_up_command(&FilePath::new("/repos/cid"), &id_labels);

        assert_eq!(command.executable.as_str(), "devcontainer");
        assert_eq!(
            command
                .arguments
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>(),
            vec![
                "up",
                "--workspace-folder",
                "/repos/cid",
                "--config",
                "/repos/cid/.devcontainer/devcontainer.json",
                "--id-label",
                "cid.repository=cid",
                "--id-label",
                "cid.devcontainer-fingerprint=abc123",
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
        let repository = Repository::new(
            1,
            "cid-rust-sandbox",
            FilePath::new("sandboxes/cid-rust-sandbox"),
            vec![BranchRule::new("main")],
            Pipeline::for_devcontainer(Vec::new()),
        );
        let id_labels = runner.devcontainer_id_labels(&repository, "abc123");
        let command = runner.build_ci_exec_command(&repository_path, &id_labels);

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
                "--id-label",
                "cid.repository=cid-rust-sandbox",
                "--id-label",
                "cid.devcontainer-fingerprint=abc123",
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
                executable: "devcontainer".into(),
                arguments: vec![
                    "exec".into(),
                    "--workspace-folder".into(),
                    "/repos/cid".into(),
                    "--config".into(),
                    "/repos/cid/.devcontainer/devcontainer.json".into(),
                    "--id-label".into(),
                    "cid.repository=cid".into(),
                    "--id-label".into(),
                    format!("cid.devcontainer-fingerprint={fingerprint}").into(),
                    "/bin/sh".into(),
                    "-lc".into(),
                    "./scripts/ci.sh".into(),
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

    #[test]
    fn queued_run_recovers_with_devcontainer_up_when_exec_hits_stale_container() {
        let pal = SequencedPal::new(PalMock::new());
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
        let state_dir = temp_state_dir("runner-state-recover");

        let runner = DockerRunner::new(
            CidStateStore::new(state_dir.clone()),
            PalHandle::new(pal.clone()),
        );
        let fingerprint = runner.devcontainer_fingerprint(&repository_path).unwrap();
        let image_tag = runner.image_tag(&repository, &fingerprint);
        let id_labels = runner.devcontainer_id_labels(&repository, &fingerprint);

        pal.set_process_execution(
            runner.build_devcontainer_build_command(&repository_path, &image_tag),
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
        pal.push_process_execution(
            runner.build_ci_exec_command(&repository_path, &id_labels),
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(5),
                stream: ProcessOutputStream::Stderr,
                bytes: b"container is not running\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(4),
                finished_at: Timestamp::new(6),
                exit_code: Some(1),
            },
        );
        pal.set_process_execution(
            runner.build_devcontainer_up_command(&repository_path, &id_labels),
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(7),
                stream: ProcessOutputStream::Stdout,
                bytes: b"started\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(7),
                finished_at: Timestamp::new(8),
                exit_code: Some(0),
            },
        );
        pal.push_process_execution(
            runner.build_ci_exec_command(&repository_path, &id_labels),
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(9),
                stream: ProcessOutputStream::Stdout,
                bytes: b"ok\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(9),
                finished_at: Timestamp::new(10),
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
            "started\n\nok\n"
        );
        cleanup(&state_dir);
    }

    #[test]
    fn queued_run_recovers_when_exec_reports_improper_container_state() {
        let pal = SequencedPal::new(PalMock::new());
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
        let state_dir = temp_state_dir("runner-state-improper");

        let runner = DockerRunner::new(
            CidStateStore::new(state_dir.clone()),
            PalHandle::new(pal.clone()),
        );
        let fingerprint = runner.devcontainer_fingerprint(&repository_path).unwrap();
        let image_tag = runner.image_tag(&repository, &fingerprint);
        let id_labels = runner.devcontainer_id_labels(&repository, &fingerprint);

        pal.set_process_execution(
            runner.build_devcontainer_build_command(&repository_path, &image_tag),
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
        pal.push_process_execution(
            runner.build_ci_exec_command(&repository_path, &id_labels),
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(5),
                stream: ProcessOutputStream::Stderr,
                bytes: b"Shell server terminated (code: 255, signal: null)\n\nError: can only create exec sessions on running containers: container state improper\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(4),
                finished_at: Timestamp::new(6),
                exit_code: Some(255),
            },
        );
        pal.set_process_execution(
            runner.build_devcontainer_up_command(&repository_path, &id_labels),
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(7),
                stream: ProcessOutputStream::Stdout,
                bytes: b"started\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(7),
                finished_at: Timestamp::new(8),
                exit_code: Some(0),
            },
        );
        pal.push_process_execution(
            runner.build_ci_exec_command(&repository_path, &id_labels),
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(9),
                stream: ProcessOutputStream::Stdout,
                bytes: b"ok\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(9),
                finished_at: Timestamp::new(10),
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
        cleanup(&state_dir);
    }

    #[test]
    fn devcontainer_id_labels_include_repository_and_fingerprint() {
        let pal = PalMock::new();
        let runner = DockerRunner::new(
            CidStateStore::new(FilePath::new("/tmp/cid-state")),
            PalHandle::new(pal),
        );
        let repository = Repository::new(
            1,
            "cid rust sandbox",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::for_devcontainer(Vec::new()),
        );

        let id_labels = runner.devcontainer_id_labels(&repository, "abc123");

        assert_eq!(
            id_labels,
            vec![
                "cid.repository=cid-rust-sandbox".to_string(),
                "cid.devcontainer-fingerprint=abc123".to_string(),
            ]
        );
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
