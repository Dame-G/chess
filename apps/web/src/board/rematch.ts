import type { Game, NewGameRequest, Seat, StartFrom } from "../api/types";
import { engineOf } from "../format";
import { levelSeat } from "../sheets/levels";

/** What a rematch needs to know that the game itself does not say. */
export interface RematchContext {
  /** Whether an engine (by id) has levels: every engine on the clock does, one with a fixed budget does not. */
  hasLevels: (engine: string) => boolean;
}

/** A strength of an engine: a level of its ladder, or `full`. */
export type Strength = number | "full";

/** Whether a finished game is one of yours against an engine played here: the games a rematch is offered for. */
function yours(g: Pick<Game, "summary">): { them: Seat; engine: string; you: "white" | "black" } | null {
  const s = g.summary;
  if (s.source.type !== "local" || !s.you || (s.status !== "finished" && s.status !== "aborted")) return null;
  const them = s.you === "white" ? s.black : s.white;
  const engine = engineOf(them.player);
  return engine ? { them, engine, you: s.you } : null;
}

/** The engine you played in a finished local game of yours (its id), else `null`. */
export function opponentEngine(g: Pick<Game, "summary">): string | null {
  return yours(g)?.engine ?? null;
}

/**
 * The strength the engine played at in a game of yours against it: the seat's level, else full strength. `null` for a
 * game against an engine without levels (a fixed budget), which has no ladder to step along.
 */
export function strengthOf(g: Pick<Game, "summary">, cx: RematchContext): Strength | null {
  const ours = yours(g);
  if (!ours || !cx.hasLevels(ours.engine)) return null;
  return ours.them.level ?? "full";
}

/**
 * The request for a rematch of a finished local game against an engine: the same engine (at the same level, or at
 * `to` when given) and the same clock, the colours swapped. A game that began from a position begins from it again.
 * `null` for a game that is not one of yours against an engine played here.
 */
export function rematchRequest(
  g: Pick<Game, "summary" | "start_fen">,
  cx: RematchContext,
  to?: Strength,
): NewGameRequest | null {
  const ours = yours(g);
  if (!ours) return null;
  const s = g.summary;
  const level = to ?? strengthOf(g, cx);
  const seat = level === null ? ours.engine : levelSeat(ours.engine, level);
  const from: StartFrom | undefined = g.start_fen ? { type: "position", fen: g.start_fen, moves: [] } : undefined;
  const youNow = ours.you === "white" ? "black" : "white";
  return {
    white: youNow === "white" ? "you" : seat,
    black: youNow === "white" ? seat : "you",
    time: s.time,
    ...(from ? { from } : {}),
  };
}
