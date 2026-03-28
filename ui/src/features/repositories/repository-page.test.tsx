import { screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { renderApp } from "../../lib/test/render";

describe("RepositoryPage", () => {
  const fetchMock = vi.fn<typeof fetch>();

  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    fetchMock.mockReset();
  });

  it("renders branches in latest-build order with status", async () => {
    fetchMock.mockImplementation(async (input) => {
      const url = String(input);

      if (url === "/api/repositories/1") {
        return new Response(
          JSON.stringify({
            id: 1,
            name: "cid",
            path: "/repos/cid",
            branch_rules: [{ branch: "main" }, { branch: "release" }],
            pipeline: { image: "rust:1.85", steps: [], artifact_paths: [] },
            status: { last_seen_at_ms: 1_700_000_000_000, last_error: null },
          }),
        );
      }

      if (url === "/api/repositories/1/branches") {
        return new Response(
          JSON.stringify([
            {
              branch_name: "main",
              latest_run: {
                run_id: 3,
                status: "passed",
                commit_sha: "abc123456789",
                queued_at_ms: 1_700_000_000_000,
                started_at_ms: 1_700_000_000_050,
                finished_at_ms: 1_700_000_000_300,
                activity_timestamp_ms: 1_700_000_000_300,
              },
              run_count: 2,
            },
            {
              branch_name: "release",
              latest_run: null,
              run_count: 0,
            },
          ]),
        );
      }

      return new Response("not found", { status: 404 });
    });

    await renderApp("/repositories/1");

    await waitFor(() => expect(screen.getByText("cid")).toBeInTheDocument());

    const branchLinks = screen.getAllByRole("link", { name: /main|release/ });
    expect(branchLinks[0]).toHaveTextContent("main");
    expect(branchLinks[1]).toHaveTextContent("release");
    expect(screen.getByText("passed")).toBeInTheDocument();
    expect(screen.getByText("not built")).toBeInTheDocument();
  });
});
