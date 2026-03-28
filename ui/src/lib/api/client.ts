import type {
  BranchDetail,
  BranchSummary,
  Repository,
  Run,
  Summary,
} from "./types";

async function requestJson<T>(path: string): Promise<T> {
  const response = await fetch(path, {
    headers: {
      Accept: "application/json",
    },
  });

  if (!response.ok) {
    throw new Error(`request failed for ${path}: ${response.status}`);
  }

  return (await response.json()) as T;
}

async function requestText(path: string): Promise<string> {
  const response = await fetch(path, {
    headers: {
      Accept: "text/plain",
    },
  });

  if (!response.ok) {
    throw new Error(`request failed for ${path}: ${response.status}`);
  }

  return response.text();
}

async function postJson<T>(path: string): Promise<T> {
  const response = await fetch(path, {
    method: "POST",
    headers: {
      Accept: "application/json",
    },
  });

  if (!response.ok) {
    throw new Error(`request failed for ${path}: ${response.status}`);
  }

  return (await response.json()) as T;
}

export async function getRepositories(): Promise<Repository[]> {
  return requestJson<Repository[]>("/api/repositories");
}

export async function getRepository(repositoryId: string): Promise<Repository> {
  return requestJson<Repository>(`/api/repositories/${repositoryId}`);
}

export async function getRepositoryBranches(
  repositoryId: string,
): Promise<BranchSummary[]> {
  return requestJson<BranchSummary[]>(
    `/api/repositories/${repositoryId}/branches`,
  );
}

export async function getRepositoryBranch(
  repositoryId: string,
  branchName: string,
): Promise<BranchDetail> {
  return requestJson<BranchDetail>(
    `/api/repositories/${repositoryId}/branches/${encodeURIComponent(branchName)}`,
  );
}

export async function getRuns(): Promise<Run[]> {
  return requestJson<Run[]>("/api/runs");
}

export async function getRun(runId: string): Promise<Run> {
  return requestJson<Run>(`/api/runs/${runId}`);
}

export async function replayRun(runId: string): Promise<Run> {
  return postJson<Run>(`/api/runs/${runId}/replay`);
}

export async function getRunStepLog(
  runId: string,
  stepIndex: number,
): Promise<string> {
  return requestText(`/api/runs/${runId}/steps/${stepIndex}/log`);
}

export async function getSummary(): Promise<Summary> {
  return requestJson<Summary>("/api/summary");
}
