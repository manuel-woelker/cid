use cid_base::shared_string::SharedString;
use cid_pal::pal::PalHandle;
use cid_pal::process_command::ProcessCommand;
use cid_pal::process_event::ProcessEvent;
use cid_pal::process_event_sink::ProcessEventSink;
use cid_pal::process_output_stream::ProcessOutputStream;

use crate::repository::{DiscoveredCommit, Repository};

#[derive(Debug, Clone)]
pub struct RepositoryWatcher {
    pal: PalHandle,
}

impl RepositoryWatcher {
    pub fn new(pal: PalHandle) -> Self {
        Self { pal }
    }

    pub fn poll(
        &self,
        repositories: &mut [Repository],
        existing_commits: &[DiscoveredCommit],
        now_ms: u64,
    ) -> Vec<DiscoveredCommit> {
        let mut discoveries = Vec::new();

        for repository in repositories {
            let mut repository_had_error = false;
            let branches: Vec<String> = repository
                .branch_rules()
                .iter()
                .map(|branch_rule| branch_rule.branch().to_string())
                .collect();

            for branch in branches {
                match resolve_branch_head(&self.pal, repository, &branch) {
                    Ok(commit_sha) => {
                        repository.mark_seen(now_ms);
                        if !existing_commits.iter().any(|existing| {
                            existing.repository_id() == repository.id()
                                && existing.branch() == branch
                                && existing.commit_sha() == commit_sha
                        }) {
                            discoveries.push(DiscoveredCommit::new(
                                repository.id(),
                                repository.name(),
                                branch,
                                commit_sha,
                                now_ms,
                            ));
                        }
                    }
                    Err(error) => {
                        repository_had_error = true;
                        repository.mark_error(error);
                    }
                }
            }

            if !repository_had_error {
                repository.mark_seen(now_ms);
            }
        }

        discoveries
    }
}

fn resolve_branch_head(
    pal: &PalHandle,
    repository: &Repository,
    branch: &str,
) -> Result<SharedString, SharedString> {
    let command = ProcessCommand {
        executable: "git".into(),
        arguments: vec!["rev-parse".into(), format!("refs/heads/{branch}").into()],
        working_directory: Some(repository.path().clone()),
        environment: Vec::new(),
    };
    let mut sink = OutputCollector::default();
    let result = pal.run_process(&command, &mut sink).map_err(|error| {
        SharedString::new(format!(
            "failed to invoke git for `{}`: {}",
            repository.name(),
            error.to_test_string()
        ))
    })?;

    if result.exit_code != Some(0) {
        let stderr = String::from_utf8_lossy(&sink.stderr);
        return Err(SharedString::new(format!(
            "failed to resolve branch `{branch}` for `{}`: {}",
            repository.name(),
            stderr.trim()
        )));
    }

    parse_commit_sha(&sink.stdout).map_err(Into::into)
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

fn parse_commit_sha(output: &[u8]) -> Result<SharedString, String> {
    let commit = String::from_utf8_lossy(output).trim().to_string();
    if commit.len() < 7 {
        return Err("git returned an invalid commit SHA".to_string());
    }
    Ok(commit.into())
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

    use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};

    use super::{RepositoryWatcher, parse_commit_sha};

    fn repository() -> Repository {
        Repository::new(
            1,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::new(
                "rust:1.85",
                vec![PipelineStep::new("test", "cargo test")],
                Vec::new(),
            ),
        )
    }

    #[test]
    fn parse_commit_sha_trims_newlines() {
        let commit = parse_commit_sha(b"abc1234\n").unwrap();
        assert_eq!(commit.as_str(), "abc1234");
    }

    #[test]
    fn parse_commit_sha_rejects_short_values() {
        let error = parse_commit_sha(b"bad\n").unwrap_err();
        assert!(error.contains("invalid commit SHA"));
    }

    #[test]
    fn watcher_discovers_new_commit_from_pal_process_output() {
        let pal = PalMock::new();
        pal.set_process_execution(
            ProcessCommand {
                executable: "git".into(),
                arguments: vec!["rev-parse".into(), "refs/heads/main".into()],
                working_directory: Some(FilePath::new("/repos/cid")),
                environment: Vec::new(),
            },
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(1),
                stream: ProcessOutputStream::Stdout,
                bytes: b"abc1234\n".to_vec(),
            })],
            ProcessResult {
                started_at: Timestamp::new(0),
                finished_at: Timestamp::new(2),
                exit_code: Some(0),
            },
        );

        let watcher = RepositoryWatcher::new(PalHandle::new(pal));
        let mut repositories = vec![repository()];

        let discoveries = watcher.poll(&mut repositories, &[], 42);

        assert_eq!(discoveries.len(), 1);
        assert_eq!(discoveries[0].commit_sha(), "abc1234");
        assert_eq!(repositories[0].status().last_seen_at_ms(), Some(42));
    }
}
