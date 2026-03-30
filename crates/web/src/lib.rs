mod asset_source;
mod embedded_assets;

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use asset_source::AssetSource;
use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use cid_daemon::{DaemonApi, DaemonState, Repository, Run, RunStatus, RunSummary};
use serde::Serialize;

const UI_DIST_DIR: &str = "ui/dist";

pub fn serve<D: DaemonApi>(
    address: &str,
    daemon: D,
    pal: cid_pal::pal::PalHandle,
) -> CidResult<()> {
    let listener = TcpListener::bind(address)
        .with_context(|| format!("failed to bind web server to `{address}`"))?;
    let asset_source = AssetSource::load(pal, FilePath::new(UI_DIST_DIR))?;

    for stream in listener.incoming() {
        let mut stream = stream.context("failed to accept web connection")?;
        handle_connection(&mut stream, &daemon, &asset_source)?;
    }

    Ok(())
}

fn handle_connection<D: DaemonApi>(
    stream: &mut TcpStream,
    daemon: &D,
    asset_source: &AssetSource,
) -> CidResult<()> {
    let mut buffer = [0; 4096];
    let read = stream
        .read(&mut buffer)
        .context("failed to read HTTP request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let method = request_method(&request);
    let path = request_path(&request);

    let response = route_request(method, path, daemon, asset_source)?;
    write_response(stream, &response)
}

fn route_request<D: DaemonApi>(
    method: &str,
    path: &str,
    daemon: &D,
    asset_source: &AssetSource,
) -> CidResult<HttpResponse> {
    if !path.starts_with("/api/") {
        return asset_response(path, asset_source);
    }

    match (method, api_path_segments(path).as_slice()) {
        ("GET", ["api", "repositories"]) => json_response(daemon.snapshot()?.repositories()),
        ("GET", ["api", "repositories", repository_name]) => {
            let state = daemon.snapshot()?;
            match find_repository(&state, repository_name)? {
                Some(repository) => json_response(repository),
                None => Ok(text_response("404 Not Found", "repository not found")),
            }
        }
        ("GET", ["api", "repositories", repository_name, "branches"]) => {
            let state = daemon.snapshot()?;
            match find_repository(&state, repository_name)? {
                Some(repository) => json_response(&branch_summaries(&state, repository)),
                None => Ok(text_response("404 Not Found", "repository not found")),
            }
        }
        (
            "GET",
            [
                "api",
                "repositories",
                repository_name,
                "branches",
                branch_name,
            ],
        ) => {
            let state = daemon.snapshot()?;
            match find_repository(&state, repository_name)? {
                Some(repository) => {
                    let branch_name = decode_path_segment(branch_name)?;
                    match branch_detail(&state, repository, &branch_name) {
                        Some(detail) => json_response(&detail),
                        None => Ok(text_response("404 Not Found", "branch not found")),
                    }
                }
                None => Ok(text_response("404 Not Found", "repository not found")),
            }
        }
        ("GET", ["api", "runs"]) => json_response(daemon.snapshot()?.runs()),
        ("GET", ["api", "runs", run_id]) => {
            let state = daemon.snapshot()?;
            match find_run(&state, run_id.parse::<u64>().ok()) {
                Some(run) => json_response(run),
                None => Ok(text_response("404 Not Found", "run not found")),
            }
        }
        ("POST", ["api", "runs", run_id, "replay"]) => replay_run_response(daemon, run_id),
        ("GET", ["api", "runs", run_id, "steps", step_index, "log"]) => {
            let state = daemon.snapshot()?;
            match find_run(&state, run_id.parse::<u64>().ok()) {
                Some(run) => match step_log_response(run, step_index.parse::<usize>().ok())? {
                    Some(response) => Ok(response),
                    None => Ok(text_response("404 Not Found", "step log not found")),
                },
                None => Ok(text_response("404 Not Found", "run not found")),
            }
        }
        ("GET", ["api", "summary"]) => {
            json_response(&RunSummary::from_runs(daemon.snapshot()?.runs()))
        }
        _ => Ok(text_response("404 Not Found", "not found")),
    }
}

fn replay_run_response<D: DaemonApi>(daemon: &D, run_id: &str) -> CidResult<HttpResponse> {
    let Some(run_id) = run_id.parse::<u64>().ok() else {
        return Ok(text_response("404 Not Found", "run not found"));
    };

    let next_run = match daemon.replay_run(run_id) {
        Ok(run) => run,
        Err(error) if error.to_test_string().contains("not found") => {
            return Ok(text_response("404 Not Found", "run not found"));
        }
        Err(error) => return Err(error),
    };

    Ok(HttpResponse::new(
        "201 Created",
        "application/json; charset=utf-8",
        serde_json::to_vec_pretty(&next_run).context("failed to serialize replayed run")?,
    ))
}

fn request_method(request: &str) -> &str {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("GET")
}

fn request_path(request: &str) -> &str {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
}

fn json_response<T>(value: &T) -> CidResult<HttpResponse>
where
    T: Serialize + ?Sized,
{
    Ok(HttpResponse::new(
        "200 OK",
        "application/json; charset=utf-8",
        serde_json::to_vec_pretty(value).context("failed to serialize JSON response")?,
    ))
}

fn text_response(status: &'static str, body: &str) -> HttpResponse {
    HttpResponse::new(
        status,
        "text/plain; charset=utf-8",
        body.as_bytes().to_vec(),
    )
}

fn text_file_response(body: String) -> HttpResponse {
    HttpResponse::new("200 OK", "text/plain; charset=utf-8", body.into_bytes())
}

fn asset_response(path: &str, asset_source: &AssetSource) -> CidResult<HttpResponse> {
    let Some(asset_path) = resolve_asset_path(path) else {
        return Ok(text_response("404 Not Found", "not found"));
    };

    if let Some(asset_bytes) = asset_source.read(&asset_path)? {
        return Ok(HttpResponse::new(
            "200 OK",
            content_type_for_path(&asset_path),
            asset_bytes,
        ));
    }

    let index_path = FilePath::new("index.html");
    if should_use_spa_fallback(path)
        && let Some(index_bytes) = asset_source.read(&index_path)?
    {
        return Ok(HttpResponse::new(
            "200 OK",
            "text/html; charset=utf-8",
            index_bytes,
        ));
    }

    if path == "/" {
        return Ok(HttpResponse::new(
            "503 Service Unavailable",
            "text/html; charset=utf-8",
            b"<!doctype html><html><head><meta charset=\"utf-8\"><title>cid</title></head><body><main><h1>cid UI is not built yet</h1><p>Run <code>pnpm --dir ui build</code> to generate <code>ui/dist</code>.</p></main></body></html>".to_vec(),
        ));
    }

    Ok(text_response("404 Not Found", "not found"))
}
fn write_response(stream: &mut TcpStream, response: &HttpResponse) -> CidResult<()> {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len(),
    );

    stream
        .write_all(header.as_bytes())
        .context("failed to write HTTP response header")?;
    stream
        .write_all(&response.body)
        .context("failed to write HTTP response body")?;
    Ok(())
}

