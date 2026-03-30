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

export async function getRepository(
  repositoryName: string,
): Promise<Repository> {
  return requestJson<Repository>(
    `/api/repositories/${encodeURIComponent(repositoryName)}`,
  );
}

export async function getRepositoryBranches(
  repositoryName: string,
): Promise<BranchSummary[]> {
  return requestJson<BranchSummary[]>(
    `/api/repositories/${encodeURIComponent(repositoryName)}/branches`,
  );
}

export async function getRepositoryBranch(
  repositoryName: string,
  branchName: string,
): Promise<BranchDetail> {
  return requestJson<BranchDetail>(
    `/api/repositories/${encodeURIComponent(repositoryName)}/branches/${encodeURIComponent(branchName)}`,
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
