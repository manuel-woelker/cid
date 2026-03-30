import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { renderApp } from "../../lib/test/render";

describe("RunDetailPage", () => {
  const fetchMock = vi.fn<typeof fetch>();

  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    fetchMock.mockReset();
  });

  it("renders run details for the selected run", async () => {
    fetchMock.mockImplementation(async (input) => {
      const url = String(input);

      if (url === "/api/runs/7") {
        return new Response(
          JSON.stringify({
            id: 7,
            repository_id: 1,
            repository_name: "cid",
            branch: "main",
            commit_sha: "abcdef123456",
            status: "failed",
            queued_at_ms: 1_700_000_000_000,
            started_at_ms: 1_700_000_000_050,
            finished_at_ms: 1_700_000_001_250,
            steps: [
              {
                name: "test",
                command: "cargo test",
                image: "rust:1.85",
                status: "failed",
                exit_code: 101,
                started_at_ms: 1_700_000_000_050,
                finished_at_ms: 1_700_000_001_250,
                duration_ms: 1200,
                log_path: ".cid/repositories/cid/runs/run-7/step-0.log",
                artifact_paths: [],
              },
            ],
            events: [
              { timestamp_ms: 1_700_000_000_000, message: "run queued" },
              {
                timestamp_ms: 1_700_000_001_250,
                message: "run finished with status failed",
              },
            ],
          }),
        );
      }

      if (url === "/api/runs/7/steps/0/log") {
        return new Response("test log output");
      }

      return new Response("not found", { status: 404 });
    });

    await renderApp("/repositories/cid/runs/7");

    await waitFor(() =>
      expect(screen.getByText("Run #7 · cid")).toBeInTheDocument(),
    );

    expect(screen.getByText("Log output")).toBeInTheDocument();
    expect(screen.getAllByText("cargo test").length).toBeGreaterThan(0);
    expect(screen.getByText("101")).toBeInTheDocument();
    expect(await screen.findByText("test log output")).toBeInTheDocument();
    expect(screen.getAllByText("failed").length).toBeGreaterThan(0);
  });

  it("defaults log output to the last step with a log", async () => {
    fetchMock.mockImplementation(async (input) => {
      const url = String(input);

      if (url === "/api/runs/7") {
        return new Response(
          JSON.stringify({
            id: 7,
            repository_id: 1,
            repository_name: "cid",
            branch: "main",
            commit_sha: "abcdef123456",
            status: "passed",
            queued_at_ms: 1_700_000_000_000,
            started_at_ms: 1_700_000_000_050,
            finished_at_ms: 1_700_000_001_250,
            steps: [
              {
                name: "build devcontainer",
                command: "devcontainer build --workspace-folder .",
                image: "devcontainer",
                status: "passed",
                exit_code: 0,
                started_at_ms: 1_700_000_000_050,
                finished_at_ms: 1_700_000_000_450,
                duration_ms: 400,
                log_path: ".cid/repositories/cid/runs/run-7/step-0.log",
                artifact_paths: [],
              },
              {
                name: "run ci script",
                command: "./scripts/ci.sh",
                image: "devcontainer",
                status: "passed",
                exit_code: 0,
                started_at_ms: 1_700_000_000_500,
                finished_at_ms: 1_700_000_001_250,
                duration_ms: 750,
                log_path: ".cid/repositories/cid/runs/run-7/step-1.log",
                artifact_paths: [],
              },
            ],
            events: [
              { timestamp_ms: 1_700_000_000_000, message: "run queued" },
              {
                timestamp_ms: 1_700_000_001_250,
                message: "run finished with status passed",
              },
            ],
          }),
        );
      }

      if (url === "/api/runs/7/steps/0/log") {
        return new Response("devcontainer build output");
      }

      if (url === "/api/runs/7/steps/1/log") {
        return new Response("ci script output");
      }

      return new Response("not found", { status: 404 });
    });

    await renderApp("/repositories/cid/runs/7");

    await waitFor(() =>
      expect(screen.getByText("Run #7 · cid")).toBeInTheDocument(),
    );

    expect(await screen.findByText("ci script output")).toBeInTheDocument();
    expect(
      screen.queryByText("devcontainer build output"),
    ).not.toBeInTheDocument();
  });

  it("renders ansi colors and bold styles in log output", async () => {
    fetchMock.mockImplementation(async (input) => {
      const url = String(input);

      if (url === "/api/runs/7") {
        return new Response(
          JSON.stringify({
            id: 7,
            repository_id: 1,
            repository_name: "cid",
            branch: "main",
            commit_sha: "abcdef123456",
            status: "failed",
            queued_at_ms: 1_700_000_000_000,
            started_at_ms: 1_700_000_000_050,
            finished_at_ms: 1_700_000_001_250,
            steps: [
              {
                name: "ci",
                command: "./scripts/ci.sh",
                image: "devcontainer",
                status: "failed",
                exit_code: 1,
                started_at_ms: 1_700_000_000_050,
                finished_at_ms: 1_700_000_001_250,
                duration_ms: 1200,
                log_path: ".cid/repositories/cid/runs/run-7/step-0.log",
                artifact_paths: [],
              },
            ],
            events: [],
          }),
        );
      }

      if (url === "/api/runs/7/steps/0/log") {
        return new Response(
          "\u001b[1;31mFAIL\u001b[0m plain \u001b[38;2;12;34;56mcustom\u001b[0m",
        );
      }

      return new Response("not found", { status: 404 });
    });

    await renderApp("/repositories/cid/runs/7");

    await waitFor(() =>
      expect(screen.getByText("Run #7 · cid")).toBeInTheDocument(),
    );

    const failText = await screen.findByText("FAIL", { selector: "span" });
    const customText = screen.getByText("custom", { selector: "span" });

    expect(failText.getAttribute("style")).toContain("font-weight: 700");
    expect(failText.getAttribute("style")).toContain(
      "color: rgb(248, 113, 113)",
    );
    expect(customText.getAttribute("style")).toContain(
      "color: rgb(12, 34, 56)",
    );
    expect(screen.getByText(/plain/)).toBeInTheDocument();
  });

  it("replays the run and navigates to the new queued run", async () => {
    fetchMock.mockImplementation(async (input, init) => {
      const url = String(input);

      if (url === "/api/runs/7") {
        return new Response(
          JSON.stringify({
            id: 7,
            repository_id: 1,
            repository_name: "cid",
            branch: "main",
            commit_sha: "abcdef123456",
            status: "failed",
            queued_at_ms: 1_700_000_000_000,
            started_at_ms: 1_700_000_000_050,
            finished_at_ms: 1_700_000_001_250,
            steps: [
              {
                name: "test",
                command: "cargo test",
                image: "rust:1.85",
                status: "failed",
                exit_code: 101,
                started_at_ms: 1_700_000_000_050,
                finished_at_ms: 1_700_000_001_250,
                duration_ms: 1200,
                log_path: ".cid/repositories/cid/runs/run-7/step-0.log",
                artifact_paths: [],
              },
            ],
            events: [],
          }),
        );
      }

      if (url === "/api/runs/7/steps/0/log") {
        return new Response("test log output");
      }

      if (url === "/api/runs/7/replay" && init?.method === "POST") {
        return new Response(
          JSON.stringify({
            id: 8,
            repository_id: 1,
            repository_name: "cid",
            branch: "main",
            commit_sha: "abcdef123456",
            status: "queued",
            queued_at_ms: 1_700_000_002_000,
            started_at_ms: null,
            finished_at_ms: null,
            steps: [
              {
                name: "test",
                command: "cargo test",
                image: "rust:1.85",
                status: "queued",
                exit_code: null,
                started_at_ms: null,
                finished_at_ms: null,
                duration_ms: null,
                log_path: null,
                artifact_paths: [],
              },
            ],
            events: [
              { timestamp_ms: 1_700_000_002_000, message: "run queued" },
            ],
          }),
        );
      }

      if (url === "/api/runs/8") {
        return new Response(
          JSON.stringify({
            id: 8,
            repository_id: 1,
            repository_name: "cid",
            branch: "main",
            commit_sha: "abcdef123456",
            status: "queued",
            queued_at_ms: 1_700_000_002_000,
            started_at_ms: null,
            finished_at_ms: null,
            steps: [
              {
                name: "test",
                command: "cargo test",
                image: "rust:1.85",
                status: "queued",
                exit_code: null,
                started_at_ms: null,
                finished_at_ms: null,
                duration_ms: null,
                log_path: null,
                artifact_paths: [],
              },
            ],
            events: [
              { timestamp_ms: 1_700_000_002_000, message: "run queued" },
            ],
          }),
        );
      }

      return new Response("not found", { status: 404 });
    });

    await renderApp("/repositories/cid/runs/7");

    await waitFor(() =>
      expect(screen.getByText("Run #7 · cid")).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: "Try build again" }));

    await waitFor(() =>
      expect(screen.getByText("Run #8 · cid")).toBeInTheDocument(),
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/runs/7/replay",
      expect.objectContaining({ method: "POST" }),
    );
  });
});
