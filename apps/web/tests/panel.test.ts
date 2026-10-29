import { describe, expect, it } from "vitest";
import { numberOf, stripMoves } from "../src/panel/strip";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const BLACK_FIRST = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 7";

describe("the phone's move row", () => {
  it("numbers the moves of a game from the start", () => {
    expect(numberOf(START, 1)).toEqual({ number: 1, white: true });
    expect(numberOf(START, 2)).toEqual({ number: 1, white: false });
    expect(numberOf(START, 3)).toEqual({ number: 2, white: true });
  });

  it("numbers a line that starts on Black's move from the position's move number", () => {
    expect(numberOf(BLACK_FIRST, 1)).toEqual({ number: 7, white: false });
    expect(numberOf(BLACK_FIRST, 2)).toEqual({ number: 8, white: true });
  });

  it("writes a number before White's moves and before a first move of Black's, and marks the current one", () => {
    const line = [
      { id: 1, ply: 1, san: "e4" },
      { id: 2, ply: 2, san: "e5" },
    ];
    expect(stripMoves(START, line, 2).map((m) => [m.number, m.san, m.current])).toEqual([
      ["1.", "e4", false],
      [undefined, "e5", true],
    ]);
    expect(stripMoves(BLACK_FIRST, line, null)[0]?.number).toBe("7…");
  });
});
