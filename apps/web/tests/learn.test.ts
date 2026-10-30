import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { retryDelay, RETRY_CAP_MS } from "../src/api/backoff";
import type {
  Drill,
  DrillAnswerResult,
  Game,
  GameSummary,
  LevelRating,
  LevelsOverview,
  ProgressChart,
  Repertoire,
  RepertoireMove,
  RepertoireNode,
} from "../src/api/types";
import {
  answerArrow,
  comesBack,
  nextQuestion,
  prompt,
  questionOf,
  sitting,
  sittingText,
  todayText,
  verdict,
} from "../src/board/drill";
import { rematchRequest } from "../src/board/rematch";
import { readVerb } from "../src/shell/commands";
import { isLevelSeat, seatName } from "../src/format";
import {
  calibrationCost,
  levelChoices,
  neighbour,
  ratingText,
  standingOf,
  playOutSeat,
  suggestedStrength,
} from "../src/sheets/levels";
import { latest, progressData } from "../src/sheets/progress";
import { departures, mainLineIds, outline, positionKey, repertoireNote } from "../src/sheets/repertoire";
import { useDrills } from "../src/stores/drills";
import { usePolicy } from "../src/stores/policy";

// ---- The event stream's retry

describe("the event stream's retry", () => {
  it("doubles with each failure in a row and stops at ten seconds", () => {
    const top = (attempt: number) => retryDelay(attempt, 0.999999);
    expect(top(0)).toBe(1000);
    expect(top(1)).toBe(2000);
    expect(top(2)).toBe(4000);
    expect(top(3)).toBe(8000);
    expect(top(4)).toBe(RETRY_CAP_MS);
    expect(top(40)).toBe(RETRY_CAP_MS);
  });

  it("spreads over the upper half, so tabs do not come back together", () => {
    expect(retryDelay(3, 0)).toBe(4000);
    expect(retryDelay(3, 0.5)).toBe(6000);
    for (let attempt = 0; attempt < 8; attempt++)
      for (const r of [0, 0.3, 0.7, 0.9999]) {
        const d = retryDelay(attempt, r);
        expect(d).toBeLessThanOrEqual(RETRY_CAP_MS);
        expect(d).toBeGreaterThanOrEqual(500);
      }
  });

  it("never asks for hundreds of requests a second", () => {
    // A server that is down for a minute is tried about ten times, not hundreds.
    let t = 0;
    let n = 0;
    for (let attempt = 0; t < 60_000; attempt++) {
      t += retryDelay(attempt, 0);
      n++;
    }
    expect(n).toBeLessThan(20);
  });
});

// ---- Drills

const drill = (id: number, patch: Partial<Drill> = {}): Drill => ({
  id,
  game: 5,
  ply: 13,
  fen: "r1bqk2r/1pp2ppp/p1np1n2/2b1p3/2B1P3/2PP1N2/PP3PPP/RNBQ1RK1 w kq - 0 7",
  key: "r1bqk2r/1pp2ppp/p1np1n2/2b1p3/2B1P3/2PP1N2/PP3PPP/RNBQ1RK1 w kq -",
  side: "white",
  best_moves: ["f1e1"],
  best_san: ["Re1"],
  line: ["f1e1", "e8g8"],
  played: "h2h3",
  played_san: "h3",
  swing: 14,
  theme: { opening: "Italian Game", phase: "opening", kind: "other", detail: null },
  due: "2026-10-27T00:00:00Z",
  interval_days: 0,
  ease: 2.5,
  reps: 0,
  lapses: 0,
  created_at: "2026-10-20T00:00:00Z",
  answered_at: null,
  ...patch,
});

