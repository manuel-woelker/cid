use crate::repository::{DiscoveredCommit, Repository};
use crate::run::{Run, RunStep};
use crate::run_status::RunStatus;

#[derive(Debug, Default)]
pub struct Scheduler;

impl Scheduler {
    pub fn new() -> Self {
        Self
    }

    pub fn enqueue_runs(
        &self,
        repositories: &[Repository],
        discovered_commits: &[DiscoveredCommit],
        runs: &mut Vec<Run>,
    ) -> usize {
        let mut queued = 0;

        for commit in discovered_commits {
            if runs.iter().any(|run| {
                run.includes_commit(commit.repository_id(), commit.branch(), commit.commit_sha())
            }) {
                continue;
            }

            if let Some(repository) = repositories
                .iter()
                .find(|repository| repository.id() == commit.repository_id())
            {
                cancel_superseded_runs(
                    runs,
                    repository.id(),
                    commit.branch(),
                    commit.discovered_at_ms(),
                );
                let steps = repository
                    .pipeline()
                    .steps()
                    .iter()
                    .map(|step| {
                        RunStep::new(
                            step.name(),
                            step.command(),
                            repository.pipeline().image(),
                            repository.pipeline().artifact_paths().to_vec(),
                        )
                    })
                    .collect();
                let run_id = next_run_id(runs);
                runs.push(Run::new(
                    run_id,
                    repository.id(),
                    repository.name(),
                    commit.branch(),
                    commit.commit_sha(),
                    commit.discovered_at_ms(),
                    steps,
                ));
                queued += 1;
            }
        }

        queued
    }
}

fn cancel_superseded_runs(runs: &mut [Run], repository_id: u64, branch: &str, timestamp_ms: u64) {
    for run in runs {
        if run.repository_id() == repository_id
            && run.branch() == branch
            && run.status() == RunStatus::Queued
        {
            run.cancel(
                timestamp_ms,
                "run canceled because a newer commit was queued",
            );
        }
    }
}

fn next_run_id(runs: &[Run]) -> u64 {
    runs.iter().map(Run::id).max().unwrap_or(0) + 1
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;

    use crate::repository::{BranchRule, DiscoveredCommit, Pipeline, PipelineStep, Repository};
    use crate::run_status::RunStatus;

    use super::Scheduler;

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
    fn scheduler_queues_new_commits_once() {
        let repository = repository();
        let scheduler = Scheduler::new();
        let commit = DiscoveredCommit::new(1, "cid", "main", "abc123", 100);
        let mut runs = Vec::new();

        let first = scheduler.enqueue_runs(
            std::slice::from_ref(&repository),
            std::slice::from_ref(&commit),
            &mut runs,
        );
        let second = scheduler.enqueue_runs(
            std::slice::from_ref(&repository),
            std::slice::from_ref(&commit),
            &mut runs,
        );

        assert_eq!(first, 1);
        assert_eq!(second, 0);
        assert_eq!(runs.len(), 1);
    }

    #[test]
    fn scheduler_cancels_older_queued_runs_for_same_branch() {
        let repository = repository();
        let scheduler = Scheduler::new();
        let mut runs = Vec::new();

        scheduler.enqueue_runs(
            std::slice::from_ref(&repository),
            &[DiscoveredCommit::new(1, "cid", "main", "old", 100)],
            &mut runs,
        );
        scheduler.enqueue_runs(
            std::slice::from_ref(&repository),
            &[DiscoveredCommit::new(1, "cid", "main", "new", 200)],
            &mut runs,
        );

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].status(), RunStatus::Canceled);
        assert_eq!(runs[1].status(), RunStatus::Queued);
    }
}
