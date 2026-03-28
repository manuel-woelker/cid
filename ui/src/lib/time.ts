export function formatTimestamp(timestampMs: number | null): string {
  if (timestampMs === null) {
    return "Not yet";
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(timestampMs));
}

export function formatDuration(durationMs: number | null): string {
  if (durationMs === null) {
    return "In progress";
  }

  if (durationMs < 1000) {
    return `${durationMs} ms`;
  }

  const seconds = durationMs / 1000;
  if (seconds < 60) {
    return `${seconds.toFixed(1)} s`;
  }

  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.round(seconds % 60);
  return `${minutes}m ${remainingSeconds}s`;
}

export function shortCommit(commitSha: string): string {
  return commitSha.slice(0, 8);
}