describe("the drill flow", () => {
  it("asks a drill without its answer", () => {
    const q = questionOf(drill(1));
    expect(Object.keys(q)).not.toContain("best_moves");
    expect(JSON.stringify(q)).not.toContain("Re1");
    expect(prompt(q)).toBe("White to move: find the move you missed");
    expect(prompt({ side: "black" })).toMatch(/^Black to move/);
  });

  it("goes on to the next drill not yet tried, and round to a skipped one", () => {
    const queue = [drill(1), drill(2), drill(3)];
    expect(nextQuestion(queue, new Set())?.id).toBe(1);
    expect(nextQuestion(queue, new Set([1]), 1)?.id).toBe(2);
    expect(nextQuestion(queue, new Set([1, 2]), 2)?.id).toBe(3);
    // Drill 3 was skipped, not tried: after drill 2 the set comes back to it, then to the start.
    expect(nextQuestion(queue, new Set([1, 3]), 3)?.id).toBe(2);
    expect(nextQuestion(queue, new Set([1, 2, 3]), 3)).toBeNull();
    expect(nextQuestion([], new Set())).toBeNull();
  });

  it("counts a sitting and says where it is", () => {
    const queue = [drill(1), drill(2), drill(3)];
    const none = sitting(queue, new Map());
    expect(none).toEqual({ tried: 0, passed: 0, left: 3, total: 3 });
    expect(sittingText(none)).toBe("1 of 3");
    const two = sitting(
      queue,
      new Map([
        [1, "pass"],
        [2, "fail"],
      ]),
    );
    expect(sittingText(two)).toBe("3 of 3");
    expect(sittingText(two, true)).toBe("2 of 3");
    const all = sitting(
      queue,
      new Map([
        [1, "pass"],
        [2, "fail"],
        [3, "pass"],
      ]),
    );
    expect(sittingText(all, true)).toBe("3 of 3 done: 2 passed");
    expect(sittingText({ tried: 0, passed: 0, left: 0, total: 0 })).toBe("");
  });

  it("states the verdict and when the drill comes back", () => {
    const base = { best_san: ["Re1"], played_san: "h3", drill: drill(1, { interval_days: 1 }) };
    expect(verdict({ ...base, outcome: "fail" })).toBe("Not h3. Re1 was the move. It comes back tomorrow.");
    expect(verdict({ ...base, played_san: "Re1", outcome: "pass", drill: drill(1, { interval_days: 6 }) })).toBe(
      "Yes, Re1. It comes back in 6 days.",
    );
    expect(comesBack(1)).toBe("tomorrow");
    expect(comesBack(16)).toBe("in 16 days");
  });

  it("draws the best move, the first of the line when none is named", () => {
    expect(answerArrow({ best_moves: ["f1e1"], line: ["f1e1", "e8g8"] })).toBe("f1e1");
    expect(answerArrow({ best_moves: [], line: ["g1f3"] })).toBe("g1f3");
    expect(answerArrow({ best_moves: [], line: [] })).toBeNull();
  });

  it("writes today's set in words", () => {
    expect(todayText({ drills: [], due: 0, new: 0 })).toBe("Nothing due today");
    expect(todayText({ drills: [drill(1), drill(2), drill(3)], due: 1, new: 2 })).toBe("3 today: 1 due, 2 new");
    expect(todayText({ drills: [drill(1)], due: 0, new: 1 })).toBe("1 today: 1 new");
  });

  it("reads the verb", () => {
    expect(readVerb("drills")).toEqual({ verb: "drills" });
    expect(readVerb("drill")).toEqual({ verb: "drills" });
  });
});

function serve(routes: Record<string, (init?: RequestInit) => unknown>) {
  const calls: { path: string; method: string; body: unknown }[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      const path = new URL(input, "http://x").pathname;
      calls.push({ path, method: init?.method ?? "GET", body: init?.body ? JSON.parse(String(init.body)) : null });
      const answer = routes[path];
      if (!answer)
        return new Response(JSON.stringify({ error: { code: "not_found", message: "no" } }), { status: 404 });
      return new Response(JSON.stringify(answer(init)));
    }),
  );
  return calls;
}

