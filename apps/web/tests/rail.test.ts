import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { queryGames } from "../src/api/fixtures-shell";
import type { GameSummary, Seat } from "../src/api/types";
import { materialLead } from "../src/board/material";
import { PAGE, useRailFeed } from "../src/rail/feed";
import { matches, queryOf, railRows, readFilter, type GameFilter } from "../src/rail/rail";
import { useGames } from "../src/stores/games";
import { shortWork } from "../src/shell/work";

const seat = (player: string, name = player): Seat => ({ player, name, rating: null, title: null });

/** A game with the fields the rail reads; the rest is what a summary has. */
function game(id: number, patch: Partial<GameSummary> = {}): GameSummary {
  return {
    id,
    white: seat("you", "You"),
    black: seat("dameless", "Dameless"),
    source: { type: "local" },
    time: { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
    status: "finished",
    outcome: "draw",
    ending: null,
    opening: null,
    plies: 40,
    started_at: new Date(Date.UTC(2026, 0, 1) + id * 60_000).toISOString(),
    ended_at: null,
    you: "white",
    accuracy: null,
    url: null,
    ...patch,
  } as GameSummary;
}
const trialGame = (id: number, trial: number, patch: Partial<GameSummary> = {}) =>
  game(id, {
    you: null,
    white: seat("dameless", "v14"),
    black: seat("dameless", "deeper"),
    source: { type: "trial", trial, index: id },
    ...patch,
  });
const imported = (id: number) =>
  game(id, { source: { type: "import", id: `i${id}`, owner: "you", rated: true, speed: "blitz" } });
const lichessOthers = (id: number) =>
  game(id, {
    you: null,
    white: seat("lichess:a"),
    black: seat("lichess:b"),
    source: { type: "import", id: `o${id}`, owner: "lichess:a", rated: false, speed: "rapid" },
  });
const botGame = (id: number) =>
  game(id, {
    you: null,
    white: seat("dameless"),
    black: seat("lichess:x"),
    source: { type: "lichess", connection: "bot", id: `b${id}`, rated: true, speed: "blitz" },
  });

const FILTERS: GameFilter[] = ["all", "you", "dameless", "lichess"];

describe("the rail's rows", () => {
  it("pins live games, newest first, and folds a running trial's games into one row", () => {
    const live = game(10, { status: "live", started_at: "2026-01-01T00:00:00Z" });
    const t = (id: number, at: string, status: GameSummary["status"] = "live") =>
      trialGame(id, 9, { started_at: at, status });
    const r = railRows([t(1, "2026-10-01T00:00:00Z"), t(2, "2026-10-02T00:00:00Z"), game(3), live], "all", null);
    expect(r.map((x) => x.key)).toEqual(["trial-9", "game-10", "game-3"]);
    expect(r[0]?.type === "trial" && r[0].games.map((g) => g.id)).toEqual([2, 1]);
  });

  it("leaves a finished trial's games to the Trials filter", () => {
    const finished = trialGame(1, 9);
    expect(railRows([finished, game(2)], "all", null).map((x) => x.key)).toEqual(["game-2"]);
    expect(railRows([finished, game(2)], "dameless", null).map((x) => x.key)).toEqual(["game-2"]);
  });

  it("shows nothing older than the pages loaded so far, live games always", () => {
    const old = game(1, { started_at: "2026-01-01T00:00:00Z" });
    const mid = game(2, { started_at: "2026-02-01T00:00:00Z" });
    const live = game(3, { status: "live", started_at: "2025-01-01T00:00:00Z" });
    expect(railRows([old, mid, live], "all", "2026-02-01T00:00:00Z").map((x) => x.key)).toEqual(["game-3", "game-2"]);
    expect(railRows([old, mid, live], "all", null).map((x) => x.key)).toEqual(["game-3", "game-2", "game-1"]);
  });

  it("reads an old remembered filter", () => {
    expect(readFilter("tests")).toBe("trials");
    expect(readFilter("you")).toBe("you");
    expect(readFilter("nonsense")).toBe("all");
  });
});

describe("a filter asks the server what the client would match", () => {
  const all = [
    game(1),
    imported(2),
    lichessOthers(3),
    botGame(4),
    trialGame(5, 3),
    game(6, { white: seat("dameless"), black: seat("stockfish:5"), you: null }),
  ];
  for (const filter of FILTERS) {
    it(`for ${filter}`, () => {
      const q = queryOf(filter);
      const params = new URLSearchParams({ limit: "100", ...q });
      const served = queryGames(all, params)
        .map((g) => g.id)
        .sort();
      expect(served).toEqual(
        all
          .filter((g) => matches(g, filter))
          .map((g) => g.id)
          .sort(),
      );
    });
  }
});

describe("the rail's feed", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    localStorage.clear();
  });

  /** A server holding `data`, answering `GET /api/games` as the real one does. */
  function serve(data: GameSummary[]) {
    const asked: string[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string | Request) => {
        const url = new URL(typeof input === "string" ? input : input.url, "http://x");
        asked.push(url.search);
        if (url.pathname !== "/api/games") return new Response("[]");
        return new Response(JSON.stringify(queryGames(data, url.searchParams)));
      }),
    );
    return asked;
  }

  it("finds your games when trial games fill the newest 300", async () => {
    // The real app: thousands of trial games newer than the games you played.
    const trials = Array.from({ length: 400 }, (_, i) =>
      trialGame(1000 + i, 3, { started_at: new Date(Date.UTC(2026, 5, 1) + i * 1000).toISOString() }),
    );
    const mine = Array.from({ length: 120 }, (_, i) => imported(i + 1));
    serve([...trials, ...mine]);
    const games = useGames();
    const feed = useRailFeed();
    await games.load();
    // The newest 300 are all trial games: filtering them in the browser would find none of yours.
    expect(games.list.filter((g) => g.source.type === "trial").length).toBe(300);
    await feed.start("you");
    expect(railRows(games.list, "you", feed.of("you").floor)).toHaveLength(PAGE);
    // Scrolling on brings the rest, a page at a time, until the server has no more.
    await feed.more("you");
    expect(feed.of("you").done).toBe(false);
    await feed.more("you");
    expect(feed.of("you").done).toBe(true);
    expect(railRows(games.list, "you", feed.of("you").floor)).toHaveLength(120);
    // A page after the end changes nothing.
    await feed.more("you");
    expect(railRows(games.list, "you", feed.of("you").floor)).toHaveLength(120);
  });

  it("asks for each filter's own games, after the last one it holds", async () => {
    const asked = serve(Array.from({ length: 70 }, (_, i) => imported(i + 1)));
    const feed = useRailFeed();
    await feed.start("lichess");
    await feed.more("lichess");
    expect(asked).toHaveLength(2);
    expect(asked[0]).toContain("source=lichess%2Cimport");
    expect(asked[0]).not.toContain("before");
    expect(asked[1]).toMatch(/before=\d+/);
    expect(feed.of("lichess").done).toBe(true);
  });

  it("pages one trial's games on their own", async () => {
    const asked = serve([...Array.from({ length: 80 }, (_, i) => trialGame(i + 1, 7)), trialGame(500, 8)]);
    const games = useGames();
    const feed = useRailFeed();
    await feed.start("trial:7");
    expect(asked[0]).toContain("trial=7");
    expect(games.list.map((g) => g.id)).not.toContain(500);
    expect(feed.of("trial:7").done).toBe(false);
  });

  it("says when a page failed, and tries again", async () => {
    let fail = true;
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        fail
          ? new Response(JSON.stringify({ error: { code: "internal", message: "down" } }), { status: 500 })
          : new Response("[]"),
      ),
    );
    const feed = useRailFeed();
    await feed.start("you");
    expect(feed.of("you").error).toBeTruthy();
    expect(feed.of("you").started).toBe(true);
    fail = false;
    await feed.retry("you");
    expect(feed.of("you").error).toBeNull();
    expect(feed.of("you").done).toBe(true);
  });
});

describe("the strips", () => {
  it("counts material from the placement alone", () => {
    expect(materialLead("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")).toBe(0);
    // White has lost a knight.
    expect(materialLead("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/R1BQKBNR w KQkq - 0 1")).toBe(-3);
    // Black has lost the queen and a pawn.
    expect(materialLead("rnb1kbnr/ppppppp1/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")).toBe(10);
  });
});

describe("the work indicator's words", () => {
  it("says what is running in a few words", () => {
    expect(shortWork({ kind: "trial", status: "running", progress: { done: 412, unit: "pairs" } as never })).toBe(
      "Trial 412 pairs",
    );
    expect(
      shortWork({ kind: "review", status: "running", progress: { done: 2, total: 20, unit: "games" } as never }),
    ).toBe("Review 2 of 20 games");
    expect(shortWork({ kind: "build", status: "paused", progress: null })).toBe("Building");
  });
});
