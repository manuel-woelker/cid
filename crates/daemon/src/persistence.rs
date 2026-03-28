use std::collections::BTreeMap;
use std::fs;

use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::daemon::DaemonState;
use crate::repository::{BranchRule, DiscoveredCommit, Pipeline, Repository};
use crate::run::{Run, RunEvent, RunStep};

#[derive(Debug, Clone)]
pub struct CidStateStore {
    state_dir: FilePath,
}

#[derive(Debug, Clone)]
struct RepositoryRow {
    id: u64,
    repository_key: String,
    name: String,
    path: String,
    last_seen_at_ms: Option<u64>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRepositoryConfig {
    branch_rules: Vec<BranchRule>,
    pipeline: Pipeline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRunArtifacts {
    artifact_paths: Vec<FilePath>,
}

#[derive(Debug, Clone)]
struct StoredRunRow {
    id: u64,
    ref_name: String,
    commit_sha: String,
    status: String,
    queued_at_ms: u64,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    duration_ms: Option<u64>,
    run_dir: String,
    workspace_revision: Option<String>,
    step_results_json: String,
    events_json: String,
    artifacts_json: String,
}

impl CidStateStore {
    pub fn new(state_dir: FilePath) -> Self {
        Self { state_dir }
    }

    pub fn load(&self) -> CidResult<DaemonState> {
        self.ensure_layout()?;
        let connection = self.open_registry_connection()?;
        let repository_rows = self.load_repository_rows(&connection)?;

        let mut state = DaemonState::default();

        for row in repository_rows {
            let repository = self.load_repository(&row)?;
            let discovered_commits =
                self.load_discovered_commits(repository.id(), repository.name(), &row)?;
            let runs = self.load_runs(repository.id(), repository.name(), &row)?;

            state.repositories.push(repository);
            state.discovered_commits.extend(discovered_commits);
            state.runs.extend(runs);
        }

        state.runs.sort_by_key(Run::id);
        Ok(state)
    }

    pub fn save(&self, state: &DaemonState) -> CidResult<()> {
        self.ensure_layout()?;
        self.save_repositories(&state.repositories)?;

        for repository in &state.repositories {
            self.save_repository_state(repository, state)?;
        }

        Ok(())
    }

    pub fn write_step_log(
        &self,
        repository: &Repository,
        run_id: u64,
        step_index: usize,
        contents: &str,
    ) -> CidResult<FilePath> {
        self.ensure_repository_layout(&repository_key(repository))?;
        let run_dir = self
            .repository_runs_dir(&repository_key(repository))
            .join(format!("run-{run_id}"));
        fs::create_dir_all(run_dir.as_path())
            .with_context(|| format!("failed to create run log directory `{run_dir}`"))?;

        let log_path = run_dir.join(format!("step-{step_index}.log"));
        fs::write(log_path.as_path(), contents.as_bytes())
            .with_context(|| format!("failed to write step log `{log_path}`"))?;
        Ok(log_path)
    }

    pub fn state_file_path(&self) -> FilePath {
        self.registry_db_path()
    }

    pub fn state_dir(&self) -> &FilePath {
        &self.state_dir
    }

    fn save_repositories(&self, repositories: &[Repository]) -> CidResult<()> {
        let mut connection = self.open_registry_connection()?;
        let transaction = connection
            .transaction()
            .context("failed to begin repositories transaction")?;
        transaction
            .execute("DELETE FROM repositories", [])
            .context("failed to clear repositories table")?;

        for repository in repositories {
            let key = repository_key(repository);
            let status = if repository.status().last_error().is_some() {
                "error"
            } else {
                "ok"
            };
            transaction
                .execute(
                    "INSERT INTO repositories (
                        id,
                        repository_key,
                        name,
                        path,
                        status,
                        last_seen_at_ms,
                        last_error,
                        created_at_ms,
                        updated_at_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0)",
                    params![
                        repository.id() as i64,
                        key,
                        repository.name(),
                        repository.path().as_str(),
                        status,
                        repository
                            .status()
                            .last_seen_at_ms()
                            .map(|value| value as i64),
                        repository.status().last_error(),
                    ],
                )
                .with_context(|| format!("failed to insert repository `{}`", repository.name()))?;
        }

        transaction
            .commit()
            .context("failed to commit repositories transaction")
    }

    fn save_repository_state(&self, repository: &Repository, state: &DaemonState) -> CidResult<()> {
        let key = repository_key(repository);
        self.ensure_repository_layout(&key)?;
        let mut connection = self.open_repository_connection(&key)?;
        let transaction = connection
            .transaction()
            .with_context(|| format!("failed to begin repository transaction for `{key}`"))?;

        transaction
            .execute("DELETE FROM repo_state", [])
            .context("failed to clear repo_state")?;
        transaction
            .execute("DELETE FROM tracked_refs", [])
            .context("failed to clear tracked_refs")?;
        transaction
            .execute("DELETE FROM runs", [])
            .context("failed to clear runs")?;

        let config_payload = serde_json::to_string(&StoredRepositoryConfig {
            branch_rules: repository.branch_rules().to_vec(),
            pipeline: repository.pipeline().clone(),
        })
        .context("failed to serialize repository config")?;

        let repo_dir = self.repository_dir(&key);
        let workspace_path = repo_dir.join("workspace");
        let cache_path = repo_dir.join("cache");
        let runs_path = repo_dir.join("runs");

        transaction
            .execute(
                "INSERT INTO repo_state (
                    repository_id,
                    repository_key,
                    name,
                    source_path,
                    workspace_path,
                    cache_path,
                    runs_path,
                    config_revision,
                    config_payload,
                    created_at_ms,
                    updated_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, 0, 0)",
                params![
                    repository.id() as i64,
                    key,
                    repository.name(),
                    repository.path().as_str(),
                    workspace_path.as_str(),
                    cache_path.as_str(),
                    runs_path.as_str(),
                    config_payload,
                ],
            )
            .with_context(|| format!("failed to insert repo_state for `{key}`"))?;

        for (ref_name, ref_state) in tracked_refs_for_repository(state, repository) {
            transaction
                .execute(
                    "INSERT INTO tracked_refs (
                        ref_name,
                        commit_sha,
                        last_seen_at_ms,
                        updated_at_ms
                    ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        ref_name,
                        ref_state.commit_sha,
                        ref_state.last_seen_at_ms.map(|value| value as i64),
                        ref_state.last_seen_at_ms.unwrap_or(0) as i64,
                    ],
                )
                .with_context(|| format!("failed to insert tracked ref for `{key}`"))?;
        }

