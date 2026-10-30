import type { ThemeCount, ThemeStat } from "../api/types";
import { formatNumber, plural } from "../format";

/** Win percentage lost, signed as a loss: `−2.4%`. Nothing lost reads `0%`. */
export function lost(points: number): string {
  const text = formatNumber(Math.abs(points), 1);
  return points > 0.05 ? `−${text}%` : "0%";
}

/** A share of the cost, 0 to 1, as a whole percentage: `34%`, and `<1%` for a sliver. */
export function shareText(share: number): string {
  const pct = share * 100;
  return pct > 0 && pct < 1 ? "<1%" : `${Math.round(pct)}%`;
}

/** The names a theme counted, most common first, with how many each: `a knight (12), a bishop (5)`. */
export function countedText(counts: readonly ThemeCount[]): string {
  return counts.map((c) => `${c.name} (${c.count})`).join(", ");
}

/** How often it happens: `38 mistakes in 31 games`. */
export function oftenText(t: Pick<ThemeStat, "count" | "games">): string {
  return `${plural(t.count, "mistake")} in ${plural(t.games, "game")}`;
}

/** The whole of it: every theme's mistakes and their cost per reviewed game, for the table's last row. */
export function totals(themes: readonly ThemeStat[]): { count: number; perGame: number } {
  return {
    count: themes.reduce((n, t) => n + t.count, 0),
    perGame: themes.reduce((n, t) => n + t.per_game, 0),
  };
}

/** The cause a theme names, or the opening it came in: the second is the point of an opening error. */
export function namedBy(
  t: Pick<ThemeStat, "kind" | "causes" | "openings">,
): { label: string; items: ThemeCount[] } | null {
  if (t.kind === "opening_error" && t.openings.length) return { label: "Mostly in", items: t.openings };
  if (t.causes.length) return { label: "Most often", items: t.causes };
  return null;
}