fn api_path_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn find_repository<'a>(
    state: &'a DaemonState,
    repository_name: &str,
) -> CidResult<Option<&'a Repository>> {
    let repository_name = decode_path_segment(repository_name)?;

    Ok(state.repositories().iter().find(|repository| {
        repository.name() == repository_name || repository.id().to_string() == repository_name
    }))
}

fn find_run(state: &DaemonState, run_id: Option<u64>) -> Option<&Run> {
    let run_id = run_id?;
    state.runs().iter().find(|run| run.id() == run_id)
}

fn branch_summaries(state: &DaemonState, repository: &Repository) -> Vec<BranchSummary> {
    let mut branches: Vec<_> = repository
        .branch_rules()
        .iter()
        .map(|rule| {
            let latest_run = latest_run_for_branch(state, repository.id(), rule.branch());
            BranchSummary {
                branch_name: rule.branch().to_string(),
                latest_run: latest_run.map(BranchLatestRun::from_run),
                run_count: runs_for_branch(state, repository.id(), rule.branch()).len(),
            }
        })
        .collect();

    branches.sort_by(|left, right| {
        branch_sort_timestamp(right)
            .cmp(&branch_sort_timestamp(left))
            .then_with(|| left.branch_name.cmp(&right.branch_name))
    });

    branches
}

