/** What a pawn, knight, bishop, rook and queen are worth when counting who is ahead in material. */
const VALUE: Record<string, number> = { p: 1, n: 3, b: 3, r: 5, q: 9 };

/**
 * White's material lead in a position, in pawns: the sum of White's pieces less Black's, kings aside. Positive is White
 * ahead. It reads only the piece placement of a FEN, so it needs no board.
 */
export function materialLead(fen: string): number {
  const placement = fen.split(" ")[0] ?? "";
  let lead = 0;
  for (const ch of placement) {
    const value = VALUE[ch.toLowerCase()];
    if (value === undefined) continue;
    lead += ch === ch.toUpperCase() ? value : -value;
  }
  return lead;
}
