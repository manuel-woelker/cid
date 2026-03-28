use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use cid_daemon::{CidStateStore, DaemonState, Run, RunSummary};
use cid_pal::pal::PalHandle;
use serde::Serialize;

const UI_DIST_DIR: &str = "ui/dist";

pub fn serve(address: &str, state_dir: FilePath, pal: PalHandle) -> CidResult<()> {
    let listener = TcpListener::bind(address)
        .with_context(|| format!("failed to bind web server to `{address}`"))?;
    let store = CidStateStore::new(state_dir, pal.clone());
    let asset_root = FilePath::new(UI_DIST_DIR);

    for stream in listener.incoming() {
        let mut stream = stream.context("failed to accept web connection")?;
        handle_connection(&mut stream, &store, pal.clone(), &asset_root)?;
    }

    Ok(())
}

fn handle_connection(
    stream: &mut TcpStream,
    store: &CidStateStore,
    pal: PalHandle,
    asset_root: &FilePath,
) -> CidResult<()> {
    let mut buffer = [0; 4096];
    let read = stream
        .read(&mut buffer)
        .context("failed to read HTTP request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request_path(&request);

    let response = route_request(path, store, pal, asset_root)?;
    write_response(stream, &response)
}

fn route_request(
    path: &str,
    store: &CidStateStore,
    pal: PalHandle,
    asset_root: &FilePath,
) -> CidResult<HttpResponse> {
    let state = store.load()?;

    match path {
        "/api/repositories" => json_response(state.repositories()),
        "/api/runs" => json_response(state.runs()),
        "/api/summary" => json_response(&RunSummary::from_runs(state.runs())),
        path if path.starts_with("/api/runs/") => match find_run(&state, path) {
            Some(run) => json_response(run),
            None => Ok(text_response("404 Not Found", "run not found")),
        },
        _ => asset_response(path, &pal, asset_root),
    }
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

fn asset_response(path: &str, pal: &PalHandle, asset_root: &FilePath) -> CidResult<HttpResponse> {
    let Some(asset_path) = resolve_asset_path(path, asset_root) else {
        return Ok(text_response("404 Not Found", "not found"));
    };

    if pal.file_exists(&asset_path)? {
        return Ok(HttpResponse::new(
            "200 OK",
            content_type_for_path(&asset_path),
            read_file_bytes(pal, &asset_path)?,
        ));
    }

    let index_path = asset_root.join("index.html");
    if should_use_spa_fallback(path) && pal.file_exists(&index_path)? {
        return Ok(HttpResponse::new(
            "200 OK",
            "text/html; charset=utf-8",
            read_file_bytes(pal, &index_path)?,
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

fn read_file_bytes(pal: &PalHandle, path: &FilePath) -> CidResult<Vec<u8>> {
    let mut file = pal.read_file(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .with_context(|| format!("failed to read asset `{path}`"))?;
    Ok(buffer)
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

fn request_path(request: &str) -> &str {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
}

fn find_run<'a>(state: &'a DaemonState, path: &str) -> Option<&'a Run> {
    path.trim_start_matches("/api/runs/")
        .parse::<u64>()
        .ok()
        .and_then(|run_id| state.runs().iter().find(|run| run.id() == run_id))
}

fn resolve_asset_path(path: &str, asset_root: &FilePath) -> Option<FilePath> {
    if path.contains("..") {
        return None;
    }

    let relative = if path == "/" {
        FilePath::new("index.html")
    } else {
        FilePath::from_string(path.trim_start_matches('/'))
    };

    let normalized = relative.normalize();
    if normalized.as_str().starts_with("../") {
        return None;
    }

    Some(asset_root.join(normalized.as_str()))
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
    use cid_base::file_path::FilePath;
    use cid_pal::pal::PalHandle;
    use cid_pal::pal_mock::PalMock;

    use cid_daemon::{BranchRule, Pipeline, PipelineStep, Repository, Run, RunStep};

    use super::{
        CidStateStore, asset_response, content_type_for_path, request_path, resolve_asset_path,
        route_request,
    };

    #[test]
    fn request_path_extracts_target_from_request_line() {
        assert_eq!(
            request_path("GET /api/runs HTTP/1.1\r\nHost: localhost\r\n"),
            "/api/runs"
        );
    }

    #[test]
    fn route_request_returns_json_summary() {
        let pal = PalMock::new();
        let store = sample_store(&pal);
        let response = route_request(
            "/api/summary",
            &store,
            PalHandle::new(pal),
            &FilePath::new("ui/dist"),
        )
        .unwrap();

        assert_eq!(response.status, "200 OK");
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        assert!(
            String::from_utf8(response.body)
                .unwrap()
                .contains("\"total_runs\": 1")
        );
    }

    #[test]
    fn asset_response_serves_index_html_for_spa_routes() {
        let pal = PalMock::new();
        pal.set_file("ui/dist/index.html", "<!doctype html><title>cid</title>");

        let response =
            asset_response("/runs/7", &PalHandle::new(pal), &FilePath::new("ui/dist")).unwrap();

        assert_eq!(response.status, "200 OK");
        assert_eq!(response.content_type, "text/html; charset=utf-8");
        assert_eq!(
            String::from_utf8(response.body).unwrap(),
            "<!doctype html><title>cid</title>"
        );
    }

    #[test]
    fn resolve_asset_path_rejects_parent_segments() {
        assert!(resolve_asset_path("/../secret.txt", &FilePath::new("ui/dist")).is_none());
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

    fn sample_store(pal: &PalMock) -> CidStateStore {
        let store = CidStateStore::new(FilePath::new(".cid"), PalHandle::new(pal.clone()));
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

        let run = Run::new(
            1,
            1,
            "cid",
            "main",
            "abc123",
            100,
            vec![RunStep::new("test", "cargo test", "rust:1.85", Vec::new())],
        );
        let state_yaml = serde_yaml::to_string(&serde_json::json!({
            "repositories": [repository],
            "discovered_commits": [],
            "runs": [run],
        }))
        .unwrap();
        pal.set_file(".cid/state.yaml", state_yaml);

        store
    }
}
