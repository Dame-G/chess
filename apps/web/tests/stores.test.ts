import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { dispatch } from "../src/api/stream";
import type { AppEvent, Game, GameSummary, Idea, Move, Trial, Version, Work } from "../src/api/types";
import { useConnections } from "../src/stores/connections";
import { useGames } from "../src/stores/games";
import { useIdeas } from "../src/stores/ideas";
import { usePolicy } from "../src/stores/policy";
import { useVersions } from "../src/stores/versions";
import { useWork } from "../src/stores/work";

/** A server that answers each path with the given body. */
function serve(routes: Record<string, unknown>) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      const path = new URL(input, "http://x").pathname;
      if (!(path in routes))
        return new Response(JSON.stringify({ error: { code: "not_found", message: "no" } }), { status: 404 });
      return new Response(JSON.stringify(routes[path]));
    }),
  );
}

let nextId = 1;
const event = <K extends AppEvent["kind"]>(kind: K, data: Extract<AppEvent, { kind: K }>["data"]) =>
  dispatch({ id: nextId++, topic: "", kind, data } as AppEvent);

const seat = (player: "you" | "dameless") => ({
  player,
  name: player === "you" ? "You" : "Dameless",
  version: player === "dameless" ? 14 : null,
  build: null,
  rating: null,
  title: null,
});

