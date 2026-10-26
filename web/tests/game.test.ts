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

describe("castling notation", () => {
  const short = "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. d3 Nf6 5. O-O O-O *";
  const long = "1. d4 d5 2. Nc3 Nc6 3. Bf4 Bf5 4. Qd2 Qd7 5. O-O-O O-O-O *";
  it("writes castling as the king's two-square step, in a game and a pasted PGN", async () => {
    const { readPosition, play } = await import("../src/board/position");
    for (const [pgn, w, b] of [
      [short, "e1g1", "e8g8"],
      [long, "e1c1", "e8c8"],
    ] as const) {
      const moves = readGame(pgn)!.plies.map((p) => p.uci);
      expect(moves.slice(-2)).toEqual([w, b]);
      expect(readPosition(pgn)!.moves).toEqual(moves);
      expect(play({ fen: "startpos", moves })!.moves).toEqual(moves);
    }
  });
  it("still reads the king-takes-rook form of old links", async () => {
    const { play } = await import("../src/board/position");
    const old = ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "f8c5", "e1h1"];
    const p = play({ fen: "startpos", moves: old })!;
    expect(p.moves.at(-1)).toBe("e1g1");
    expect(p.lastMove).toEqual(["e1", "g1"]);
  });
});