describe("the drills store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    usePolicy().known = true;
  });

  const result = (outcome: "pass" | "fail", d: Drill): DrillAnswerResult => ({
    outcome,
    hinted: false,
    drill: { ...d, reps: outcome === "pass" ? 1 : 0, interval_days: 1, answered_at: "2026-10-27T10:00:00Z" },
    best_moves: d.best_moves,
    best_san: d.best_san,
    line: d.line,
    played_san: outcome === "pass" ? "Re1" : "h3",
  });

  it("starts today's set, takes one try, and moves on", async () => {
    const set = [drill(1), drill(2)];
    const calls = serve({
      "/api/drills/today": () => ({ drills: set, due: 0, new: 2, started_today: 0, total: 2 }),
      "/api/drills": () => set,
      "/api/drills/trend": () => [],
      "/api/drills/1/answer": () => result("fail", set[0]!),
    });
    const drills = useDrills();
    expect(await drills.start()).toBe(true);
    expect(drills.active?.id).toBe(1);
    expect(drills.drilling).toBe(true);
    expect(drills.result).toBeNull();

    const r = await drills.answer("h2h3");
    expect(r?.outcome).toBe("fail");
    expect(drills.result?.outcome).toBe("fail");
    expect(calls.find((c) => c.path === "/api/drills/1/answer")?.body).toEqual({ move: "h2h3" });
    // One try: a second answer is not sent.
    expect(await drills.answer("f1e1")).toBeUndefined();
    expect(calls.filter((c) => c.path === "/api/drills/1/answer")).toHaveLength(1);

    expect(drills.progress).toEqual({ tried: 1, passed: 0, left: 1, total: 2 });
    expect(drills.next()).toBe(true);
    expect(drills.active?.id).toBe(2);
    expect(drills.result).toBeNull();
  });

  it("finds the set done after the last drill", async () => {
    const set = [drill(1)];
    serve({
      "/api/drills/today": () => ({ drills: set, due: 0, new: 1, started_today: 0, total: 1 }),
      "/api/drills": () => set,
      "/api/drills/trend": () => [],
      "/api/drills/1/answer": () => result("pass", set[0]!),
    });
    const drills = useDrills();
    await drills.start();
    await drills.answer("f1e1");
    expect(drills.next()).toBe(false);
    drills.leave();
    expect(drills.drilling).toBe(false);
  });

  it("does nothing while the policy forbids analysis", async () => {
    const calls = serve({});
    const policy = usePolicy();
    policy.policy = { assist: "off", reason: "You are playing on Lichess" };
    const drills = useDrills();
    expect(drills.available).toBe(false);
    expect(await drills.start()).toBe(false);
    expect(await drills.open(3)).toBe(false);
    expect(calls).toHaveLength(0);
  });

  it("answers false when there is nothing today", async () => {
    serve({
      "/api/drills/today": () => ({ drills: [], due: 0, new: 0, started_today: 0, total: 0 }),
      "/api/drills": () => [],
      "/api/drills/trend": () => [],
    });
    const drills = useDrills();
    expect(await drills.start()).toBe(false);
    expect(drills.active).toBeNull();
  });
});

// ---- Your openings

const move = (uci: string, san: string, next: string, games: number, share: number, score = 0.5): RepertoireMove => ({
  uci,
  san,
  next,
  games,
  share,
  score,
  wins: 0,
  draws: 0,
  losses: 0,
  leaves_theory: false,
});
const node = (key: string, ply: number, your_turn: boolean, moves: RepertoireMove[], games = 10): RepertoireNode => ({
  key,
  fen: `${key} 0 1`,
  ply,
  games,
  your_turn,
  theory: true,
  moves,
});

const rep: Repertoire = {
  side: "white",
  games: 10,
  options: { max_plies: 20, min_games: 2 },
  nodes: [
    node("start w KQkq -", 0, true, [
      move("e2e4", "e4", "e4 b - -", 7, 0.7, 0.6),
      move("d2d4", "d4", "d4 b - -", 3, 0.3, 0.4),
      move("c2c4", "c4", "c4 b - -", 2, 0.1),
    ]),
    node(
      "e4 b - -",
      1,
      false,
      [
        move("e7e5", "e5", "e5 w - -", 5, 0.71),
        move("c7c5", "c5", "c5 w - -", 2, 0.29),
        move("e7e6", "e6", "e6 w - -", 1, 0.1),
        move("a7a6", "a6", "a6 w - -", 1, 0.1),
      ],
      7,
    ),
    node("e5 w - -", 2, true, [move("g1f3", "Nf3", "Nf3 b - -", 5, 1, 0.55)], 5),
    node("Nf3 b - -", 3, false, [], 5),
    node("c5 w - -", 2, true, [], 2),
    node("d4 b - -", 1, false, [move("d7d5", "d5", "d5 w - -", 3, 1)], 3),
    node("d5 w - -", 2, true, [], 3),
  ],
  departures: [
    {
      key: "e5 w - -",
      fen: "e5 w - - 0 2",
      ply: 3,
      uci: "g1f3",
      san: "Nf3",
      by_you: true,
      games: 5,
      score: 0.6,
      example: 9,
    },
    {
      key: "c5 w - -",
      fen: "c5 w - - 0 2",
      ply: 2,
      uci: "c7c5",
      san: "c5",
      by_you: false,
      games: 2,
      score: 0.5,
      example: 4,
    },
    {
      key: "d5 w - -",
      fen: "d5 w - - 0 3",
      ply: 4,
      uci: "c2c4",
      san: "c4",
      by_you: true,
      games: 3,
      score: 0.4,
      example: 7,
    },
  ],
};

