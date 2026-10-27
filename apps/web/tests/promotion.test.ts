import { describe, expect, it } from "vitest";
import { boardMove, promotingColor, type Position } from "../src/board/position";
import { PROMOTIONS, promotionCells } from "../src/board/promotion";

const at = (fen: string) => ({ fen }) as Position;
const white = at("8/4P3/8/8/8/8/8/k6K w - - 0 1");
const black = at("k6K/8/8/8/8/8/4p3/8 b - - 0 1");

describe("promotion", () => {
  it("knows a pawn reaching the last rank, of either colour, and nothing else", () => {
    expect(promotingColor(white, "e7", "e8")).toBe("white");
    expect(promotingColor(black, "e2", "e1")).toBe("black");
    expect(promotingColor(white, "h1", "h2")).toBeNull();
    expect(promotingColor(at("8/8/4P3/8/8/8/8/k6K w - - 0 1"), "e6", "e7")).toBeNull();
  });

  it("writes the chosen piece into the move, and a queen when none is chosen", () => {
    expect(boardMove(white, "e7", "e8", "n")).toBe("e7e8n");
    expect(boardMove(black, "e2", "e1", "r")).toBe("e2e1r");
    expect(boardMove(white, "e7", "e8")).toBe("e7e8q");
    expect(boardMove(white, "h1", "g1", "n")).toBe("h1g1");
  });

  it("offers queen, knight, rook and bishop", () => {
    expect(PROMOTIONS.map((p) => p.letter).join("")).toBe("qnrb");
  });

  it("lays the choices from the promotion square toward the middle", () => {
    const rows = (to: "e8" | "e1", side: "white" | "black") => promotionCells(to, side).map((c) => c.row);
    expect(rows("e8", "white")).toEqual([0, 1, 2, 3]);
    expect(rows("e1", "white")).toEqual([7, 6, 5, 4]);
    // Flipped, the far rank is at the bottom.
    expect(rows("e8", "black")).toEqual([7, 6, 5, 4]);
    expect(rows("e1", "black")).toEqual([0, 1, 2, 3]);
  });

  it("puts the column on the promotion file as drawn", () => {
    expect(promotionCells("a8", "white")[0]!.col).toBe(0);
    expect(promotionCells("a8", "black")[0]!.col).toBe(7);
    expect(new Set(promotionCells("c8", "white").map((c) => c.col)).size).toBe(1);
  });
});
