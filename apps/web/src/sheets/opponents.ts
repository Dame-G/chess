import type { Engine, LevelsOverview } from "../api/types";
import { formatNumber } from "../format";
import { levelChoices, levelSeat, type LevelChoice } from "./levels";

/** One engine to play as New game offers it: a row of the picker, with the seat it asks for. */
export interface OpponentChoice extends LevelChoice {
  /** The engine family it belongs to (`OpponentFamily.key`): the engine's id. */
  family: string;
  /** The name of the engine it is a strength of. */
  engine: string;
  /** Not installed: shown, but not to be chosen. */
  disabled: boolean;
  /** Who it is in a few words, for the Start button: "Stockfish 5", "Reckless", "Berserk". */
  label: string;
}

/** A row for an engine that plays as it is: no levels, its own name, the rating it was given or what it reports. */
function asIs(e: Engine): OpponentChoice {
  return {
    value: e.id,
    name: e.name,
    text: !e.installed ? "Not installed" : e.rating != null ? `≈ ${formatNumber(e.rating)}` : (e.id_name ?? ""),
    standing: "found",
    near: false,
    family: e.id,
    engine: e.name,
    disabled: !e.installed,
    label: e.name,
  };
}

/** An engine as New game's first step offers it: its levels (or the engine as it is) as the second step. */
export interface OpponentFamily {
  key: string;
  name: string;
  /** What choosing it means, in a few words: "Levels, then full strength", "Plays as it is". */
  detail: string;
  choices: OpponentChoice[];
  /** Nothing in it can be chosen: it is not installed. */
  disabled: boolean;
}

/**
 * The families of the choices, in their order: one per engine. An engine with levels has them as the strength step;
 * one without MultiPV says its levels are its node budgets alone.
 */
export function opponentFamilies(
  choices: readonly OpponentChoice[],
  engines: readonly Engine[] = [],
): OpponentFamily[] {
  const out: OpponentFamily[] = [];
  for (const c of choices) {
    let f = out.find((x) => x.key === c.family);
    if (!f) {
      f = { key: c.family, name: c.engine, detail: "", choices: [], disabled: true };
      out.push(f);
    }
    f.choices.push(c);
    if (!c.disabled) f.disabled = false;
  }
  for (const f of out) {
    const first = f.choices[0]!;
    if (f.choices.length > 1) {
      const plain = engines.find((e) => e.id === f.key)?.multipv === false;
      f.detail = plain ? "Levels by node budget alone, then full strength" : "Levels, then full strength";
    } else {
      f.name = first.label;
      f.detail = first.disabled ? "Not installed" : "Plays as it is";
    }
  }
  return out;
}

/**
 * Every engine you can play, in the order New game shows them: the analysis engine first, then the rest as listed
 * (built-ins, then yours). Each engine with levels offers them weakest first and full strength last (seats
 * `reckless@5`, `reckless`), with that engine's own ratings; an engine whose ladder is not read yet, or that plays a
 * fixed budget, is offered as it is. An engine that is not installed is listed but cannot be chosen; Settings installs
 * it.
 */
export function opponentChoices(
  engines: readonly Engine[],
  analysis: string,
  ladderOf: (engine: string) => LevelsOverview | null,
): OpponentChoice[] {
  const out: OpponentChoice[] = [];
  const ordered = [...engines.filter((e) => e.id === analysis), ...engines.filter((e) => e.id !== analysis)];
  for (const e of ordered) {
    const ladder = e.installed && e.levels ? levelChoices(ladderOf(e.id)) : [];
    if (!ladder.length) {
      out.push(asIs(e));
      continue;
    }
    for (const c of ladder)
      out.push({
        ...c,
        value: levelSeat(e.id, c.value),
        name: c.value === "full" ? `${e.name} at full strength` : c.name,
        family: e.id,
        engine: e.name,
        disabled: false,
        label: c.value === "full" ? e.name : `${e.name} ${c.value}`,
      });
  }
  return out;
}