/** The tree flattened depth first, to read it as the rows a person sees fully open. */
const flat = (branches: ReturnType<typeof outline>): ReturnType<typeof outline>[number]["row"][] =>
  branches.flatMap((b) => [b.row, ...flat(b.children)]);

describe("the repertoire tree", () => {
  it("shows your top two moves and the top three replies you met", () => {
    const rows = flat(outline(rep));
    // Root: your two moves (c4 is the third and is left out). Under e4: the replies, three of four.
    expect(rows.map((r) => `${r.depth}:${r.san}`)).toEqual(["0:e4", "1:e5", "2:Nf3", "1:c5", "1:e6", "0:d4", "1:d5"]);
    expect(rows[0]).toMatchObject({ id: "e2e4", yours: true, number: "1.", games: 7, expandable: true, ply: 1 });
    expect(rows[1]).toMatchObject({ yours: false, number: "1…", line: ["e2e4", "e7e5"] });
  });

  it("nests the moves that lead on and marks what has none unexpandable", () => {
    const tree = outline(rep);
    expect(tree.map((b) => b.row.san)).toEqual(["e4", "d4"]);
    const nf3 = flat(tree).find((r) => r.san === "Nf3");
    expect(nf3).toMatchObject({ depth: 2, expandable: false });
    expect(tree[0]!.children.map((b) => b.row.san)).toEqual(["e5", "c5", "e6"]);
  });

  it("opens along the main line", () => {
    expect([...mainLineIds(rep)]).toEqual(["e2e4", "e2e4 e7e5", "e2e4 e7e5 g1f3"]);
    expect([...mainLineIds(rep, 1)]).toEqual(["e2e4"]);
  });

  it("ends a branch that comes back to a position on its own path", () => {
    const loop: Repertoire = {
      ...rep,
      nodes: [
        node("start", 0, true, [move("g1f3", "Nf3", "a", 4, 1)]),
        node("a", 1, false, [move("g8f6", "Nf6", "start", 4, 1)]),
      ],
    };
    const rows = flat(outline(loop));
    expect(rows.map((r) => r.san)).toEqual(["Nf3", "Nf6"]);
    expect(rows[1]!.expandable).toBe(false);
  });

  it("lists where you and your opponents leave theory, the most common first", () => {
    const d = departures(rep);
    expect(d.yours.map((x) => x.san)).toEqual(["Nf3", "c4"]);
    expect(d.theirs.map((x) => x.san)).toEqual(["c5"]);
    expect(departures(rep, 1).yours).toHaveLength(1);
  });

  it("keys a position without its move counters", () => {
    expect(positionKey("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1")).toBe(
      "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3",
    );
  });

  it("notes what your repertoire plays at the position on the board", () => {
    expect(repertoireNote({ white: rep }, "e5 w - - 0 2")).toBe("Your repertoire: you play Nf3 here (100%, 55% score)");
    expect(repertoireNote({ white: rep }, "e4 b - - 0 1")).toBe(
      "Your repertoire: your opponents play e5 here (71%, 50% score)",
    );
    expect(repertoireNote({ white: rep, black: null }, "somewhere/else w - - 0 1")).toBeNull();
    expect(repertoireNote({ white: null, black: null }, "e5 w - - 0 2")).toBeNull();
  });
});

// ---- Levels

const level = (id: number, nominal: number, nodes: number | null, games = 0, elo?: number): LevelRating => ({
  level: { id, name: `Level ${id}`, nodes, multipv: 1, temperature: 0, nominal },
  games,
  rating: elo == null ? null : { elo, low: elo - 40, high: elo + 40 },
});
const overview: LevelsOverview = {
  engine: "stockfish",
  levels: [level(1, 800, 300, 12, 742), level(2, 1000, 300), level(3, 1200, 500, 0), level(12, 3000, null)],
  suggested: 3,
  you: null,
  updated_at: null,
};

