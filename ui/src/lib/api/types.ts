export type RunStatus = "queued" | "running" | "passed" | "failed" | "canceled";

export interface BranchRule {
  branch: string;
}

export interface PipelineStep {
  name: string;
  command: string;
}

export interface Pipeline {
  image: string;
  steps: PipelineStep[];
  artifact_paths: string[];
}

export interface RepositoryStatus {
  last_seen_at_ms: number | null;
  last_error: string | null;
}

export interface Repository {
  id: number;
  name: string;
  path: string;
  branch_rules: BranchRule[];
  pipeline: Pipeline;
  status: RepositoryStatus;
}

export interface RunStep {
  name: string;
  command: string;
  image: string;
  status: RunStatus;
  exit_code: number | null;
  started_at_ms: number | null;
  finished_at_ms: number | null;
  duration_ms: number | null;
  log_path: string | null;
  artifact_paths: string[];
}

export interface RunEvent {
  timestamp_ms: number;
  message: string;
}

export interface Run {
  id: number;
  repository_id: number;
  repository_name: string;
  branch: string;
  commit_sha: string;
  status: RunStatus;
  queued_at_ms: number;
  started_at_ms: number | null;
  finished_at_ms: number | null;
  steps: RunStep[];
  events: RunEvent[];
}

export interface BranchLatestRun {
  run_id: number;
  status: RunStatus;
  commit_sha: string;
  queued_at_ms: number;
  started_at_ms: number | null;
  finished_at_ms: number | null;
  activity_timestamp_ms: number;
}

export interface BranchSummary {
  branch_name: string;
  latest_run: BranchLatestRun | null;
  run_count: number;
}

export interface BranchDetail {
  repository: Repository;
  branch: BranchSummary;
  runs: Run[];
}

export interface Summary {
  total_runs: number;
  queued_runs: number;
  running_runs: number;
  passed_runs: number;
  failed_runs: number;
  canceled_runs: number;
}
