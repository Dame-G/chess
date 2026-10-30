import type { Engine, LevelsOverview } from "../api/types";
import { formatNumber } from "../format";
import { levelChoices, levelSeat, type LevelChoice } from "./levels";

/** One engine to play as New game offers it: a row of the picker, under a group, with the seat it asks for. */
export interface OpponentChoice extends LevelChoice {
  /** The heading the row sits under: the analysis engine's levels, Human-like, Engines. */
  group: string;
  /** Not installed: shown, but not to be chosen. */
  disabled: boolean;
  /** Who it is in a sentence: "Stockfish level 5", "Stockfish", "Maia 1500". */
  label: string;
}

/** A row for an engine that plays as it is: no levels, its own name, a Maia's nominal rating or what it reports. */
function asIs(e: Engine, group: string): OpponentChoice {
  return {
    value: e.id,
    name: e.name,
    text: !e.installed ? "Not installed" : e.rating != null ? `≈ ${formatNumber(e.rating)}` : (e.id_name ?? ""),
    standing: "found",
    near: false,
    group,
    disabled: !e.installed,
    label: e.name,
  };
}

/**
 * Every engine you can play, in the order New game shows them: the analysis engine's levels weakest first and at full
 * strength last (seats `stockfish@5`, `stockfish`), then the human-like engines that have a rating (Maia), weakest
 * first, then every other engine. An engine that is not installed is listed but cannot be chosen; Settings installs it.
 */
export function opponentChoices(
  engines: readonly Engine[],
  analysis: string,
  levels: LevelsOverview | null,
): OpponentChoice[] {
  const out: OpponentChoice[] = [];
  const main = engines.find((e) => e.id === analysis);
  if (main) {
    const group = `${main.name} levels`;
    const ladder = main.installed && main.levels ? levelChoices(levels) : [];
    if (ladder.length)
      for (const c of ladder)
        out.push({
          ...c,
          value: levelSeat(main.id, c.value),
          name: c.value === "full" ? `${main.name} at full strength` : c.name,
          group,
          disabled: false,
          label: c.value === "full" ? main.name : `${main.name} ${c.name.toLowerCase()}`,
        });
    else out.push(asIs(main, group));
  }
  const rest = engines.filter((e) => e.id !== analysis);
  const rated = rest.filter((e) => e.rating != null).toSorted((a, b) => a.rating! - b.rating!);
  for (const e of rated) out.push(asIs(e, "Human-like"));
  for (const e of rest.filter((x) => x.rating == null)) out.push(asIs(e, "Engines"));
  return out;
}