describe("the levels", () => {
  it("writes a rating by how well the ladder knows it", () => {
    // 12 games: found, but too few to call it settled.
    expect(ratingText(overview.levels[0]!)).toBe("≈ 742 ± 40");
    expect(ratingText(overview.levels[1]!)).toBe("≈ 1,000");
    expect(ratingText(level(4, 1400, 800, 40, 1403))).toBe("1,403 ± 40");
    // Plenty of games but a margin of ±488: still being calibrated.
    expect(
      ratingText({ ...level(9, 2400, 40_000, 40, 2440), rating: { elo: 2440.8, low: 1952.5, high: 2929.2 } }),
    ).toBe("≈ 2,441 ± 488");
  });

  it("sorts levels into found, provisional and estimate", () => {
    expect(standingOf(level(4, 1400, 800, 40, 1403))).toBe("found");
    expect(standingOf(level(4, 1400, 800, 20, 1403))).toBe("found");
    expect(standingOf(level(4, 1400, 800, 19, 1403))).toBe("provisional");
    expect(standingOf(level(9, 2400, 40_000))).toBe("estimate");
  });

  it("offers each level from weakest to strongest with its rating, the one about yours marked", () => {
    const options = levelChoices(overview);
    expect(options.map((o) => o.value)).toEqual(["1", "2", "3", "full"]);
    expect(options[0]).toMatchObject({ name: "Level 1", text: "≈ 742 ± 40", standing: "provisional", near: false });
    expect(options[2]).toMatchObject({ name: "Level 3", text: "≈ 1,200", standing: "estimate", near: true });
    expect(options[3]).toMatchObject({ name: "Full strength", text: "≈ 3,000" });
    expect(levelChoices(null)).toEqual([]);
  });

  it("finds the level a step harder or easier, through full strength", () => {
    expect(neighbour(overview, 1, true)).toMatchObject({ strength: 2, name: "Level 2" });
    expect(neighbour(overview, 3, true)).toMatchObject({ strength: "full", name: "Full strength" });
    expect(neighbour(overview, "full", false)).toMatchObject({ strength: 3 });
    expect(neighbour(overview, 1, false)).toBeNull();
    expect(neighbour(overview, "full", true)).toBeNull();
    expect(neighbour(null, 1, true)).toBeNull();
  });

  it("practises at the level nearest your rating", () => {
    expect(suggestedStrength(overview)).toBe("3");
    expect(playOutSeat("stockfish", overview)).toBe("stockfish@3");
    expect(playOutSeat("stockfish", { ...overview, suggested: 12 })).toBe("stockfish");
    expect(playOutSeat("stockfish", { ...overview, suggested: null })).toBe("stockfish");
    expect(playOutSeat("lc0", null)).toBe("lc0");
  });

  it("puts the cost of calibrating in words", () => {
    const plan = [
      { a: 1, b: 2, games: 40, priority: 1 },
      { a: 2, b: 3, games: 40, priority: 1 },
    ];
    const text = calibrationCost(plan, overview.levels, 2);
    expect(text).toMatch(
      /^80 games between 2 pairs of neighbouring levels\. (About .+|Under a minute) in the background/,
    );
    expect(calibrationCost([], overview.levels, 2)).toBe("");
  });

  it("names a level seat by its rating", () => {
    const seat = { player: "engine:stockfish", name: "Stockfish 1650", level: 5, rating: 1650, title: null };
    expect(isLevelSeat(seat)).toBe(true);
    expect(seatName(seat)).toBe("Stockfish 1650");
    expect(isLevelSeat({ ...seat, level: null })).toBe(false);
    expect(isLevelSeat({ ...seat, player: "you", name: "Stockfish 12" })).toBe(false);
  });
});

// ---- Progress

describe("the progress chart", () => {
  const point = (at: string, value: number) => ({
    at,
    value,
    game: null,
    low: null,
    high: null,
    label: null,
    version: null,
  });
  const chart: ProgressChart = {
    series: [
      {
        player: "you",
        metric: "rating",
        points: [point("2026-10-01T00:00:00Z", 1800), point("2026-10-10T00:00:00Z", 1840)],
      },
      {
        player: "you",
        metric: "accuracy",
        points: [point("2026-10-05T00:00:00Z", 76), point("2026-10-10T00:00:00Z", 81)],
      },
      { player: "you", metric: "drill_hit_rate", points: [] },
    ],
  };

  it("puts the lines on one time axis, a gap where a line has no point", () => {
    const d = progressData(chart, ["rating", "accuracy"]);
    expect(d.x).toHaveLength(3);
    expect(d.lines.map((l) => l.metric)).toEqual(["rating", "accuracy"]);
    expect(d.lines[0]!.values).toEqual([1800, null, 1840]);
    expect(d.lines[1]!.values).toEqual([null, 76, 81]);
    expect(latest(d.lines[0]!)).toBe(1840);
    expect(progressData(chart).lines.map((l) => l.metric)).toEqual(["rating"]);
  });

  it("shows the hit rate as a percentage and leaves out a line of fewer than two points", () => {
    const withHits: ProgressChart = {
      series: [
        {
          player: "you",
          metric: "drill_hit_rate",
          points: [point("2026-10-05T00:00:00Z", 0.5), point("2026-10-06T00:00:00Z", 0.75)],
        },
      ],
    };
    expect(progressData(withHits, ["drill_hit_rate"]).lines[0]!.values).toEqual([50, 75]);
    expect(progressData(chart, ["drill_hit_rate"]).lines).toEqual([]);
  });
});

