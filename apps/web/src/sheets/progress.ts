import type { Metric, ProgressChart } from "../api/types";

/** One line of the progress chart, on a shared time axis. */
export interface ProgressLine {
  metric: Metric;
  player: string;
  label: string;
  /** Values at each x (Unix seconds), `null` where this line has no point. */
  values: (number | null)[];
}

export interface ProgressData {
  x: number[];
  lines: ProgressLine[];
}

const seconds = (iso: string) => Math.round(new Date(iso).getTime() / 1000);

/** The lines the chart draws, each with its name: yours. Any other series is left out. */
const LABEL: Record<string, string> = {
  rating: "Your rating",
  accuracy: "Your accuracy",
  drill_hit_rate: "Your drill hit rate",
};

/**
 * The series of a progress answer on one time axis: your rating, or your accuracy and the drill hit rate. A series of
 * fewer than two points is not a line and is left out. The hit rate is scaled to 0 to 100 so it reads as a percentage.
 */
export function progressData(chart: ProgressChart, metrics: readonly Metric[] = ["rating"]): ProgressData {
  const chosen = chart.series.filter((s) => s.points.length > 1 && metrics.includes(s.metric) && LABEL[s.metric]);
  const x = [...new Set(chosen.flatMap((s) => s.points.map((p) => seconds(p.at))))].sort((a, b) => a - b);
  const index = new Map(x.map((t, i) => [t, i]));
  const lines = chosen.map((s): ProgressLine => {
    const values: (number | null)[] = x.map(() => null);
    const factor = s.metric === "drill_hit_rate" ? 100 : 1;
    for (const p of s.points) values[index.get(seconds(p.at))!] = p.value * factor;
    return { metric: s.metric, player: s.player, label: LABEL[s.metric]!, values };
  });
  return { x, lines };
}

/** The last figure of a line, for the caption. */
export function latest(line: ProgressLine): number | null {
  for (let i = line.values.length - 1; i >= 0; i--) if (line.values[i] != null) return line.values[i]!;
  return null;
}
