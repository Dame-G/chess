import { describe, expect, it } from "vitest";
import type { GameSummary } from "../src/api/types";
import { firstGame, modesFor } from "../src/board/modes";
import { inspectPosition, INITIAL_FEN } from "../src/board/position";
import { GameTree, rows } from "../src/board/tree";
import { readVerb } from "../src/shell/commands";
import { parseEvents } from "../src/api/stream";
import { railRows } from "../src/rail/rail";
import { clock, scoreText, timeControl } from "../src/format";

describe("the game tree", () => {
  it("keeps the game as the main line and adds variations", () => {
    const tree = new GameTree({ fen: INITIAL_FEN, moves: ["e2e4", "e7e5", "g1f3"] });
    const e5 = tree.mainAt(2);
    expect(e5.san).toBe("e5");
    const nc3 = tree.play(e5, "b1c3")!;
    expect(nc3.san).toBe("Nc3");
    expect(tree.isMain(nc3)).toBe(false);
    // Playing a move that exists goes to it.
    expect(tree.play(e5, "g1f3")).toBe(tree.mainAt(3));
    expect(tree.line(nc3).moves).toEqual(["e2e4", "e7e5", "b1c3"]);
    tree.promote(nc3);
    expect(tree.isMain(nc3)).toBe(true);
    expect(tree.remove(nc3)).toBe(e5);
    expect(tree.end(tree.root).san).toBe("Nf3");
  });

  it("writes castling as engines do", () => {
    const tree = new GameTree({ fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", moves: ["e1h1"] });
    expect(tree.mainAt(1).uci).toBe("e1g1");
    expect(tree.mainAt(1).san).toBe("O-O");
  });

  it("lays out rows with variations under the move they branch from", () => {
    const tree = new GameTree({ fen: INITIAL_FEN, moves: ["e2e4", "e7e5", "g1f3"] });
    tree.play(tree.mainAt(1), "c7c5");
    const r = rows(tree);
    expect(r[0]).toMatchObject({ type: "moves", number: 1 });
    expect(r[1]).toMatchObject({ type: "variations" });
    const tokens = r[1]!.type === "variations" ? r[1]!.lines[0]!.map((t) => t.text) : [];
    expect(tokens).toEqual(["1… c5"]);
    expect(r[2]).toMatchObject({ type: "moves", number: 2 });
  });
});

describe("reading a pasted position", () => {
  it("reads a FEN, a PGN and says what is wrong", () => {
    expect(inspectPosition("")).toEqual({ empty: true });
    expect(inspectPosition(INITIAL_FEN)).toMatchObject({ kind: "fen" });
    expect(inspectPosition("1. e4 e5 2. Nf3")).toMatchObject({
      kind: "pgn",
      line: { moves: ["e2e4", "e7e5", "g1f3"] },
    });
    expect(inspectPosition("8/8/8/8/8/8/8/8 w - - 0 1")).toHaveProperty("error");
  });
});

const base: GameSummary = {
  id: 1,
  white: { player: "you", name: "You", version: null, build: null, rating: null, title: null },
  black: { player: "dameless", name: "Dameless", version: 1, build: null, rating: null, title: null },
  source: { type: "local" },
  time: { type: "clock", initial_ms: 180_000, increment_ms: 2000 },
  status: "finished",
  outcome: "draw",
  ending: "agreement",
  opening: null,
  plies: 40,
  started_at: "2026-10-01T00:00:00Z",
  ended_at: null,
  you: "white",
  accuracy: null,
  url: null,
};

describe("modes and what opens first", () => {
  it("offers only the tabs that apply", () => {
    expect(modesFor(null)).toEqual(["explore"]);
    expect(modesFor({ status: "live", you: "white" })).toEqual(["play"]);
    expect(modesFor({ status: "live", you: null })).toEqual(["watch", "explore"]);
    expect(modesFor({ status: "finished", you: null })).toEqual(["review", "explore"]);
  });

  it("opens your live game, then the bot's, then a trial's, then your last finished game", () => {
    const bot = {
      ...base,
      id: 2,
      you: null,
      status: "live" as const,
      source: { type: "lichess" as const, connection: "bot" as const, id: "x", rated: true, speed: "blitz" as const },
    };
    const trial = {
      ...base,
      id: 3,
      you: null,
      status: "live" as const,
      source: { type: "trial" as const, trial: 1, index: 0 },
    };
    const mine = { ...base, id: 4, status: "live" as const };
    expect(firstGame([trial, bot, mine])?.id).toBe(4);
    expect(firstGame([trial, bot])?.id).toBe(2);
    expect(firstGame([trial])?.id).toBe(3);
    expect(firstGame([{ ...base, id: 5, you: null }, base])?.id).toBe(1);
    expect(firstGame([{ ...base, you: null }])).toBeUndefined();
  });
});

describe("the rail", () => {
  it("pins live games and folds a trial's games into one row", () => {
    const t = (id: number, at: string) => ({
      ...base,
      id,
      you: null,
      started_at: at,
      source: { type: "trial" as const, trial: 9, index: id },
    });
    const live = { ...base, id: 10, status: "live" as const, started_at: "2026-01-01T00:00:00Z" };
    const r = railRows([t(1, "2026-10-01T00:00:00Z"), t(2, "2026-10-02T00:00:00Z"), base, live], "all");
    expect(r.map((x) => x.key)).toEqual(["game-10", "trial-9", "game-1"]);
    expect(r[1]?.type === "trial" && r[1].games.map((g) => g.id)).toEqual([2, 1]);
    expect(railRows([t(1, "2026-10-01T00:00:00Z"), base], "you").map((x) => x.key)).toEqual(["game-1"]);
    expect(railRows([t(1, "2026-10-01T00:00:00Z"), base], "tests").map((x) => x.key)).toEqual(["trial-9"]);
  });
});

describe("the command bar's verbs", () => {
  it("reads the CLI's words", () => {
    expect(readVerb("play 5+3 dameless")).toEqual({
      verb: "play",
      opponent: "dameless",
      time: { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
    });
    expect(readVerb("review last 20")).toEqual({ verb: "review", last: 20 });
    expect(readVerb("compare cutemouse83")).toEqual({ verb: "compare", players: ["cutemouse83"] });
    expect(readVerb("new idea")).toEqual({ verb: "new-idea" });
    expect(readVerb("settings")).toBeUndefined();
  });
});

describe("the event stream", () => {
  it("parses envelopes and named events, keeping an incomplete one for later", () => {
    const text =
      'id: 4\ndata: {"id":4,"topic":"work","kind":"work.log","data":{"id":1,"line":"x"}}\n\n' +
      'id: 5\nevent: policy.changed\ndata: {"assist":"off","reason":null}\n\n' +
      "id: 6\ndata: {";
    const [events, rest] = parseEvents(text);
    expect(events.map((e) => [e.id, e.kind])).toEqual([
      [4, "work.log"],
      [5, "policy.changed"],
    ]);
    expect(rest).toBe("id: 6\ndata: {");
  });
});

describe("formatting", () => {
  it("writes scores, clocks and time controls", () => {
    expect(scoreText({ cp: 35 })).toBe("+0.35");
    expect(scoreText({ mate: -2 })).toBe("−M2");
    expect(clock(65_000)).toBe("1:05");
    expect(clock(9_400)).toBe("0:09.4");
    expect(timeControl({ type: "clock", initial_ms: 180_000, increment_ms: 2000 })).toBe("3+2");
    expect(timeControl({ type: "nodes", nodes: 25_000 })).toBe("25k nodes");
  });
});