        for run in state
            .runs()
            .iter()
            .filter(|run| run.repository_id() == repository.id())
        {
            let artifacts = StoredRunArtifacts {
                artifact_paths: run
                    .steps()
                    .iter()
                    .flat_map(|step| step.artifact_paths().iter().cloned())
                    .collect(),
            };
            transaction
                .execute(
                    "INSERT INTO runs (
                        id,
                        ref_name,
                        commit_sha,
                        status,
                        queued_at_ms,
                        started_at_ms,
                        finished_at_ms,
                        duration_ms,
                        run_dir,
                        workspace_revision,
                        step_results_json,
                        events_json,
                        artifacts_json,
                        created_at_ms,
                        updated_at_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, ?12, 0, 0)",
                    params![
                        run.id() as i64,
                        run.branch(),
                        run.commit_sha(),
                        run.status().label(),
                        run.queued_at_ms() as i64,
                        run.started_at_ms().map(|value| value as i64),
                        run.finished_at_ms().map(|value| value as i64),
                        run.finished_at_ms()
                            .zip(run.started_at_ms())
                            .map(|(finished, started)| finished.saturating_sub(started) as i64),
                        self.repository_runs_dir(&key)
                            .join(format!("run-{}", run.id()))
                            .as_str(),
                        serde_json::to_string(run.steps())
                            .context("failed to serialize run steps")?,
                        serde_json::to_string(run.events())
                            .context("failed to serialize run events")?,
                        serde_json::to_string(&artifacts)
                            .context("failed to serialize run artifacts")?,
                    ],
                )
                .with_context(|| format!("failed to insert run {} for `{key}`", run.id()))?;
        }

        transaction
            .commit()
            .with_context(|| format!("failed to commit repository transaction for `{key}`"))
    }

    fn load_repository(&self, row: &RepositoryRow) -> CidResult<Repository> {
        let connection = self.open_repository_connection(&row.repository_key)?;
        let (config_payload,): (String,) = connection
            .query_row("SELECT config_payload FROM repo_state LIMIT 1", [], |row| {
                Ok((row.get(0)?,))
            })
            .with_context(|| {
                format!(
                    "failed to load repo_state for repository `{}`",
                    row.repository_key
                )
            })?;
        let config: StoredRepositoryConfig =
            serde_json::from_str(&config_payload).context("failed to parse repo_state config")?;

        let mut repository = Repository::new(
            row.id,
            row.name.clone(),
            FilePath::new(&row.path),
            config.branch_rules,
            config.pipeline,
        );

        if let Some(last_seen_at_ms) = row.last_seen_at_ms {
            repository.mark_seen(last_seen_at_ms);
        }
        if let Some(last_error) = &row.last_error {
            repository.mark_error(last_error.clone());
        }

        Ok(repository)
    }

    fn load_discovered_commits(
        &self,
        repository_id: u64,
        repository_name: &str,
        row: &RepositoryRow,
    ) -> CidResult<Vec<DiscoveredCommit>> {
        let connection = self.open_repository_connection(&row.repository_key)?;
        let mut statement = connection
            .prepare(
                "SELECT ref_name, commit_sha, last_seen_at_ms
                 FROM tracked_refs
                 WHERE commit_sha IS NOT NULL",
            )
            .with_context(|| format!("failed to prepare tracked_refs query for `{}`", row.name))?;

        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })
            .with_context(|| format!("failed to read tracked_refs for `{}`", row.name))?;

        let mut discovered_commits = Vec::new();
        for row in rows {
            let (ref_name, commit_sha, last_seen_at_ms) =
                row.context("failed to decode tracked_refs row")?;
            discovered_commits.push(DiscoveredCommit::new(
                repository_id,
                repository_name,
                ref_name,
                commit_sha,
                last_seen_at_ms.unwrap_or(0) as u64,
            ));
        }

        Ok(discovered_commits)
    }

    fn load_runs(
        &self,
        repository_id: u64,
        repository_name: &str,
        row: &RepositoryRow,
    ) -> CidResult<Vec<Run>> {
        let connection = self.open_repository_connection(&row.repository_key)?;
        let mut statement = connection
            .prepare(
                "SELECT
                    id,
                    ref_name,
                    commit_sha,
                    status,
                    queued_at_ms,
                    started_at_ms,
                    finished_at_ms,
                    duration_ms,
                    run_dir,
                    workspace_revision,
                    step_results_json,
                    events_json,
                    artifacts_json
                 FROM runs
                 ORDER BY id",
            )
            .with_context(|| format!("failed to prepare runs query for `{}`", row.name))?;

        let rows = statement
            .query_map([], |row| {
                Ok(StoredRunRow {
                    id: row.get::<_, i64>(0)? as u64,
                    ref_name: row.get(1)?,
                    commit_sha: row.get(2)?,
                    status: row.get(3)?,
                    queued_at_ms: row.get::<_, i64>(4)? as u64,
                    started_at_ms: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                    finished_at_ms: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                    duration_ms: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                    run_dir: row.get(8)?,
                    workspace_revision: row.get(9)?,
                    step_results_json: row.get(10)?,
                    events_json: row.get(11)?,
                    artifacts_json: row.get(12)?,
                })
            })
            .with_context(|| format!("failed to read runs for `{}`", row.name))?;

        let mut runs = Vec::new();
        for row in rows {
            let stored = row.context("failed to decode run row")?;
            let run = self.decode_run(repository_id, repository_name, stored)?;
            runs.push(run);
        }

        Ok(runs)
    }

    fn decode_run(
        &self,
        repository_id: u64,
        repository_name: &str,
        stored: StoredRunRow,
    ) -> CidResult<Run> {
        let steps: Vec<RunStep> =
            serde_json::from_str(&stored.step_results_json).context("failed to parse run steps")?;
        let events: Vec<RunEvent> =
            serde_json::from_str(&stored.events_json).context("failed to parse run events")?;
        let artifacts: StoredRunArtifacts = serde_json::from_str(&stored.artifacts_json)
            .context("failed to parse run artifacts")?;

        let mut run_value = serde_json::to_value(serde_json::json!({
            "id": stored.id,
            "repository_id": repository_id,
            "repository_name": repository_name,
            "branch": stored.ref_name,
            "commit_sha": stored.commit_sha,
            "status": stored.status,
            "queued_at_ms": stored.queued_at_ms,
            "started_at_ms": stored.started_at_ms,
            "finished_at_ms": stored.finished_at_ms,
            "steps": steps,
            "events": events,
        }))
        .context("failed to build run value")?;

        let run = serde_json::from_value(run_value.take()).context("failed to decode run")?;

        let _ = stored.duration_ms;
        let _ = stored.run_dir;
        let _ = stored.workspace_revision;
        let _ = artifacts;

        Ok(run)
    }

    fn load_repository_rows(&self, connection: &Connection) -> CidResult<Vec<RepositoryRow>> {
        let mut statement = connection
            .prepare(
                "SELECT
                    id,
                    repository_key,
                    name,
                    path,
                    last_seen_at_ms,
                    last_error
                 FROM repositories
                 ORDER BY id",
            )
            .context("failed to prepare repositories query")?;
        let rows = statement
            .query_map([], |row| {
                Ok(RepositoryRow {
                    id: row.get::<_, i64>(0)? as u64,
                    repository_key: row.get(1)?,
                    name: row.get(2)?,
                    path: row.get(3)?,
                    last_seen_at_ms: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                    last_error: row.get(5)?,
                })
            })
            .context("failed to query repositories")?;

        let mut repositories = Vec::new();
        for row in rows {
            repositories.push(row.context("failed to decode repository row")?);
        }
        Ok(repositories)
    }

    fn ensure_layout(&self) -> CidResult<()> {
        fs::create_dir_all(self.state_dir.as_path())
            .with_context(|| format!("failed to create state directory `{}`", self.state_dir))?;
        fs::create_dir_all(self.repositories_dir().as_path()).with_context(|| {
            format!(
                "failed to create repositories directory `{}`",
                self.repositories_dir()
            )
        })?;
        let connection = self.open_registry_connection()?;
        self.create_registry_schema(&connection)
    }

    fn ensure_repository_layout(&self, repository_key: &str) -> CidResult<()> {
        let repository_dir = self.repository_dir(repository_key);
        fs::create_dir_all(repository_dir.as_path())
            .with_context(|| format!("failed to create repository directory `{repository_dir}`"))?;
        fs::create_dir_all(self.repository_workspace_dir(repository_key).as_path()).with_context(
            || {
                format!(
                    "failed to create workspace directory `{}`",
                    self.repository_workspace_dir(repository_key)
                )
            },
        )?;
        fs::create_dir_all(self.repository_cache_dir(repository_key).as_path()).with_context(
            || {
                format!(
                    "failed to create cache directory `{}`",
                    self.repository_cache_dir(repository_key)
                )
            },
        )?;
        fs::create_dir_all(self.repository_runs_dir(repository_key).as_path()).with_context(
            || {
                format!(
                    "failed to create runs directory `{}`",
                    self.repository_runs_dir(repository_key)
                )
            },
        )?;

        let connection = self.open_repository_connection(repository_key)?;
        self.create_repository_schema(&connection)
    }

    fn open_registry_connection(&self) -> CidResult<Connection> {
        Connection::open(self.registry_db_path().as_path()).with_context(|| {
            format!(
                "failed to open registry database `{}`",
                self.registry_db_path()
            )
        })
    }

    fn open_repository_connection(&self, repository_key: &str) -> CidResult<Connection> {
        Connection::open(self.repository_db_path(repository_key).as_path()).with_context(|| {
            format!(
                "failed to open repository database `{}`",
                self.repository_db_path(repository_key)
            )
        })
    }

    fn create_registry_schema(&self, connection: &Connection) -> CidResult<()> {
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS repositories (
                    id INTEGER PRIMARY KEY,
                    repository_key TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL,
                    status TEXT NOT NULL,
                    last_seen_at_ms INTEGER,
                    last_error TEXT,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );",
            )
            .context("failed to create repositories schema")
    }

    fn create_repository_schema(&self, connection: &Connection) -> CidResult<()> {
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS repo_state (
                    repository_id INTEGER PRIMARY KEY,
                    repository_key TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    workspace_path TEXT NOT NULL,
                    cache_path TEXT NOT NULL,
                    runs_path TEXT NOT NULL,
                    config_revision TEXT,
                    config_payload TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS tracked_refs (
                    id INTEGER PRIMARY KEY,
                    ref_name TEXT NOT NULL UNIQUE,
                    commit_sha TEXT,
                    last_seen_at_ms INTEGER,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS runs (
                    id INTEGER PRIMARY KEY,
                    ref_name TEXT NOT NULL,
                    commit_sha TEXT NOT NULL,
                    status TEXT NOT NULL,
                    queued_at_ms INTEGER NOT NULL,
                    started_at_ms INTEGER,
                    finished_at_ms INTEGER,
                    duration_ms INTEGER,
                    run_dir TEXT NOT NULL,
                    workspace_revision TEXT,
                    step_results_json TEXT NOT NULL,
                    events_json TEXT NOT NULL,
                    artifacts_json TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );",
            )
            .context("failed to create repository schema")
    }

    fn registry_db_path(&self) -> FilePath {
        self.state_dir.join("cid.db")
    }

    fn repositories_dir(&self) -> FilePath {
        self.state_dir.join("repositories")
    }

    fn repository_dir(&self, repository_key: &str) -> FilePath {
        self.repositories_dir().join(repository_key)
    }

    fn repository_db_path(&self, repository_key: &str) -> FilePath {
        self.repository_dir(repository_key).join("cid-repo.db")
    }

    fn repository_workspace_dir(&self, repository_key: &str) -> FilePath {
        self.repository_dir(repository_key).join("workspace")
    }

    fn repository_cache_dir(&self, repository_key: &str) -> FilePath {
        self.repository_dir(repository_key).join("cache")
    }

    fn repository_runs_dir(&self, repository_key: &str) -> FilePath {
        self.repository_dir(repository_key).join("runs")
    }
}

