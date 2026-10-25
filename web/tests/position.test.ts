import { describe, expect, it } from "vitest";
import { boardMove, isValidFen, play, sanLine, whiteScore } from "../src/board/position";

describe("play", () => {
  it("plays legal moves and drops an illegal one with everything after it", () => {
    const p = play({ fen: "startpos", moves: ["e2e4", "e7e5", "e1e3", "g1f3"] })!;
    expect(p.moves).toEqual(["e2e4", "e7e5"]);
    expect(p.turn).toBe("white");
    expect(p.lastMove).toEqual(["e7", "e5"]);
    expect(p.dests.get("g1")).toEqual(expect.arrayContaining(["f3", "h3"]));
  });

  it("rejects a FEN that is not a legal position", () => {
    expect(play({ fen: "not a fen", moves: [] })).toBeUndefined();
    expect(isValidFen("8/8/8/8/8/8/8/8 w - - 0 1")).toBe(false);
  });
});

describe("boardMove", () => {
  it("promotes to a queen on the last rank", () => {
    const p = play({ fen: "8/4P3/8/8/8/8/k7/7K w - - 0 1", moves: [] })!;
    expect(boardMove(p, "e7", "e8")).toBe("e7e8q");
    expect(boardMove(p, "h1", "g1")).toBe("h1g1");
  });
});

describe("sanLine and whiteScore", () => {
  it("writes a line in SAN and scores from White's side", () => {
    expect(sanLine("startpos", ["e2e4", "e7e5", "g1f3"])).toBe("1. e4 e5 2. Nf3");
    expect(whiteScore({ cp: 35 }, "white")).toBe("+0.35");
    expect(whiteScore({ cp: 35 }, "black")).toBe("-0.35");
    expect(whiteScore({ mate: 2 }, "black")).toBe("#-2");
  });
});
