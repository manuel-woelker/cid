import { screen, waitFor } from "@testing-library/react";
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

    await renderApp("/repositories/1/runs/7");

    await waitFor(() =>
      expect(screen.getByText("Run #7 · cid")).toBeInTheDocument(),
    );

    expect(screen.getByText("Log output")).toBeInTheDocument();
    expect(screen.getAllByText("cargo test").length).toBeGreaterThan(0);
    expect(screen.getByText("101")).toBeInTheDocument();
    expect(await screen.findByText("test log output")).toBeInTheDocument();
    expect(screen.getAllByText("failed").length).toBeGreaterThan(0);
  });
});
