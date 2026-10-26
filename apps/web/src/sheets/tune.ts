/** Seconds one scaled short game takes, wall clock, on one core. A rough figure, enough to say "hours" or "minutes". */
const SECONDS_PER_GAME = 25;

export interface TuneCost {
  /** Games the tune plays: two per pair. */
  games: number;
  /** Hours of wall time on `cores` cores. */
  hours: number;
}

/** What a tune costs: `iterations` of `pairs` game pairs, played `cores` at a time. */
export function tuneCost(iterations: number, pairs: number, cores: number): TuneCost {
  const games = iterations * pairs * 2;
  return { games, hours: (games * SECONDS_PER_GAME) / Math.max(1, cores) / 3600 };
}

/** The cost as a sentence: "16,000 games, about 7 hours on this machine." */
export function tuneCostText(cost: TuneCost): string {
  const when =
    cost.hours < 1
      ? `about ${Math.max(5, Math.round((cost.hours * 60) / 5) * 5)} minutes`
      : `about ${cost.hours < 10 ? cost.hours.toFixed(1).replace(/\.0$/, "") : Math.round(cost.hours)} hours`;
  const note =
    cost.games < 60_000
      ? " Reckless's own tunes play 95,000 games or more, so this shows the idea works, not a finished tune."
      : "";
  return `${cost.games.toLocaleString("en-GB")} games, ${when} on this machine.${note}`;
}