// ---- Rematch

const seat = (player: string, extra: Partial<GameSummary["white"]> = {}) => ({
  player,
  name: player === "you" ? "You" : "Stockfish",
  rating: null,
  title: null,
  ...extra,
});
const gameOf = (
  patch: Partial<GameSummary> = {},
  start_fen: string | null = null,
): Pick<Game, "summary" | "start_fen"> => ({
  start_fen,
  summary: {
    id: 3,
    white: seat("you"),
    black: seat("engine:stockfish"),
    source: { type: "local" },
    time: { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
    status: "finished",
    outcome: "white_wins",
    ending: "checkmate",
    opening: null,
    plies: 40,
    started_at: "2026-10-27T10:00:00Z",
    ended_at: "2026-10-27T10:20:00Z",
    you: "white",
    accuracy: null,
    url: null,
    ...patch,
  },
});

describe("a rematch", () => {
  const cx = { hasLevels: (id: string) => id !== "fixed" };

  it("swaps the colours and keeps the opponent and the clock", () => {
    expect(rematchRequest(gameOf(), cx)).toEqual({
      white: "stockfish",
      black: "you",
      time: { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
    });
    const asBlack = gameOf({ you: "black", white: seat("engine:stockfish"), black: seat("you") });
    expect(rematchRequest(asBlack, cx)).toMatchObject({ white: "you", black: "stockfish" });
  });

  it("asks for the same engine at the same level", () => {
    const atLevel = gameOf({ black: seat("engine:stockfish", { name: "Stockfish 742", rating: 742, level: 1 }) });
    expect(rematchRequest(atLevel, cx)?.white).toBe("stockfish@1");
    const reckless = gameOf({ black: seat("engine:reckless", { name: "Reckless 1400", rating: 1400, level: 4 }) });
    expect(rematchRequest(reckless, cx)?.white).toBe("reckless@4");
    const fixed = gameOf({ black: seat("engine:fixed", { name: "Fixed", rating: 1500 }) });
    expect(rematchRequest(fixed, cx)?.white).toBe("fixed");
  });

  it("starts from the same position", () => {
    const fen = "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3";
    expect(rematchRequest(gameOf({}, fen), cx)?.from).toEqual({ type: "position", fen, moves: [] });
    expect(rematchRequest(gameOf(), cx)?.from).toBeUndefined();
  });

  it("is only for a finished local game of yours against an engine", () => {
    expect(rematchRequest(gameOf({ status: "live" }), cx)).toBeNull();
    expect(rematchRequest(gameOf({ you: null }), cx)).toBeNull();
    expect(
      rematchRequest(
        gameOf({ source: { type: "lichess", connection: "you", id: "abc", rated: true, speed: "blitz" } as never }),
        cx,
      ),
    ).toBeNull();
    expect(rematchRequest(gameOf({ black: seat("you"), white: seat("you") }), cx)).toBeNull();
  });
});

// ---- The event stream against a server that is down

describe("the event stream with the server down", () => {
  it("backs off instead of asking hundreds of times a second", async () => {
    vi.useFakeTimers();
    try {
      const { stream } = await import("../src/api/stream");
      let asked = 0;
      vi.stubGlobal(
        "fetch",
        vi.fn(async () => {
          asked++;
          return new Response(JSON.stringify({ error: { code: "unauthorized", message: "no" } }), { status: 401 });
        }),
      );
      stream.start(1000);
      await vi.advanceTimersByTimeAsync(30_000);
      stream.stop();
      expect(asked).toBeGreaterThan(3);
      // 1 + 2 + 4 + 8 seconds, then every ten: about eight tries in half a minute.
      expect(asked).toBeLessThan(14);
    } finally {
      vi.useRealTimers();
    }
  });
});
