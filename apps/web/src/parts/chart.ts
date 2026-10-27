/** What a `LineChart` draws. */
export interface ChartSeries {
  label: string;
  values: (number | null)[];
  /** A colour token (`--e-primary`). */
  color: string;
  /** The right-hand scale instead of the left. */
  right?: boolean;
  kind?: "line" | "step" | "points";
  width?: number;
  dash?: number[];
  /** A shaded band around the line. */
  band?: { low: (number | null)[]; high: (number | null)[] };
}

export interface ReferenceLine {
  value: number;
  color: string;
  dash?: number[];
}
