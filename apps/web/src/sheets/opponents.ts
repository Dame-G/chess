import type { Engine, LevelsOverview } from "../api/types";
import { formatNumber } from "../format";
import { levelChoices, levelSeat, type LevelChoice } from "./levels";

/** One engine to play as New game offers it: a row of the picker, with the seat it asks for. */
export interface OpponentChoice extends LevelChoice {
  /** The engine family it belongs to (`OpponentFamily.key`): the analysis engine, the human-like engines, or one engine. */
  family: string;
  /** The name of the engine it is a strength of. */
  engine: string;
  /** Not installed: shown, but not to be chosen. */
  disabled: boolean;
  /** Who it is in a few words, for the Start button: "Stockfish 5", "Stockfish", "Maia 1500". */
  label: string;
}

/** The family of the human-like engines (Maia): one family, a rating each. */
export const HUMAN = "human-like";

/** A row for an engine that plays as it is: no levels, its own name, a Maia's nominal rating or what it reports. */
function asIs(e: Engine, family: string): OpponentChoice {
  return {
    value: e.id,
    name: e.name,
    text: !e.installed
      ? "Not installed"
      : family === HUMAN
        ? ""
        : e.rating != null
          ? `≈ ${formatNumber(e.rating)}`
          : (e.id_name ?? ""),
    standing: "found",
    near: false,
    family,
    engine: e.name,
    disabled: !e.installed,
    label: e.name,
  };
}

/** An engine family as New game's first step offers it: the analysis engine, the human-like engines, or one engine. */
export interface OpponentFamily {
  key: string;
  name: string;
  /** What choosing it means, in a few words: "Levels 1 to 12 and full strength", "Plays like a person". */
  detail: string;
  choices: OpponentChoice[];
  /** Nothing in it can be chosen: it is not installed. */
  disabled: boolean;
}

/**
 * The families of the choices, in their order: the analysis engine (its levels are the strength step), the
 * human-like engines as "Maia" (its ratings are), then each other engine alone (no strength step).
 */
export function opponentFamilies(choices: readonly OpponentChoice[], analysis: string): OpponentFamily[] {
  const out: OpponentFamily[] = [];
  for (const c of choices) {
    let f = out.find((x) => x.key === c.family);
    if (!f) {
      f = { key: c.family, name: "", detail: "", choices: [], disabled: true };
      out.push(f);
    }
    f.choices.push(c);
    if (!c.disabled) f.disabled = false;
  }
  for (const f of out) {
    const first = f.choices[0]!;
    if (f.key === HUMAN) {
      f.name = "Maia";
      f.detail = "Plays like a person, at a rating";
    } else if (f.key === analysis && f.choices.length > 1) {
      f.name = first.engine;
      f.detail = "Levels, then full strength";
    } else {
      f.name = first.label;
      f.detail = first.disabled ? "Not installed" : "Plays as it is";
    }
  }
  return out;
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
    const ladder = main.installed && main.levels ? levelChoices(levels) : [];
    if (ladder.length)
      for (const c of ladder)
        out.push({
          ...c,
          value: levelSeat(main.id, c.value),
          name: c.value === "full" ? `${main.name} at full strength` : c.name,
          family: main.id,
          engine: main.name,
          disabled: false,
          label: c.value === "full" ? main.name : `${main.name} ${c.value}`,
        });
    else out.push(asIs(main, main.id));
  }
  const rest = engines.filter((e) => e.id !== analysis);
  const rated = rest.filter((e) => e.rating != null).toSorted((a, b) => a.rating! - b.rating!);
  for (const e of rated) out.push(asIs(e, HUMAN));
  for (const e of rest.filter((x) => x.rating == null)) out.push(asIs(e, e.id));
  return out;
}
