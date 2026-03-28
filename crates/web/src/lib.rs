use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use cid_daemon::{CidStateStore, DaemonState, RunSummary};
use cid_pal::pal::PalHandle;

pub fn serve(address: &str, state_dir: FilePath, pal: PalHandle) -> CidResult<()> {
    let listener = TcpListener::bind(address)
        .with_context(|| format!("failed to bind web server to `{address}`"))?;
    let store = CidStateStore::new(state_dir, pal);

    for stream in listener.incoming() {
        let mut stream = stream.context("failed to accept web connection")?;
        handle_connection(&mut stream, &store)?;
    }

    Ok(())
}

fn handle_connection(stream: &mut TcpStream, store: &CidStateStore) -> CidResult<()> {
    let mut buffer = [0; 4096];
    let read = stream
        .read(&mut buffer)
        .context("failed to read HTTP request")?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request_path(&request);

    let state = store.load()?;
    let (status, content_type, body) = match path {
        "/" => (
            "200 OK",
            "text/html; charset=utf-8",
            render_dashboard(&state),
        ),
        "/api/repositories" => (
            "200 OK",
            "application/yaml; charset=utf-8",
            serde_yaml::to_string(state.repositories()).context("failed to render repositories")?,
        ),
        "/api/runs" => (
            "200 OK",
            "application/yaml; charset=utf-8",
            serde_yaml::to_string(state.runs()).context("failed to render runs")?,
        ),
        path if path.starts_with("/api/runs/") => match path
            .trim_start_matches("/api/runs/")
            .parse::<u64>()
            .ok()
            .and_then(|run_id| state.runs().iter().find(|run| run.id() == run_id))
        {
            Some(run) => (
                "200 OK",
                "application/yaml; charset=utf-8",
                serde_yaml::to_string(run).context("failed to render run detail")?,
            ),
            None => (
                "404 Not Found",
                "text/plain; charset=utf-8",
                "run not found".to_string(),
            ),
        },
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            "not found".to_string(),
        ),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    stream
        .write_all(response.as_bytes())
        .context("failed to write HTTP response")?;
    Ok(())
}

fn request_path(request: &str) -> &str {
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
}

fn render_dashboard(state: &DaemonState) -> String {
    let summary = RunSummary::from_runs(state.runs());
    let repositories = state
        .repositories()
        .iter()
        .map(|repository| {
            format!(
                "<li><strong>{}</strong><br><code>{}</code><br>last seen: {:?}</li>",
                escape_html(repository.name()),
                escape_html(repository.path().as_str()),
                repository.status().last_seen_at_ms(),
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let runs = state
        .runs()
        .iter()
        .rev()
        .take(10)
        .map(|run| {
            format!(
                "<li>#{} {} {} {} <strong>{}</strong></li>",
                run.id(),
                escape_html(run.repository_name()),
                escape_html(run.branch()),
                escape_html(run.commit_sha()),
                run.status().label(),
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>cid</title><style>body{{font-family: ui-sans-serif,system-ui,sans-serif;max-width:960px;margin:40px auto;padding:0 16px;background:#f6f2ea;color:#1d1b18}}main{{display:grid;grid-template-columns:1fr 1fr;gap:24px}}section{{background:#fff;padding:20px;border-radius:16px;box-shadow:0 10px 30px rgba(0,0,0,.08)}}code{{font-family: ui-monospace,monospace }}</style></head><body><h1>cid dashboard</h1><p>{} repos, {} runs</p><main><section><h2>Repositories</h2><ul>{}</ul></section><section><h2>Recent runs</h2><p>queued: {} | running: {} | passed: {} | failed: {} | canceled: {}</p><ul>{}</ul></section></main></body></html>",
        state.repositories().len(),
        summary.total_runs(),
        repositories,
        summary.queued_runs,
        summary.running_runs,
        summary.passed_runs,
        summary.failed_runs,
        summary.canceled_runs,
        runs
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::request_path;

    #[test]
    fn request_path_extracts_target_from_request_line() {
        assert_eq!(
            request_path("GET /api/runs HTTP/1.1\r\nHost: localhost\r\n"),
            "/api/runs"
        );
    }
}
