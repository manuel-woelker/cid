import { describe, expect, it } from "vitest";

import { describeTimestamp } from "./time";

describe("describeTimestamp", () => {
  const timeZone = "UTC";
  const nowMs = Date.UTC(2026, 2, 30, 14, 0, 0, 0);

  it("renders null timestamps as not yet without a tooltip", () => {
    expect(describeTimestamp(null, { nowMs, timeZone })).toEqual({
      display: "Not yet",
      tooltip: null,
    });
  });

  it("humanizes timestamps from the last minute as one minute ago", () => {
    expect(
      describeTimestamp(nowMs - 30 * 1000, { nowMs, timeZone }),
    ).toMatchObject({
      display: "1 minute ago",
    });
  });

  it("humanizes recent timestamps as minutes ago", () => {
    expect(
      describeTimestamp(nowMs - 23 * 60 * 1000, { nowMs, timeZone }),
    ).toMatchObject({
      display: "23 minutes ago",
    });
  });

  it("renders same-day timestamps as today at 24h time", () => {
    expect(
      describeTimestamp(Date.UTC(2026, 2, 30, 12, 52, 0, 0), {
        nowMs,
        timeZone,
      }),
    ).toMatchObject({
      display: "today at 12:52",
    });
  });

  it("renders previous-day timestamps as yesterday at 24h time", () => {
    expect(
      describeTimestamp(Date.UTC(2026, 2, 29, 19, 8, 0, 0), {
        nowMs,
        timeZone,
      }),
    ).toMatchObject({
      display: "yesterday at 19:08",
    });
  });

  it("falls back to absolute formatting for timestamps older than 48 hours", () => {
    expect(
      describeTimestamp(Date.UTC(2026, 2, 28, 11, 4, 0, 0), {
        nowMs,
        timeZone,
      }),
    ).toMatchObject({
      display: "2026-03-28 11:04",
    });
  });

  it("always formats the tooltip with seconds and milliseconds", () => {
    expect(
      describeTimestamp(Date.UTC(2026, 2, 30, 12, 52, 4, 321), {
        nowMs,
        timeZone,
      }),
    ).toEqual({
      display: "today at 12:52",
      tooltip: "2026-03-30 12:52:04.321",
    });
  });
});
