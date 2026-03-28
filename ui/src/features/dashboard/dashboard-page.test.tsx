import { screen, waitFor } from "@testing-library/react";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";

import { renderApp } from "../../lib/test/render";

describe("DashboardPage", () => {
  const fetchMock = vi.fn<typeof fetch>();

  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    fetchMock.mockReset();
  });

  it("renders repository and run data from the API", async () => {
    fetchMock.mockImplementation(async (input) => {
      const url = String(input);

      if (url === "/api/repositories") {
        return new Response(
          JSON.stringify([
            {
              id: 1,
              name: "cid",
              path: "/repos/cid",
              branch_rules: [{ branch: "main" }],
              pipeline: { image: "rust:1.85", steps: [], artifact_paths: [] },
              status: { last_seen_at_ms: 1_700_000_000_000, last_error: null },
            },
          ]),
        );
      }

      if (url === "/api/runs") {
        return new Response(
          JSON.stringify([
            {
              id: 12,
              repository_id: 1,
              repository_name: "cid",
              branch: "main",
              commit_sha: "abc123456789",
              status: "passed",
              queued_at_ms: 1_700_000_000_000,
              started_at_ms: 1_700_000_000_100,
              finished_at_ms: 1_700_000_000_500,
              steps: [],
              events: [],
            },
          ]),
        );
      }

      if (url === "/api/summary") {
        return new Response(
          JSON.stringify({
            total_runs: 1,
            queued_runs: 0,
            running_runs: 0,
            passed_runs: 1,
            failed_runs: 0,
            canceled_runs: 0,
          }),
        );
      }

      return new Response("not found", { status: 404 });
    });

    await renderApp("/");

    await waitFor(() =>
      expect(screen.getByText("Tracked repositories")).toBeInTheDocument(),
    );

    expect(screen.getByText("/repos/cid")).toBeInTheDocument();
    expect(screen.getByText("#12 cid")).toBeInTheDocument();
    expect(screen.getByText("passed")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try build again" })).toBeInTheDocument();
  });
});