function summary(id: number, patch: Partial<GameSummary> = {}): GameSummary {
  return {
    id,
    white: seat("you"),
    black: seat("dameless"),
    source: { type: "local" },
    time: { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
    status: "live",
    outcome: null,
    ending: null,
    opening: null,
    plies: 2,
    started_at: `2026-10-0${id}T10:00:00Z`,
    ended_at: null,
    you: "white",
    accuracy: null,
    url: null,
    ...patch,
  };
}
const move = (uci: string, san: string): Move => ({
  uci,
  san,
  clock_ms: null,
  think_ms: null,
  eval: null,
  depth: null,
  nodes: null,
});
function game(id: number): Game {
  return {
    summary: summary(id),
    start_fen: null,
    moves: [move("e2e4", "e4"), move("e7e5", "e5")],
    clocks: { white: 290_000, black: 295_000 },
    offers: { draw: null, takeback: null },
    actions: ["move", "resign"],
  };
}

beforeEach(() => {
  setActivePinia(createPinia());
  vi.unstubAllGlobals();
});

describe("games", () => {
  it("loads summaries and lists live games first, then newest", async () => {
    serve({
      "/api/games": [
        summary(1, { status: "finished" }),
        summary(2, { status: "finished" }),
        summary(3, { status: "live", started_at: "2026-09-01T00:00:00Z" }),
      ],
    });
    const games = useGames();
    await games.load();
    expect(games.list.map((g) => g.id)).toEqual([3, 2, 1]);
  });

  it("applies game.upserted to the list and to the open game", async () => {
    serve({ "/api/games": [], "/api/games/7": game(7) });
    const games = useGames();
    await games.load();
    await games.openGame(7);
    event("game.upserted", summary(7, { status: "finished", outcome: "white_wins" }));
    expect(games.summaries[7]?.outcome).toBe("white_wins");
    expect(games.open?.summary.status).toBe("finished");
    event("game.upserted", summary(8));
    expect(games.list.map((g) => g.id)).toContain(8);
  });

  it("appends game.moved to the open game, replaces a replayed ply, and refetches after a gap", async () => {
    serve({ "/api/games": [summary(7)], "/api/games/7": game(7) });
    const games = useGames();
    await games.load();
    await games.openGame(7);
    event("game.moved", {
      id: 7,
      ply: 3,
      move: move("g1f3", "Nf3"),
      clocks: { white: 280_000, black: 295_000 },
      fen: "",
    });
    expect(games.open?.moves.map((m) => m.san)).toEqual(["e4", "e5", "Nf3"]);
    expect(games.open?.clocks?.white).toBe(280_000);
    expect(games.summaries[7]?.plies).toBe(3);
    event("game.moved", { id: 7, ply: 3, move: move("f1c4", "Bc4"), clocks: null, fen: "" });
    expect(games.open?.moves.map((m) => m.san)).toEqual(["e4", "e5", "Bc4"]);
    // A move for another game leaves the open one alone.
    event("game.moved", { id: 9, ply: 1, move: move("d2d4", "d4"), clocks: null, fen: "" });
    expect(games.open?.moves).toHaveLength(3);
    const fetches = vi.mocked(fetch).mock.calls.length;
    event("game.moved", { id: 7, ply: 9, move: move("a2a3", "a3"), clocks: null, fen: "" });
    expect(vi.mocked(fetch).mock.calls.length).toBeGreaterThan(fetches);
  });

  it("takes back to the plies game.undone names", async () => {
    serve({ "/api/games": [], "/api/games/7": game(7) });
    const games = useGames();
    await games.openGame(7);
    event("game.undone", { id: 7, plies: 1 });
    expect(games.open?.moves.map((m) => m.san)).toEqual(["e4"]);
  });

  it("applies game.changed: offers, actions, clocks and the opponent leaving", async () => {
    serve({ "/api/games": [], "/api/games/7": game(7) });
    const games = useGames();
    await games.openGame(7);
    event("game.changed", {
      id: 7,
      offers: { draw: "black", takeback: null },
      actions: ["move", "accept_draw", "decline_draw", "resign"],
      clocks: { white: 100_000, black: 200_000 },
      opponent_gone: { claim_in_seconds: 10 },
    });
    expect(games.open?.offers.draw).toBe("black");
    expect(games.open?.actions).toContain("accept_draw");
    expect(games.open?.clocks?.black).toBe(200_000);
    expect(games.opponentGone?.claim_in_seconds).toBe(10);
  });

  it("keeps the thinking line only for the move being chosen", async () => {
    serve({ "/api/games": [], "/api/games/7": game(7) });
    const games = useGames();
    await games.openGame(7);
    const think = { id: 7, side: "white" as const, depth: 12, score: { cp: 30 }, nodes: 1, nps: 1, pv: ["g1f3"] };
    event("game.thinking", { ...think, ply: 2 });
    expect(games.thinking).toBeNull();
    event("game.thinking", { ...think, ply: 3 });
    expect(games.thinking?.depth).toBe(12);
    event("game.moved", { id: 7, ply: 3, move: move("g1f3", "Nf3"), clocks: null, fen: "" });
    expect(games.thinking).toBeNull();
  });

  it("puts a review's accuracy on the summary", async () => {
    serve({ "/api/games": [summary(5, { status: "finished" })] });
    const games = useGames();
    await games.load();
    event("review.upserted", { game: 5, accuracy: { white: 91, black: 77 } });
    expect(games.summaries[5]?.accuracy).toEqual({ white: 91, black: 77 });
  });
});

describe("ideas", () => {
  const idea: Idea = {
    id: 1,
    title: "Deeper LMR",
    hypothesis: "h",
    kind: "gain",
    author: "claude",
    branch: "deeper-lmr",
    base: 14,
    version: 20,
    status: "running",
    verdict: null,
    trials: [],
    tags: [],
    notes: null,
    created_at: "2026-10-01T00:00:00Z",
    ended_at: null,
  };
  const trial: Trial = {
    id: 3,
    idea: 1,
    stage: "short",
    status: "running",
    preset: "stc",
    time: null,
    played: null,
    scaling: null,
    bounds: [0, 3],
    penta: [1, 2, 3, 2, 1],
    wdl: [10, 10, 10],
    llr: null,
    elo: null,
    nelo: null,
    detail: null,
    started_at: null,
    ended_at: null,
  };

  it("adds a trial to its idea and patches it with progress", () => {
    const ideas = useIdeas();
    event("idea.upserted", idea);
    event("trial.upserted", trial);
    expect(ideas.ideas[1]?.trials.map((t) => t.id)).toEqual([3]);
    event("trial.progress", {
      id: 3,
      penta: [2, 2, 3, 2, 2],
      wdl: [11, 10, 11],
      llr: { value: 1, lower: -2.94, upper: 2.94 },
      elo: null,
    });
    expect(ideas.ideas[1]?.trials[0]?.penta).toEqual([2, 2, 3, 2, 2]);
    expect(ideas.trials[3]?.wdl).toEqual([11, 10, 11]);
    expect(ideas.ofTrial(3)?.idea.title).toBe("Deeper LMR");
    event("trial.upserted", { ...trial, status: "passed" });
    expect(ideas.ideas[1]?.trials).toHaveLength(1);
    expect(ideas.ideas[1]?.trials[0]?.status).toBe("passed");
  });

  it("keeps each tune's latest step", () => {
    const ideas = useIdeas();
    event("tune.progress", { idea: 4, iteration: 10, values: [1, 2], result: 3 });
    expect(ideas.tunes[4]?.iteration).toBe(10);
  });
});

describe("work", () => {
  const work = (id: number, patch: Partial<Work>): Work => ({
    id,
    kind: "trial",
    status: "queued",
    priority: "idea",
    title: `w${id}`,
    subject: { type: "idea", id: 1 },
    cores: 0,
    progress: null,
    paused_for: null,
    error: null,
    created_at: `2026-10-01T00:00:0${id}Z`,
    started_at: null,
    ended_at: null,
    ...patch,
  });

  it("sorts running, queued by priority then age, and recent", () => {
    const w = useWork();
    event("work.upserted", work(1, { status: "running" }));
    event("work.upserted", work(2, { priority: "idea" }));
    event("work.upserted", work(3, { priority: "review" }));
    event("work.upserted", work(4, { status: "done", ended_at: "2026-10-02T00:00:00Z" }));
    expect(w.running.map((x) => x.id)).toEqual([1]);
    expect(w.queued.map((x) => x.id)).toEqual([3, 2]);
    expect(w.recent.map((x) => x.id)).toEqual([4]);
    event("work.upserted", work(1, { status: "paused", paused_for: "Your Lichess game" }));
    expect(w.running[0]?.status).toBe("paused");
  });
});

describe("connections, policy and versions", () => {
  it("replaces a connection and tracks incoming challenges", () => {
    const c = useConnections();
    event("connection.upserted", {
      key: "you",
      configured: true,
      account: {
        id: "me",
        username: "me",
        title: null,
        ratings: { blitz: { rating: 1840, games: 1, provisional: false } },
      },
      state: "online",
      error: null,
      playing: [],
    });
    expect(c.youRating).toBe(1840);
    const challenge = {
      id: "abc",
      connection: "you" as const,
      direction: "incoming" as const,
      opponent: { name: "x", title: null, rating: 1800 },
      rated: true,
      speed: "rapid" as const,
      time: null,
      color: "random" as const,
      url: "",
    };
    event("challenge", { ...challenge, event: "created" });
    expect(c.challenges.map((x) => x.id)).toEqual(["abc"]);
    event("challenge", { ...challenge, event: "declined" });
    expect(c.challenges).toEqual([]);
  });

  it("follows the fair-play policy", () => {
    const p = usePolicy();
    expect(p.allowed).toBe(true);
    event("policy.changed", { assist: "off", reason: "Analysis is off while you're playing on Lichess" });
    expect(p.allowed).toBe(false);
    expect(p.explorer).toBe(false);
    expect(p.reason).toContain("Lichess");
  });

  it("knows the champion from the line", () => {
    const v = useVersions();
    const version = (id: number, n: number): Version => ({
      id,
      name: `v${n}`,
      commit: "x",
      branch: null,
      line: { type: "champion", number: n },
      parent: null,
      bench: null,
      elo: null,
      created_at: `2026-10-0${n}T00:00:00Z`,
    });
    event("version.upserted", version(1, 1));
    event("version.upserted", version(2, 2));
    expect(v.current?.name).toBe("v2");
    expect(v.name(1)).toBe("v1");
  });
});
