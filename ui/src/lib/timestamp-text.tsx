import { Tooltip } from "antd";

import { describeTimestamp } from "./time";

interface TimestampTextProps {
  timestampMs: number | null;
}

export function TimestampText({ timestampMs }: TimestampTextProps) {
  const { display, tooltip } = describeTimestamp(timestampMs);

  if (tooltip === null) {
    return display;
  }

  return (
    <Tooltip title={tooltip}>
      <span>{display}</span>
    </Tooltip>
  );
}
