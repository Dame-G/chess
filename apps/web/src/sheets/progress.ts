import type { Metric, ProgressChart, Series } from "../api/types";

/** One line of the progress chart, on a shared time axis. */
export interface ProgressLine {
  metric: Metric;
  player: string;
  label: string;
  /** Values at each x (Unix seconds), `null` where this line has no point. */
  values: (number | null)[];
  /** Which scale: Dameless's Elo over v1 is a small number on its own scale; the rest share the rating scale. */
  scale: "rating" | "elo";
}

export interface ProgressData {
  x: number[];
  lines: ProgressLine[];
}

const seconds = (iso: string) => Math.round(new Date(iso).getTime() / 1000);

const LABEL: Record<Metric, string> = {
  rating: "Your rating",
  accuracy: "Your accuracy",
  drill_hit_rate: "Your drill hit rate",
  elo: "Dameless",
};

/**
 * Dameless's Elo is on the human scale once the levels anchor it (about 2000 and more), else it is Elo over v1, a
 * small number that cannot share an axis with ratings.
 */
function eloOnRatingScale(s: Series): boolean {
  return s.points.some((p) => p.value > 800);
}

/**
 * The series of a progress answer on one time axis, for the side-by-side chart: your rating, your accuracy (and the
 * drill hit rate as a percentage) and Dameless's Elo. A series of fewer than two points is not a line and is left out. Accuracy and the hit rate
 * are scaled to 0 to 100 so they read as percentages.
 */
export function progressData(chart: ProgressChart, metrics: readonly Metric[] = ["rating", "elo"]): ProgressData {
  const chosen = chart.series.filter((s) => s.points.length > 1 && metrics.includes(s.metric));
  const x = [...new Set(chosen.flatMap((s) => s.points.map((p) => seconds(p.at))))].sort((a, b) => a - b);
  const index = new Map(x.map((t, i) => [t, i]));
  const lines = chosen.map((s): ProgressLine => {
    const values: (number | null)[] = x.map(() => null);
    const factor = s.metric === "drill_hit_rate" ? 100 : 1;
    for (const p of s.points) values[index.get(seconds(p.at))!] = p.value * factor;
    return {
      metric: s.metric,
      player: s.player,
      label: LABEL[s.metric],
      values,
      scale: s.metric === "elo" && !eloOnRatingScale(s) ? "elo" : "rating",
    };
  });
  return { x, lines };
}

/** The last figure of a line, for the caption. */
export function latest(line: ProgressLine): number | null {
  for (let i = line.values.length - 1; i >= 0; i--) if (line.values[i] != null) return line.values[i]!;
  return null;
}