#[derive(Debug, Clone)]
struct TrackedRefState {
    commit_sha: Option<String>,
    last_seen_at_ms: Option<u64>,
}

fn tracked_refs_for_repository(
    state: &DaemonState,
    repository: &Repository,
) -> BTreeMap<String, TrackedRefState> {
    let mut tracked_refs = BTreeMap::new();

    for rule in repository.branch_rules() {
        tracked_refs.insert(
            rule.branch().to_string(),
            TrackedRefState {
                commit_sha: None,
                last_seen_at_ms: None,
            },
        );
    }

    for commit in state
        .discovered_commits()
        .iter()
        .filter(|commit| commit.repository_id() == repository.id())
    {
        let entry = tracked_refs
            .entry(commit.branch().to_string())
            .or_insert(TrackedRefState {
                commit_sha: None,
                last_seen_at_ms: None,
            });

        if entry
            .last_seen_at_ms
            .is_none_or(|existing| commit.discovered_at_ms() >= existing)
        {
            entry.commit_sha = Some(commit.commit_sha().to_string());
            entry.last_seen_at_ms = Some(commit.discovered_at_ms());
        }
    }

    tracked_refs
}

fn repository_key(repository: &Repository) -> String {
    let slug = slugify(repository.name());
    let encoded_path = hex_encode(repository.path().as_str().as_bytes());
    format!("{slug}-{encoded_path}")
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

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(nibble_to_hex(byte >> 4));
        output.push(nibble_to_hex(byte & 0x0f));
    }
    output
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use cid_base::file_path::FilePath;

    use crate::daemon::DaemonState;
    use crate::repository::{BranchRule, DiscoveredCommit, Pipeline, PipelineStep, Repository};
    use crate::run::{Run, RunStep};

    use super::CidStateStore;

    #[test]
    fn persistence_round_trip_restores_state() {
        let state_dir = temp_state_dir("persistence-round-trip");
        let store = CidStateStore::new(state_dir.clone());
        let state = sample_state();

        store.save(&state).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(loaded.repositories(), state.repositories());
        assert_eq!(loaded.runs(), state.runs());
        assert!(store.state_file_path().as_path().exists());
        cleanup(&state_dir);
    }

    #[test]
    fn step_logs_are_written_under_repository_run_directories() {
        let state_dir = temp_state_dir("step-logs");
        let store = CidStateStore::new(state_dir.clone());
        let repository = sample_state().repositories()[0].clone();

        let log_path = store
            .write_step_log(&repository, 12, 1, "hello log")
            .unwrap();

        assert!(log_path.as_str().contains("/runs/run-12/step-1.log"));
        assert_eq!(
            std::fs::read_to_string(log_path.as_path()).unwrap(),
            "hello log"
        );
        cleanup(&state_dir);
    }

    #[test]
    fn repository_key_is_stable_for_repository_identity() {
        let repository = Repository::new(
            7,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::new(
                "rust:1.85",
                vec![PipelineStep::new("test", "cargo test")],
                Vec::new(),
            ),
        );

        let first = super::repository_key(&repository);
        let second = super::repository_key(&repository);

        assert_eq!(first, second);
    }

    fn sample_state() -> DaemonState {
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

        let discovered_commit = DiscoveredCommit::new(1, "cid", "main", "abc123", 100);
        let run = Run::new(
            1,
            1,
            "cid",
            "main",
            "abc123",
            100,
            vec![RunStep::new("test", "cargo test", "rust:1.85", Vec::new())],
        );

        DaemonState {
            repositories: vec![repository],
            discovered_commits: vec![discovered_commit],
            runs: vec![run],
        }
    }

    fn temp_state_dir(prefix: &str) -> FilePath {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cid-{prefix}-{unique}"));
        FilePath::new(path)
    }

    fn cleanup(path: &FilePath) {
        let _ = std::fs::remove_dir_all(path.as_path());
    }
}
