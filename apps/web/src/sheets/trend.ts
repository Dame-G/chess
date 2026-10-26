import type { TrendPoint } from "../api/types";

export interface TrendSeries {
  /** Unix seconds. */
  x: number[];
  accuracy: (number | null)[];
  /** The mean accuracy of the last `window` games that have one. */
  average: (number | null)[];
  rating: (number | null)[];
}

/** A profile's trend as chart columns: accuracy per game, its rolling average, and the rating. */
export function trendSeries(points: readonly TrendPoint[], window = 10): TrendSeries {
  const accuracy = points.map((p) => p.accuracy);
  const average = accuracy.map((_, i) => {
    const recent = accuracy.slice(Math.max(0, i - window + 1), i + 1).filter((a): a is number => a !== null);
    return recent.length ? Math.round((10 * recent.reduce((a, b) => a + b, 0)) / recent.length) / 10 : null;
  });
  return {
    x: points.map((p) => Math.floor(new Date(p.played_at).getTime() / 1000)),
    accuracy,
    average,
    rating: points.map((p) => p.rating),
  };
}
