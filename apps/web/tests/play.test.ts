import { afterEach, describe, expect, it, vi } from "vitest";
import type { Game, Seat } from "../src/api/types";
import { rematchRequest, strengthOf } from "../src/board/rematch";

const seat = (player: string, extra: Partial<Seat> = {}): Seat => ({
  player,
  name: player === "you" ? "You" : player,
  rating: null,
  title: null,
  ...extra,
});

describe("playing again one level up or down", () => {
  const game = (them: Partial<Seat>): Pick<Game, "summary" | "start_fen"> =>
    ({
      start_fen: null,
      summary: {
        id: 1,
        white: seat("you"),
        black: seat("engine:stockfish", { name: "Stockfish 1600", rating: 1600, level: 5, ...them }),
        source: { type: "local" },
        time: { type: "movetime", ms: 1000 },
        status: "finished",
        outcome: "white_wins",
        you: "white",
      },
    }) as never;
  const cx = { analysis: "stockfish" };

  it("reads the level from the seat, full strength when it has none, and no ladder for another engine", () => {
    expect(strengthOf(game({}), cx)).toBe(5);
    expect(strengthOf(game({ level: null, name: "Stockfish", rating: null }), cx)).toBe("full");
    expect(strengthOf(game({ player: "engine:maia-1500", level: null }), cx)).toBeNull();
  });

  it("asks for the neighbouring level with the colours swapped and the same clock", () => {
    expect(rematchRequest(game({}), cx, 6)).toEqual({
      white: "stockfish@6",
      black: "you",
      time: { type: "movetime", ms: 1000 },
    });
    expect(rematchRequest(game({}), cx, "full")?.white).toBe("stockfish");
    expect(rematchRequest(game({}), cx)?.white).toBe("stockfish@5");
  });
});

describe("sounds", () => {
  const withMotion = async (reduce: boolean) => {
    vi.resetModules();
    vi.stubGlobal("matchMedia", (q: string) => ({ matches: reduce && q.includes("reduce"), media: q }));
    localStorage.clear();
    return import("../src/board/sound");
  };
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it("are on, and quiet, by default", async () => {
    const sound = await withMotion(false);
    expect(sound.soundOn.value).toBe(true);
    expect(sound.soundVolume.value).toBeLessThanOrEqual(0.5);
  });

  it("start off for someone who asked for less motion, until they say otherwise", async () => {
    const sound = await withMotion(true);
    expect(sound.soundOn.value).toBe(false);
  });

  it("name a move by its notation", async () => {
    const { soundOf } = await withMotion(false);
    expect(soundOf("Nf3")).toBe("move");
    expect(soundOf("exd5")).toBe("capture");
    expect(soundOf("Qh5+")).toBe("check");
    expect(soundOf("Qxf7#")).toBe("check");
    expect(soundOf(undefined)).toBe("move");
  });
});
