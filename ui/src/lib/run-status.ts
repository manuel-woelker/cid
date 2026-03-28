import type { RunStatus } from "./api/types";

export function statusColor(status: RunStatus): string {
  switch (status) {
    case "passed":
      return "success";
    case "failed":
      return "error";
    case "running":
      return "processing";
    case "queued":
      return "warning";
    case "canceled":
      return "default";
  }
}
