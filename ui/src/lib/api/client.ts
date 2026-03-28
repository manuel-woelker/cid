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

export async function getSummary(): Promise<Summary> {
  return requestJson<Summary>("/api/summary");
}