fn branch_detail(
    state: &DaemonState,
    repository: &Repository,
    branch_name: &str,
) -> Option<RepositoryBranchDetail> {
    if !repository
        .branch_rules()
        .iter()
        .any(|rule| rule.branch() == branch_name)
    {
        return None;
    }

    let runs = runs_for_branch(state, repository.id(), branch_name)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let latest_run = runs.first().map(BranchLatestRun::from_run);

    Some(RepositoryBranchDetail {
        repository: repository.clone(),
        branch: BranchSummary {
            branch_name: branch_name.to_string(),
            latest_run,
            run_count: runs.len(),
        },
        runs,
    })
}

fn latest_run_for_branch<'a>(
    state: &'a DaemonState,
    repository_id: u64,
    branch_name: &str,
) -> Option<&'a Run> {
    runs_for_branch(state, repository_id, branch_name)
        .into_iter()
        .next()
}

fn runs_for_branch<'a>(
    state: &'a DaemonState,
    repository_id: u64,
    branch_name: &str,
) -> Vec<&'a Run> {
    let mut runs = state
        .runs()
        .iter()
        .filter(|run| run.repository_id() == repository_id && run.branch() == branch_name)
        .collect::<Vec<_>>();
    runs.sort_by(|left, right| {
        run_activity_timestamp(right)
            .cmp(&run_activity_timestamp(left))
            .then_with(|| right.id().cmp(&left.id()))
    });
    runs
}

fn run_activity_timestamp(run: &Run) -> u64 {
    run.finished_at_ms()
        .or(run.started_at_ms())
        .unwrap_or(run.queued_at_ms())
}

fn branch_sort_timestamp(branch: &BranchSummary) -> Option<u64> {
    branch
        .latest_run
        .as_ref()
        .map(|run| run.activity_timestamp_ms)
}

fn decode_path_segment(segment: &str) -> CidResult<String> {
    let mut bytes = Vec::with_capacity(segment.len());
    let input = segment.as_bytes();
    let mut index = 0;

    while index < input.len() {
        if input[index] == b'%' {
            if index + 2 >= input.len() {
                return Err(cid_base::err!("invalid percent-encoded path segment"));
            }

            let high = decode_hex_nibble(input[index + 1])?;
            let low = decode_hex_nibble(input[index + 2])?;
            bytes.push((high << 4) | low);
            index += 3;
            continue;
        }

        bytes.push(input[index]);
        index += 1;
    }

    String::from_utf8(bytes).context("invalid utf-8 in path segment")
}

fn decode_hex_nibble(byte: u8) -> CidResult<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(cid_base::err!("invalid percent-encoded path segment")),
    }
}

fn step_log_response(run: &Run, step_index: Option<usize>) -> CidResult<Option<HttpResponse>> {
    let step_index = match step_index {
        Some(step_index) => step_index,
        None => return Ok(None),
    };

    let Some(step) = run.steps().get(step_index) else {
        return Ok(None);
    };
    let Some(log_path) = step.log_path() else {
        return Ok(None);
    };

    let log_contents = fs::read_to_string(log_path.as_path())
        .with_context(|| format!("failed to read step log `{log_path}`"))?;
    Ok(Some(text_file_response(log_contents)))
}

#[derive(Debug, Clone, Serialize)]
struct BranchLatestRun {
    run_id: u64,
    status: RunStatus,
    commit_sha: String,
    queued_at_ms: u64,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    activity_timestamp_ms: u64,
}

