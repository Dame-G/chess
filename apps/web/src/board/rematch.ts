import type { Game, LevelsOverview, NewGameRequest, StartFrom, VersionId } from "../api/types";
import { levelOfRating } from "../sheets/levels";

/** What a rematch needs to know that the game itself does not say. */
export interface RematchContext {
  /** The champion, so that a version other than it is asked for by id. */
  champion: VersionId | null | undefined;
  levels: LevelsOverview | null;
  /** How the game was started, when this browser started it (the game does not say whether its position was a book line). */
  started?: StartFrom | null;
}

/**
 * The request for a rematch of a finished local game against Dameless: the same opponent (version and level) and the
 * same clock, the colours swapped. A game that began from a position begins from it again; one that began from a book
 * line begins from a new book line. `null` for a game that is not one of yours against Dameless played here.
 */
export function rematchRequest(g: Pick<Game, "summary" | "start_fen">, cx: RematchContext): NewGameRequest | null {
  const s = g.summary;
  if (s.source.type !== "local" || !s.you || (s.status !== "finished" && s.status !== "aborted")) return null;
  const them = s.you === "white" ? s.black : s.white;
  if (them.player !== "dameless") return null;

  const level = /^Dameless [0-9]+$/.test(them.name) ? levelOfRating(cx.levels, them.rating) : null;
  const version = them.version != null && them.version !== cx.champion ? `:#${them.version}` : "";
  const seat = `dameless${version}${level != null ? `@${level}` : ""}`;

  const from: StartFrom | undefined =
    cx.started?.type === "book"
      ? { type: "book" }
      : g.start_fen
        ? { type: "position", fen: g.start_fen, moves: [] }
        : undefined;

  const youNow = s.you === "white" ? "black" : "white";
  return {
    white: youNow === "white" ? "you" : seat,
    black: youNow === "white" ? seat : "you",
    time: s.time,
    ...(from ? { from } : {}),
  };
}
