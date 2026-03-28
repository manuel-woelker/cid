import { screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { renderApp } from "../../lib/test/render";

describe("BranchPage", () => {
  const fetchMock = vi.fn<typeof fetch>();

  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    fetchMock.mockReset();
  });

  it("renders branch details from a deep link", async () => {
    fetchMock.mockImplementation(async (input) => {
      const url = String(input);

      if (url === "/api/repositories/1/branches/feature%2Fbeta") {
        return new Response(
          JSON.stringify({
            repository: {
              id: 1,
              name: "cid",
              path: "/repos/cid",
              branch_rules: [{ branch: "feature/beta" }],
              pipeline: { image: "rust:1.85", steps: [], artifact_paths: [] },
              status: { last_seen_at_ms: 1_700_000_000_000, last_error: null },
            },
            branch: {
              branch_name: "feature/beta",
              latest_run: {
                run_id: 7,
                status: "failed",
                commit_sha: "abcdef123456",
                queued_at_ms: 1_700_000_000_000,
                started_at_ms: 1_700_000_000_050,
                finished_at_ms: 1_700_000_001_250,
                activity_timestamp_ms: 1_700_000_001_250,
              },
              run_count: 1,
            },
            runs: [
              {
                id: 7,
                repository_id: 1,
                repository_name: "cid",
                branch: "feature/beta",
                commit_sha: "abcdef123456",
                status: "failed",
                queued_at_ms: 1_700_000_000_000,
                started_at_ms: 1_700_000_000_050,
                finished_at_ms: 1_700_000_001_250,
                steps: [],
                events: [],
              },
            ],
          }),
        );
      }

      return new Response("not found", { status: 404 });
    });

    await renderApp("/repositories/1/branches/feature%2Fbeta");

    await waitFor(() =>
      expect(screen.getByText("feature/beta")).toBeInTheDocument(),
    );

    expect(screen.getByText("Branch runs")).toBeInTheDocument();
    expect(screen.getByText("#7")).toBeInTheDocument();
    expect(screen.getAllByText("failed").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Try build again" })).toBeInTheDocument();
  });
});
