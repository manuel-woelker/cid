const MS_PER_MINUTE = 60 * 1000;
const MS_PER_HOUR = 60 * MS_PER_MINUTE;
const MS_PER_DAY = 24 * MS_PER_HOUR;

interface TimestampParts {
  year: string;
  month: string;
  day: string;
  hour: string;
  minute: string;
  second: string;
}

export interface FormattedTimestamp {
  display: string;
  tooltip: string | null;
}

export interface FormatTimestampOptions {
  nowMs?: number;
  timeZone?: string;
}

function resolvedTimeZone(timeZone?: string): string {
  return timeZone ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
}

function timestampFormatter(timeZone: string) {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  });
}

function getTimestampParts(date: Date, timeZone: string): TimestampParts {
  const parts = timestampFormatter(timeZone).formatToParts(date);

  function getPart(type: Intl.DateTimeFormatPartTypes): string {
    return parts.find((part) => part.type === type)?.value ?? "00";
  }

  return {
    year: getPart("year"),
    month: getPart("month"),
    day: getPart("day"),
    hour: getPart("hour"),
    minute: getPart("minute"),
    second: getPart("second"),
  };
}

function dayIndex(parts: TimestampParts): number {
  return (
    Date.UTC(Number(parts.year), Number(parts.month) - 1, Number(parts.day)) /
    MS_PER_DAY
  );
}

function formatClock(parts: TimestampParts): string {
  return `${parts.hour}:${parts.minute}`;
}

function formatDateTime(parts: TimestampParts): string {
  return `${parts.year}-${parts.month}-${parts.day} ${formatClock(parts)}`;
}

function formatTooltipTimestamp(
  parts: TimestampParts,
  milliseconds: number,
): string {
  return `${parts.year}-${parts.month}-${parts.day} ${parts.hour}:${parts.minute}:${parts.second}.${String(milliseconds).padStart(3, "0")}`;
}

export function describeTimestamp(
  timestampMs: number | null,
  options: FormatTimestampOptions = {},
): FormattedTimestamp {
  if (timestampMs === null) {
    return {
      display: "Not yet",
      tooltip: null,
    };
  }

  const timeZone = resolvedTimeZone(options.timeZone);
  const nowMs = options.nowMs ?? Date.now();
  const timestampDate = new Date(timestampMs);
  const timestampParts = getTimestampParts(timestampDate, timeZone);
  const tooltip = formatTooltipTimestamp(
    timestampParts,
    timestampDate.getMilliseconds(),
  );

  const ageMs = nowMs - timestampMs;
  if (ageMs < 0 || ageMs >= 48 * MS_PER_HOUR) {
    return {
      display: formatDateTime(timestampParts),
      tooltip,
    };
  }

  if (ageMs < MS_PER_HOUR) {
    const minuteCount = Math.max(1, Math.floor(ageMs / MS_PER_MINUTE));
    return {
      display: `${minuteCount} minute${minuteCount === 1 ? "" : "s"} ago`,
      tooltip,
    };
  }

  const nowParts = getTimestampParts(new Date(nowMs), timeZone);
  const dayDifference = dayIndex(nowParts) - dayIndex(timestampParts);

  if (dayDifference === 0) {
    return {
      display: `today at ${formatClock(timestampParts)}`,
      tooltip,
    };
  }

  if (dayDifference === 1) {
    return {
      display: `yesterday at ${formatClock(timestampParts)}`,
      tooltip,
    };
  }

  return {
    display: `${dayDifference} days ago at ${formatClock(timestampParts)}`,
    tooltip,
  };
}

export function formatTimestamp(
  timestampMs: number | null,
  options?: FormatTimestampOptions,
): string {
  return describeTimestamp(timestampMs, options).display;
}

export function formatTimestampTooltip(
  timestampMs: number | null,
  options?: FormatTimestampOptions,
): string | null {
  return describeTimestamp(timestampMs, options).tooltip;
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
