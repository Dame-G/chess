import type { Color, Key } from "@lichess-org/chessground/types";

/** What a pawn may become, in the order Lichess lays them out: from the promotion square toward the board's centre. */
export const PROMOTIONS = [
  { role: "queen", letter: "q", label: "Queen" },
  { role: "knight", letter: "n", label: "Knight" },
  { role: "rook", letter: "r", label: "Rook" },
  { role: "bishop", letter: "b", label: "Bishop" },
] as const;

export type Promotion = (typeof PROMOTIONS)[number];

/**
 * Where each choice sits over the board, as a column and row of the grid as drawn (row 0 at the top). The queen takes
 * the promotion square and the rest follow toward the middle, so the column never leaves the board however it is flipped.
 */
export function promotionCells(to: Key, orientation: Color): { choice: Promotion; col: number; row: number }[] {
  const file = to.charCodeAt(0) - 97;
  const rank = Number(to[1]);
  const col = orientation === "white" ? file : 7 - file;
  const row = orientation === "white" ? 8 - rank : rank - 1;
  const step = row === 0 ? 1 : -1;
  return PROMOTIONS.map((choice, i) => ({ choice, col, row: row + step * i }));
}