impl BranchLatestRun {
    fn from_run(run: &Run) -> Self {
        Self {
            run_id: run.id(),
            status: run.status(),
            commit_sha: run.commit_sha().to_string(),
            queued_at_ms: run.queued_at_ms(),
            started_at_ms: run.started_at_ms(),
            finished_at_ms: run.finished_at_ms(),
            activity_timestamp_ms: run_activity_timestamp(run),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct BranchSummary {
    branch_name: String,
    latest_run: Option<BranchLatestRun>,
    run_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct RepositoryBranchDetail {
    repository: Repository,
    branch: BranchSummary,
    runs: Vec<Run>,
}

fn resolve_asset_path(path: &str) -> Option<FilePath> {
    if path.contains("..") {
        return None;
    }

    let relative_path = if path == "/" {
        FilePath::new("index.html")
    } else {
        FilePath::from_string(path.trim_start_matches('/'))
    };

    let normalized = relative_path.normalize();
    if normalized.as_str().starts_with("../") {
        return None;
    }

    Some(normalized)
}

fn should_use_spa_fallback(path: &str) -> bool {
    !path.starts_with("/api/") && !path.contains('.') && path != "/favicon.ico"
}

fn content_type_for_path(path: &FilePath) -> &'static str {
    match path.extension() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

struct HttpResponse {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

impl HttpResponse {
    fn new(status: &'static str, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            body,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use cid_base::file_path::FilePath;
    use cid_base::result::CidResult;
    use cid_pal::pal::PalHandle;
    use cid_pal::pal_mock::PalMock;

    use cid_daemon::{
        BranchRule, DaemonApi, DaemonState, Pipeline, PipelineStep, Repository, Run, RunStep,
    };

    use super::{
        asset_response, content_type_for_path, request_method, request_path, resolve_asset_path,
        route_request,
    };
    use crate::asset_source::AssetSource;

    struct TestDaemon {
        state: Mutex<DaemonState>,
    }

    impl TestDaemon {
        fn new(state: DaemonState) -> Self {
            Self {
                state: Mutex::new(state),
            }
        }
    }

    impl DaemonApi for TestDaemon {
        fn snapshot(&self) -> CidResult<DaemonState> {
            Ok(self.state.lock().unwrap().clone())
        }

        fn replay_run(&self, run_id: u64) -> CidResult<Run> {
            let mut state = self.state.lock().unwrap();
            let Some(source_run) = state.runs().iter().find(|run| run.id() == run_id).cloned()
            else {
                return Err(cid_base::err!("run not found"));
            };
            let Some(repository) = state
                .repositories()
                .iter()
                .find(|repository| repository.id() == source_run.repository_id())
                .cloned()
            else {
                return Err(cid_base::err!("repository not found"));
            };

            let next_run_id = state.runs().iter().map(Run::id).max().unwrap_or(0) + 1;
            let next_run = Run::new(
                next_run_id,
                source_run.repository_id(),
                source_run.repository_name(),
                source_run.branch(),
                source_run.commit_sha(),
                999,
                repository
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
                    .collect(),
            );
            state.push_run(next_run.clone());

            Ok(next_run)
        }
    }

    #[test]
    fn request_path_extracts_target_from_request_line() {
        assert_eq!(
            request_path("GET /api/runs HTTP/1.1\r\nHost: localhost\r\n"),
            "/api/runs"
        );
    }

    #[test]
    fn request_method_extracts_verb_from_request_line() {
        assert_eq!(
            request_method("POST /api/runs/3/replay HTTP/1.1\r\nHost: localhost\r\n"),
            "POST"
        );
    }

    #[test]
    fn route_request_returns_json_summary() {
        let pal = PalMock::new();
        let daemon = TestDaemon::new(sample_state(&pal));
        let response = route_request(
            "GET",
            "/api/summary",
            &daemon,
            &AssetSource::filesystem(PalHandle::new(pal), FilePath::new("ui/dist")),
        )
        .unwrap();

        assert_eq!(response.status, "200 OK");
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        assert!(
            String::from_utf8(response.body)
                .unwrap()
                .contains("\"total_runs\": 3")
        );
    }

    #[test]
    fn asset_response_serves_index_html_for_spa_routes() {
        let pal = PalMock::new();
        pal.set_file("ui/dist/index.html", "<!doctype html><title>cid</title>");

        let response = asset_response(
            "/runs/7",
            &AssetSource::filesystem(PalHandle::new(pal), FilePath::new("ui/dist")),
        )
        .unwrap();

        assert_eq!(response.status, "200 OK");
        assert_eq!(response.content_type, "text/html; charset=utf-8");
        assert_eq!(
            String::from_utf8(response.body).unwrap(),
            "<!doctype html><title>cid</title>"
        );
    }

    #[test]
    fn resolve_asset_path_rejects_parent_segments() {
        assert!(resolve_asset_path("/../secret.txt").is_none());
    }

    #[test]
    fn content_type_detects_frontend_assets() {
        assert_eq!(
            content_type_for_path(&FilePath::new("ui/dist/assets/app.css")),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            content_type_for_path(&FilePath::new("ui/dist/assets/app.js")),
            "application/javascript; charset=utf-8"
        );
    }

    #[test]
    fn route_request_returns_repository_branch_summaries_in_latest_build_order() {
        let pal = PalMock::new();
        let daemon = TestDaemon::new(sample_state(&pal));
        let response = route_request(
            "GET",
            "/api/repositories/cid/branches",
            &daemon,
            &AssetSource::filesystem(PalHandle::new(pal), FilePath::new("ui/dist")),
        )
        .unwrap();

        let body = String::from_utf8(response.body).unwrap();
        let main_index = body.find("\"branch_name\": \"main\"").unwrap();
        let release_index = body.find("\"branch_name\": \"release\"").unwrap();
        let beta_index = body.find("\"branch_name\": \"feature/beta\"").unwrap();

        assert_eq!(response.status, "200 OK");
        assert!(main_index < release_index);
        assert!(release_index < beta_index);
        assert!(body.contains("\"status\": \"passed\""));
        assert!(body.contains("\"status\": \"failed\""));
    }

    #[test]
    fn route_request_returns_branch_detail_for_url_encoded_branch_name() {
        let pal = PalMock::new();
        let daemon = TestDaemon::new(sample_state(&pal));
        let response = route_request(
            "GET",
            "/api/repositories/cid/branches/feature%2Fbeta",
            &daemon,
            &AssetSource::filesystem(PalHandle::new(pal), FilePath::new("ui/dist")),
        )
        .unwrap();

        let body = String::from_utf8(response.body).unwrap();
        assert_eq!(response.status, "200 OK");
        assert!(body.contains("\"branch_name\": \"feature/beta\""));
        assert!(body.contains("\"run_count\": 0"));
    }

    #[test]
    fn route_request_returns_step_log_contents() {
        let pal = PalMock::new();
        let daemon = TestDaemon::new(sample_state(&pal));
        let response = route_request(
            "GET",
            "/api/runs/3/steps/0/log",
            &daemon,
            &AssetSource::filesystem(PalHandle::new(pal), FilePath::new("ui/dist")),
        )
        .unwrap();

        assert_eq!(response.status, "200 OK");
        assert_eq!(response.content_type, "text/plain; charset=utf-8");
        assert_eq!(String::from_utf8(response.body).unwrap(), "latest main log");
    }

    #[test]
    fn route_request_replays_a_run_as_new_queued_run() {
        let pal = PalMock::new();
        let daemon = TestDaemon::new(sample_state(&pal));
        let response = route_request(
            "POST",
            "/api/runs/3/replay",
            &daemon,
            &AssetSource::filesystem(PalHandle::new(pal), FilePath::new("ui/dist")),
        )
        .unwrap();

        let body = String::from_utf8(response.body).unwrap();
        let state = daemon.snapshot().unwrap();

        assert_eq!(response.status, "201 Created");
        assert!(body.contains("\"id\": 4"));
        assert!(body.contains("\"status\": \"queued\""));
        assert_eq!(state.runs().len(), 4);
        let replayed_run = state.runs().iter().find(|run| run.id() == 4).unwrap();
        assert_eq!(replayed_run.status(), cid_daemon::RunStatus::Queued);
        assert_eq!(replayed_run.branch(), "main");
        assert_eq!(replayed_run.commit_sha(), "fedcba");
        assert_eq!(
            replayed_run.steps()[0].status(),
            cid_daemon::RunStatus::Queued
        );
        assert_eq!(replayed_run.steps()[0].image(), "rust:1.85");
        assert!(replayed_run.steps()[0].log_path().is_none());
    }

    fn sample_state(_pal: &PalMock) -> DaemonState {
        let repository = Repository::new(
            1,
            "cid",
            FilePath::new("/repos/cid"),
            vec![
                BranchRule::new("main"),
                BranchRule::new("release"),
                BranchRule::new("feature/beta"),
            ],
            Pipeline::new(
                "rust:1.85",
                vec![PipelineStep::new("test", "cargo test")],
                Vec::new(),
            ),
        );

        let latest_log_dir = FilePath::new(temp_state_dir("web-store")).join("runs");
        std::fs::create_dir_all(latest_log_dir.as_path()).unwrap();
        let latest_log_path = latest_log_dir.join("step-0.log");
        std::fs::write(latest_log_path.as_path(), "latest main log").unwrap();
        let run = sample_run(SampleRun {
            id: 1,
            branch: "main",
            commit_sha: "abc123",
            status: "queued",
            queued_at_ms: 100,
            started_at_ms: None,
            finished_at_ms: None,
            log_path: None,
            image: "rust:1.85",
        });
        let newer_run = sample_run(SampleRun {
            id: 2,
            branch: "release",
            commit_sha: "def456",
            status: "failed",
            queued_at_ms: 90,
            started_at_ms: Some(110),
            finished_at_ms: Some(120),
            log_path: None,
            image: "rust:1.85",
        });
        let latest_run = sample_run(SampleRun {
            id: 3,
            branch: "main",
            commit_sha: "fedcba",
            status: "passed",
            queued_at_ms: 130,
            started_at_ms: Some(140),
            finished_at_ms: Some(150),
            log_path: Some(latest_log_path.as_str()),
            image: "alpine:3.20",
        });

        DaemonState::new(
            vec![repository],
            Vec::new(),
            vec![run, newer_run, latest_run],
        )
    }

    struct SampleRun<'a> {
        id: u64,
        branch: &'a str,
        commit_sha: &'a str,
        status: &'a str,
        queued_at_ms: u64,
        started_at_ms: Option<u64>,
        finished_at_ms: Option<u64>,
        log_path: Option<&'a str>,
        image: &'a str,
    }

    fn sample_run(sample: SampleRun<'_>) -> Run {
        serde_json::from_value(serde_json::json!({
            "id": sample.id,
            "repository_id": 1,
            "repository_name": "cid",
            "branch": sample.branch,
            "commit_sha": sample.commit_sha,
            "status": sample.status,
            "queued_at_ms": sample.queued_at_ms,
            "started_at_ms": sample.started_at_ms,
            "finished_at_ms": sample.finished_at_ms,
            "steps": [
                {
                    "name": "test",
                    "command": "cargo test",
                    "image": sample.image,
                    "status": sample.status,
                    "exit_code": null,
                    "started_at_ms": sample.started_at_ms,
                    "finished_at_ms": sample.finished_at_ms,
                    "duration_ms": sample
                        .finished_at_ms
                        .zip(sample.started_at_ms)
                        .map(|(finished, started)| finished - started),
                    "log_path": sample.log_path,
                    "artifact_paths": [],
                }
            ],
            "events": [],
        }))
        .unwrap()
    }

    fn temp_state_dir(prefix: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("cid-{prefix}-{unique}"))
            .to_string_lossy()
            .to_string()
    }
}
