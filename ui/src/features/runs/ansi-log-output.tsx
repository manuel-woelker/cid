import type { CSSProperties, ReactNode } from "react";

type AnsiStyle = {
  bold: boolean;
  color: string | null;
};

type AnsiSegment = {
  style: AnsiStyle;
  text: string;
};

// eslint-disable-next-line no-control-regex
const ANSI_PATTERN = new RegExp("\\u001b\\[([0-9;]*)m", "g");

const BASIC_COLORS: Record<number, string> = {
  30: "#94a3b8",
  31: "#f87171",
  32: "#4ade80",
  33: "#facc15",
  34: "#60a5fa",
  35: "#f472b6",
  36: "#22d3ee",
  37: "#f8fafc",
  90: "#64748b",
  91: "#fb7185",
  92: "#86efac",
  93: "#fde047",
  94: "#93c5fd",
  95: "#f9a8d4",
  96: "#67e8f9",
  97: "#ffffff",
};

const DEFAULT_STYLE: AnsiStyle = {
  bold: false,
  color: null,
};

export function AnsiLogOutput({ text }: { text: string }) {
  return <>{renderAnsiText(text)}</>;
}

function renderAnsiText(text: string): ReactNode[] {
  const segments: AnsiSegment[] = [];
  let currentStyle = DEFAULT_STYLE;
  let cursor = 0;

  for (const match of text.matchAll(ANSI_PATTERN)) {
    const matchText = match[0];
    const matchIndex = match.index;

    if (matchIndex === undefined) {
      continue;
    }

    if (matchIndex > cursor) {
      segments.push({
        style: currentStyle,
        text: text.slice(cursor, matchIndex),
      });
    }

    currentStyle = applySgrCodes(currentStyle, parseSgrCodes(match[1] ?? ""));
    cursor = matchIndex + matchText.length;
  }

  if (cursor < text.length) {
    segments.push({
      style: currentStyle,
      text: text.slice(cursor),
    });
  }

  if (segments.length === 0) {
    return [text];
  }

  return segments.map((segment, index) => {
    const style = toCssStyle(segment.style);
    if (Object.keys(style).length === 0) {
      return segment.text;
    }

    return (
      <span key={`${index}-${segment.text.length}`} style={style}>
        {segment.text}
      </span>
    );
  });
}

function parseSgrCodes(rawCodes: string): number[] {
  if (rawCodes === "") {
    return [0];
  }

  return rawCodes
    .split(";")
    .map((value) => Number.parseInt(value, 10))
    .filter((value) => Number.isFinite(value));
}

function applySgrCodes(initialStyle: AnsiStyle, codes: number[]): AnsiStyle {
  let nextStyle = initialStyle;

  for (let index = 0; index < codes.length; index += 1) {
    const code = codes[index];

    if (code === 0) {
      nextStyle = DEFAULT_STYLE;
      continue;
    }

    if (code === 1) {
      nextStyle = { ...nextStyle, bold: true };
      continue;
    }

    if (code === 22) {
      nextStyle = { ...nextStyle, bold: false };
      continue;
    }

    if (code === 39) {
      nextStyle = { ...nextStyle, color: null };
      continue;
    }

    if (code in BASIC_COLORS) {
      nextStyle = { ...nextStyle, color: BASIC_COLORS[code] ?? null };
      continue;
    }

    if (code !== 38) {
      continue;
    }

    const mode = codes[index + 1];

    if (mode === 5) {
      const paletteIndex = codes[index + 2];
      if (paletteIndex !== undefined) {
        nextStyle = {
          ...nextStyle,
          color: ansi256Color(paletteIndex),
        };
      }
      index += 2;
      continue;
    }

    if (mode === 2) {
      const red = codes[index + 2];
      const green = codes[index + 3];
      const blue = codes[index + 4];

      if (red !== undefined && green !== undefined && blue !== undefined) {
        nextStyle = {
          ...nextStyle,
          color: `rgb(${red}, ${green}, ${blue})`,
        };
      }
      index += 4;
    }
  }

  return nextStyle;
}

function toCssStyle(style: AnsiStyle): CSSProperties {
  const cssStyle: CSSProperties = {};

  if (style.bold) {
    cssStyle.fontWeight = 700;
  }

  if (style.color !== null) {
    cssStyle.color = style.color;
  }

  return cssStyle;
}

function ansi256Color(index: number): string {
  if (index < 0) {
    return "#e2e8f0";
  }

  if (index < 16) {
    return (
      {
        0: "#0f172a",
        1: "#ef4444",
        2: "#22c55e",
        3: "#eab308",
        4: "#3b82f6",
        5: "#d946ef",
        6: "#06b6d4",
        7: "#e2e8f0",
        8: "#475569",
        9: "#f87171",
        10: "#4ade80",
        11: "#fde047",
        12: "#60a5fa",
        13: "#f0abfc",
        14: "#67e8f9",
        15: "#ffffff",
      }[index] ?? "#e2e8f0"
    );
  }

  if (index < 232) {
    const paletteIndex = index - 16;
    const blue = paletteIndex % 6;
    const green = Math.floor(paletteIndex / 6) % 6;
    const red = Math.floor(paletteIndex / 36);
    const toRgbChannel = (value: number) => (value === 0 ? 0 : value * 40 + 55);

    return `rgb(${toRgbChannel(red)}, ${toRgbChannel(green)}, ${toRgbChannel(blue)})`;
  }

  const grayscale = 8 + (index - 232) * 10;
  return `rgb(${grayscale}, ${grayscale}, ${grayscale})`;
}
