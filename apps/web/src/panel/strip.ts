/**
 * What the phone's move row needs that the list does not: the number to write before a move, and the moves of a line
 * as items. Pure, so the numbering (a game from a position, Black to move first) is tested on its own.
 */
import type { Classification } from "../api/types";

/** One move in the row. Each is named by an id the panel understands (a ply, or a tree node). */
export interface StripMove {
  id: number;
  /** The move number before it (`12.` or `12…` for Black's first), when it starts a pair. */
  number?: string;
  san: string;
  current: boolean;
  mark?: Classification | null;
}

/** The move number and side of the `ply`th move (1-based) of a line that starts from `start`. */
export function numberOf(start: string, ply: number): { number: number; white: boolean } {
  const parts = start.split(" ");
  const blackFirst = parts[1] === "b";
  const first = Number(parts[5] ?? 1) || 1;
  const index = ply - 1 + (blackFirst ? 1 : 0);
  return { number: first + Math.floor(index / 2), white: index % 2 === 0 };
}

/** One move of a line, named by an id the panel understands. */
export interface LineMove {
  id: number;
  ply: number;
  san: string;
  mark?: Classification | null;
}

/**
 * The row's items: each move, with its number before White's move and before the first move shown if that is Black's
 * (`12…`), the move on the board (`current`) marked.
 */
export function stripMoves(start: string, line: readonly LineMove[], currentId: number | null): StripMove[] {
  return line.map((m, i) => {
    const { number, white } = numberOf(start, m.ply);
    const item: StripMove = { id: m.id, san: m.san, current: m.id === currentId, mark: m.mark };
    if (white) item.number = `${number}.`;
    else if (i === 0) item.number = `${number}…`;
    return item;
  });
}
