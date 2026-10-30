import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { watch } from "vue";
import { queryGames } from "../src/api/fixtures-shell";
import type { GameSummary, Seat } from "../src/api/types";
import { materialLead } from "../src/board/material";
import { useRailFeed } from "../src/rail/feed";
import { matches, monthOf, queryOf, railRows, readFilter, type RailFilter } from "../src/rail/rail";
import { shortWork } from "../src/shell/work";

const seat = (player: string, name = player): Seat => ({ player, name, rating: null, title: null });

/** A game with the fields the rail reads; the rest is what a summary has. */
function game(id: number, patch: Partial<GameSummary> = {}): GameSummary {
  return {
    id,
    white: seat("you", "You"),
    black: seat("engine:stockfish", "Stockfish"),
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
const imported = (id: number) =>
  game(id, { source: { type: "import", id: `i${id}`, owner: "you", rated: true, speed: "blitz" } });
const lichessOthers = (id: number) =>
  game(id, {
    you: null,
    white: seat("lichess:a"),
    black: seat("lichess:b"),
    source: { type: "import", id: `o${id}`, owner: "lichess:a", rated: false, speed: "rapid" },
  });

const FILTERS: RailFilter[] = ["all", "you", "lichess"];

describe("the rail's rows", () => {
  it("pins live games, then the newest first", () => {
    const live = game(10, { status: "live", started_at: "2026-01-01T00:00:00Z" });
    expect(railRows([game(2), game(3), live], "all", null).map((g) => g.id)).toEqual([10, 3, 2]);
  });

  it("shows nothing older than the pages loaded so far, live games always", () => {
    const old = game(1, { started_at: "2026-01-01T00:00:00Z" });
    const mid = game(2, { started_at: "2026-02-01T00:00:00Z" });
    const live = game(3, { status: "live", started_at: "2025-01-01T00:00:00Z" });
    expect(railRows([old, mid, live], "all", "2026-02-01T00:00:00Z").map((g) => g.id)).toEqual([3, 2]);
    expect(railRows([old, mid, live], "all", null).map((g) => g.id)).toEqual([3, 2, 1]);
  });

  it("files games older than two weeks under their month", () => {
    const now = Date.UTC(2026, 9, 29);
    expect(monthOf("2026-10-20T10:00:00Z", now)).toBeNull();
    expect(monthOf("2026-10-01T10:00:00Z", now)).toBe("October");
    expect(monthOf("2026-03-12T10:00:00Z", now)).toBe("March");
    expect(monthOf("2025-12-31T12:00:00Z", now)).toBe("December 2025");
  });

  it("reads a remembered filter, and one it no longer has as All", () => {
    expect(readFilter("tests")).toBe("all");
    expect(readFilter("dameless")).toBe("all");
    expect(readFilter("you")).toBe("you");
    expect(readFilter("nonsense")).toBe("all");
  });
});

describe("a filter asks the server what the client would match", () => {
  const all = [
    game(1),
    imported(2),
    lichessOthers(3),
    game(6, { white: seat("engine:stockfish"), black: seat("engine:maia-1500"), you: null }),
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

  it("lets a computed watch a list's progress", async () => {
    serve(Array.from({ length: 10 }, (_, i) => imported(i + 1)));
    const feed = useRailFeed();
    const states: string[] = [];
    const stop = watch(
      () => `${feed.of("you").started}/${feed.of("you").loading}/${feed.of("you").done}`,
      (now) => states.push(now),
      { flush: "sync" },
    );
    await feed.start("you");
    stop();
    expect(states.at(-1)).toBe("true/false/true");
    expect(states).toContain("false/true/false");
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
    expect(
      shortWork({ kind: "review", status: "running", progress: { done: 2, total: 20, unit: "games" } as never }),
    ).toBe("Review 2 of 20 games");
    expect(shortWork({ kind: "calibrate", status: "paused", progress: null })).toBe("Calibrating");
  });
});
