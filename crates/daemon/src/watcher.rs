use std::process::Command;

use cid_base::shared_string::SharedString;

use crate::repository::{DiscoveredCommit, Repository};

#[derive(Debug, Default)]
pub struct RepositoryWatcher;

impl RepositoryWatcher {
    pub fn new() -> Self {
        Self
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
                match resolve_branch_head(repository, &branch) {
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
    repository: &Repository,
    branch: &str,
) -> Result<SharedString, SharedString> {
    let output = Command::new("git")
        .current_dir(repository.path().as_path())
        .args(["rev-parse", &format!("refs/heads/{branch}")])
        .output()
        .map_err(|error| {
            SharedString::new(format!(
                "failed to invoke git for `{}`: {error}",
                repository.name()
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SharedString::new(format!(
            "failed to resolve branch `{branch}` for `{}`: {}",
            repository.name(),
            stderr.trim()
        )));
    }

    parse_commit_sha(&output.stdout).map_err(Into::into)
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
    use super::parse_commit_sha;

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
}
