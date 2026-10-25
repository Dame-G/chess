import { describe, expect, it } from "vitest";
import { readComment, readGame } from "../src/board/game";

describe("readComment", () => {
  it("reads fastchess's evaluation, depth, time and nodes", () => {
    expect(readComment("+1.06/9 0.063s, n=9351, sd=14, nps=149921")).toEqual({
      eval: 1.06,
      depth: 9,
      nodes: 9351,
      time: 0.063,
    });
    expect(readComment("-M3/20 0.5s, n=10").eval).toBe(-100);
    expect(readComment("book").eval).toBeNull();
  });
});

describe("readGame", () => {
  it("plays a game from its FEN and scores every move from White's side", () => {
    const pgn = `[Event "x"]
[White "base"]
[Black "dev"]
[Result "1-0"]
[SetUp "1"]
[FEN "r1bqkbnr/p1pp1ppp/4p3/nB6/1P1PP3/5N2/P1P2PPP/RNBQK2R b KQkq - 0 5"]

5... Bxb4+ {-1.16/8 0.176s, n=4991, sd=14, nps=63883}
6. c3 {+1.06/9 0.063s, n=9351, sd=14, nps=149921} 1-0
`;
    const game = readGame(pgn)!;
    expect(game.plies.map((p) => p.san)).toEqual(["Bxb4+", "c3"]);
    expect(game.plies[0]!.eval).toBe(1.16);
    expect(game.plies[1]!.eval).toBe(1.06);
    expect(game.headers.White).toBe("base");
  });
});
