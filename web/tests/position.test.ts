import { describe, expect, it } from "vitest";
import {
  boardMove,
  isValidFen,
  play,
  readPosition,
  sanLine,
  sanMoves,
  toPgn,
  whitePawns,
  whiteScore,
} from "../src/board/position";

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

describe("readPosition and toPgn", () => {
  it("reads a PGN's main line and writes it back", () => {
    const line = readPosition('[Event "x"]\n\n1. e4 c5 2. Nf3 (2. c3) d6 *')!;
    expect(line).toEqual({ fen: "startpos", moves: ["e2e4", "c7c5", "g1f3", "d7d6"] });
    expect(sanMoves(line)).toEqual(["e4", "c5", "Nf3", "d6"]);
    expect(toPgn(line)).toBe("1. e4 c5 2. Nf3 d6 *");
  });

  it("reads a FEN, and keeps it in the PGN", () => {
    const fen = "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1";
    expect(readPosition(fen)).toEqual({ fen, moves: [] });
    expect(toPgn({ fen, moves: ["e2e4"] })).toBe(`[SetUp "1"]\n[FEN "${fen}"]\n\n1. e4 *`);
  });

  it("refuses text that is neither", () => {
    expect(readPosition("hello there")).toBeUndefined();
    expect(readPosition("")).toBeUndefined();
  });
});

describe("whitePawns", () => {
  it("turns the mover's score into White's, with mates at the ends", () => {
    expect(whitePawns({ cp: 50 }, "black")).toBe(-0.5);
    expect(whitePawns({ mate: 3 }, "white")).toBe(100);
    expect(whitePawns({ mate: 2 }, "black")).toBe(-100);
  });
});
