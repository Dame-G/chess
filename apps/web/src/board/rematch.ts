import type { Game, LevelsOverview, NewGameRequest, Seat, StartFrom, VersionId } from "../api/types";
import { levelOfRating } from "../sheets/levels";

/** What a rematch needs to know that the game itself does not say. */
export interface RematchContext {
  /** The champion, so that a version other than it is asked for by id. */
  champion: VersionId | null | undefined;
  levels: LevelsOverview | null;
  /** How the game was started, when this browser started it (the game does not say whether its position was a book line). */
  started?: StartFrom | null;
}

/** A strength of Dameless: a level of the ladder, or `full`. */
export type Strength = number | "full";

/** Whether a finished game is one of yours against Dameless played here: the games a rematch is offered for. */
function yours(g: Pick<Game, "summary">): { them: Seat; you: "white" | "black" } | null {
  const s = g.summary;
  if (s.source.type !== "local" || !s.you || (s.status !== "finished" && s.status !== "aborted")) return null;
  const them = s.you === "white" ? s.black : s.white;
  return them.player === "dameless" ? { them, you: s.you } : null;
}

/** The strength Dameless played at in a game of yours against it: the seat's level, else read back from its rating. */
export function strengthOf(g: Pick<Game, "summary">, levels: LevelsOverview | null): Strength | null {
  const seat = yours(g)?.them;
  if (!seat) return null;
  const level = seat.level ?? (/^Dameless [0-9]+$/.test(seat.name) ? levelOfRating(levels, seat.rating) : null);
  return level ?? "full";
}

/**
 * The request for a rematch of a finished local game against Dameless: the same opponent (version and level, or at
 * `to` when given) and the same clock, the colours swapped. A game that began from a position begins from it again;
 * one that began from a book line begins from a new book line. `null` for a game that is not one of yours against
 * Dameless played here.
 */
export function rematchRequest(
  g: Pick<Game, "summary" | "start_fen">,
  cx: RematchContext,
  to?: Strength,
): NewGameRequest | null {
  const ours = yours(g);
  if (!ours) return null;
  const { them } = ours;
  const s = g.summary;

  const level = to !== undefined ? to : strengthOf(g, cx.levels);
  const version = them.version != null && them.version !== cx.champion ? `:#${them.version}` : "";
  const seat = `dameless${version}${typeof level === "number" ? `@${level}` : ""}`;

  const from: StartFrom | undefined =
    cx.started?.type === "book"
      ? { type: "book" }
      : g.start_fen
        ? { type: "position", fen: g.start_fen, moves: [] }
        : undefined;

  const youNow = ours.you === "white" ? "black" : "white";
  return {
    white: youNow === "white" ? "you" : seat,
    black: youNow === "white" ? seat : "you",
    time: s.time,
    ...(from ? { from } : {}),
  };
}
