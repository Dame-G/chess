import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { createApp, nextTick, type App as VueApp } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
import { createEos } from "@eos-ui/vue/plugin";
import "@eos-ui/vue/icons/builtin";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import GameList from "../src/board/GameList.vue";
import type { Match, MatchSummary } from "../src/board/matchText";
import { findPly, hint, pawnsOf, reviewSeries, scoreText, type PlyReview, type Review } from "../src/board/review";

vi.mock("uplot", () => ({
  default: class {
    constructor(_options: unknown, _data: unknown, parent: HTMLElement) {
      parent.append(document.createElement("div"));
    }
    setSize() {}
    setData() {}
    setScale() {}
    destroy() {}
  },
}));
vi.mock("uplot/dist/uPlot.min.css", () => ({}));

// The first test pulls in the whole app.
vi.setConfig({ testTimeout: 30_000 });

const fixture = (name: string) => readFileSync(join(process.cwd(), "tests", "fixtures", name), "utf8");
const json = (body: unknown, status = 200) =>
  new Response(typeof body === "string" ? body : JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

const line = [
  ["e2e4", "e4", "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1", "white"],
  ["e7e5", "e5", "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2", "black"],
  ["f2f3", "f3", "rnbqkbnr/pppp1ppp/8/4p3/4P3/5P2/PPPP2PP/RNBQKBNR b KQkq - 0 2", "white"],
] as const;

function finished(): Match {
  return {
    id: 5,
    white: { name: "dameg", commit: "", human: true, lichess: "dameg", rating: 1650 },
    black: { name: "Alpha", commit: "", lichess: "alpha", rating: 1712 },
    time: { type: "clock", seconds: 600, increment: 5 },
    start_fen: "startpos",
    book: false,
    hash: 16,
    threads: 1,
    adjudicate: false,
    moves: line.map(([uci, san, fen_after, side], i) => ({
      ply: i + 1,
      side,
      uci,
      san,
      fen_after,
      clock_white_ms: 590_000,
      clock_black_ms: 595_000,
      eval: null,
      time_ms: 1000,
    })),
    created_at: "2026-10-26T10:00:00Z",
    finished_at: "2026-10-26T10:10:00Z",
    lichess: {
      id: "abcd1234",
      url: "https://lichess.org/abcd1234",
      rated: true,
      speed: "rapid",
      our_side: "white",
      account: "me",
    },
    turn: "black",
    clock_white_ms: 590_000,
    clock_black_ms: 595_000,
    status: "finished",
    result: "0-1",
    reason: "resigned",
  };
}

const counts = {
  brilliant: 0,
  great: 0,
  best: 1,
  excellent: 0,
  good: 0,
  book: 0,
  inaccuracy: 0,
  mistake: 0,
  blunder: 1,
  miss: 0,
};
const side = (accuracy: number) => ({
  moves: 2,
  accuracy,
  acpl: 30,
  counts,
  judgements: { inaccuracy: 0, mistake: 0, blunder: 1 },
  phases: {} as never,
  time_trouble_blunders: 0,
});
const ply = (n: number, over: Partial<PlyReview>): PlyReview => ({
  ply: n,
  side: n % 2 ? "white" : "black",
  uci: line[n - 1]![0],
  san: line[n - 1]![1],
  eval_before: { cp: 20 },
  eval_after: { cp: 25 },
  best_uci: line[n - 1]![0],
  best_san: line[n - 1]![1],
  best_pv: [line[n - 1]![0]],
  win_before: 52,
  win_after: 52,
  accuracy: 100,
  cp_loss: 0,
  classification: "best",
  phase: "opening",
  sacrifice: 0,
  depth: 14,
  ...over,
});
const plies: PlyReview[] = [
  ply(1, {}),
  ply(2, {}),
  ply(3, {
    classification: "blunder",
    judgement: "blunder",
    eval_after: { cp: -310 },
    best_uci: "g1f3",
    best_san: "Nf3",
    best_pv: ["g1f3", "b8c6"],
    cp_loss: 330,
    accuracy: 4,
  }),
];
const review: Review = {
  id: 1,
  match_id: 5,
  build: 2,
  nodes: 200_000,
  created_at: "2026-10-26T10:20:00Z",
  summary: { build: 2, nodes: 200_000, white: side(61.5), black: side(97), middlegame_ply: null, endgame_ply: null },
  plies,
};

let reviewed: Review | null = review;
let locked = false;
const calls: string[] = [];

function runner(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "http://x");
  const path = url.pathname;
  calls.push(`${init?.method ?? (input instanceof Request ? input.method : "GET")} ${path}`);
  if (path === "/api/events") return new Promise(() => {});
  const me = { ...JSON.parse(fixture("lichess-me-off.json")), fair_play_lock: locked };
  const table: [RegExp, () => Response][] = [
    [/^\/api\/lichess\/me$/, () => json(me)],
    [/^\/api\/matches\/5$/, () => json(finished())],
    [/^\/api\/matches\/5\/review$/, () => (reviewed ? json(reviewed) : json({ error: "no review" }, 404))],
    [/^\/api\/reviews$/, () => json({ id: 7, spec: { kind: "review", matches: [5] }, status: "queued" }, 202)],
    [/^\/api\/matches$/, () => json([])],
    [/^\/api\/builds$/, () => json(fixture("builds.json"))],
    [/^\/api\/experiments$/, () => json(fixture("experiments.json"))],
    [/^\/api\/experiments\/\d+$/, () => json(fixture("experiment-3.json"))],
    [/^\/api\/lichess$/, () => json(fixture("lichess-off.json"))],
    [/^\/api\/status$/, () => json(fixture("status.json"))],
    [/^\/api\/jobs$/, () => json([])],
    [/^\/api\/tunes$/, () => json(fixture("tunes.json"))],
    [/^\/api\/versions$/, () => json(fixture("versions.json"))],
    [/^\/api\/branches$/, () => json(fixture("branches.json"))],
  ];
  const hit = table.find(([p]) => p.test(path));
  return Promise.resolve(hit ? hit[1]() : json({ error: "not found" }, 404));
}

let mounted: VueApp | undefined;
let root: HTMLElement;
const problems: string[] = [];

beforeAll(() => {
  vi.stubGlobal("fetch", vi.fn(runner));
});
afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
});

async function settle() {
  for (let i = 0; i < 12; i++) {
    await new Promise((r) => setTimeout(r, 10));
    await nextTick();
  }
}

async function open(path: string) {
  problems.length = 0;
  calls.length = 0;
  const { default: App } = await import("../src/App.vue");
  const { router } = await import("../src/router");
  root = document.createElement("div");
  document.body.append(root);
  const app = createApp(App);
  app.config.errorHandler = (e, _vm, info) => problems.push(`error in ${info}: ${String(e)}`);
  app.config.warnHandler = (msg) => problems.push(`warning: ${msg}`);
  app
    .use(createEos({ locale: "en-US", applyTheme: false }))
    .use(createPinia())
    .use(PiniaColada, { queryOptions: { staleTime: 5_000 } })
    .use(router);
  mounted = app;
  await router.push(path);
  await router.isReady();
  app.mount(root);
  await settle();
}

const text = () => root.textContent ?? "";
const buttons = () => [...root.querySelectorAll("button, a")].map((b) => b.textContent?.trim() ?? "");
const counter = () => root.querySelector(".counter")?.textContent?.replace(/\s+/g, " ").trim();
const askButton = () => [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === "Review this game")!;

describe("the review on the board", () => {
  it("marks the move list, summarises both sides and explains the move on the board", async () => {
    reviewed = review;
    locked = false;
    await open("/matches/5?ply=3");
    expect(problems).toEqual([]);
    // Opens at the ply the address names, as the Compare screen links to it.
    expect(counter()).toContain("2. f3");
    expect(text()).toContain("61.5");
    expect(text()).toContain("97");
    const blunder = root.querySelector('[data-ply="3"]');
    expect(blunder?.textContent).toContain("??");
    expect(blunder?.getAttribute("title")).toBe("Blunder. Best was Nf3 (+0.20)");
    expect(root.querySelector('[data-ply="1"]')?.textContent).not.toMatch(/[?!]/);
    expect(text()).toContain("Blunder. Best was Nf3");
    expect(buttons()).toEqual(expect.arrayContaining(["Show best move", "Play best line"]));
    expect(buttons()).not.toContain("Review this game");
  });

  it("walks to the mistake with X", async () => {
    reviewed = review;
    locked = false;
    await open("/matches/5?ply=0");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "x" }));
    await settle();
    expect(counter()).toContain("2. f3");
  });

  it("offers a review of a game that has none, and asks the runner for it", async () => {
    reviewed = null;
    locked = false;
    await open("/matches/5");
    expect(text()).toContain("Not reviewed yet");
    expect(root.querySelector('[data-ply="3"]')?.textContent).not.toContain("?");
    expect(askButton().disabled).toBe(false);
    askButton().click();
    await settle();
    expect(calls).toContain("POST /api/reviews");
    expect(text()).toContain("In the queue for review");
  });

  it("shows nothing of a review, and cannot ask for one, while a game of yours is on at Lichess", async () => {
    reviewed = review;
    locked = true;
    await open("/matches/5");
    expect(calls.filter((c) => c.includes("/review"))).toEqual([]);
    expect(root.querySelector('[data-ply="3"]')?.textContent).not.toContain("?");
    expect(text()).not.toContain("accuracy");
    expect(askButton().disabled).toBe(true);
    expect(text()).toContain("Reviews start again when your game on Lichess ends");
  });
});

describe("the review's words", () => {
  it("reads scores for people", () => {
    expect(scoreText({ cp: 45 }, "white")).toBe("+0.45");
    expect(scoreText({ cp: -120 }, "black")).toBe("−1.20");
    expect(scoreText({ mate: 3 }, "white")).toBe("+M3");
    expect(scoreText({ mate: -2 }, "white")).toBe("−M2");
    expect(scoreText({ mate: 0 }, "black")).toBe("−Mate");
    expect(pawnsOf({ mate: 0 }, "white")).toBe(100);
    expect(pawnsOf({ cp: 250 }, "black")).toBe(2.5);
  });

  it("says what a move was and what was better", () => {
    expect(hint(plies[0]!)).toBe("Best move");
    expect(hint(plies[2]!)).toBe("Blunder. Best was Nf3 (+0.20)");
    expect(hint({ ...plies[2]!, classification: "brilliant", sacrifice: 3 })).toContain("gives up about 3 pawns");
  });

  it("finds the next and previous move worth a look, wrapping at the ends", () => {
    const worth = (p: PlyReview) => p.classification === "blunder";
    expect(findPly(plies, 0, worth)).toBe(3);
    expect(findPly(plies, 3, worth)).toBe(3);
    expect(findPly(plies, 3, (p) => p.ply === 2, true)).toBe(2);
    expect(findPly(plies, 2, (p) => p.ply === 1, true)).toBe(1);
    expect(findPly(plies, 0, () => false)).toBeUndefined();
  });

  it("draws the graph from the review: the start, then each move's score", () => {
    const s = reviewSeries(plies, (p) => p);
    expect(s.values).toEqual([0.2, 0.25, 0.25, -3.1]);
    expect(s.texts[3]).toBe("−3.10");
  });
});

describe("accuracy in the Games tab", () => {
  const summary = (over: Partial<MatchSummary> = {}): MatchSummary => ({
    id: 1,
    white: { name: "dameg", commit: "", human: true, rating: 1650 },
    black: { name: "Alpha", commit: "", rating: 1712 },
    time: { type: "clock", seconds: 600, increment: 5 },
    start_fen: "startpos",
    status: "finished",
    result: "1-0",
    reason: "resigned",
    created_at: new Date(Date.now() - 3600_000).toISOString(),
    plies: 40,
    lichess: { id: "x", url: "u", rated: true, speed: "rapid", our_side: "black", account: "me" },
    review: {
      id: 1,
      build: 2,
      nodes: 200_000,
      white: { accuracy: 71.4, counts },
      black: { accuracy: 88.2, counts },
    },
    ...over,
  });
  const rows = async (locked: boolean) => {
    const host = document.createElement("div");
    document.body.append(host);
    const app = createApp(GameList, { matches: [summary()], locked }).use(
      createEos({ locale: "en-US", applyTheme: false }),
    );
    app.mount(host);
    await nextTick();
    const t = host.textContent ?? "";
    app.unmount();
    return t;
  };

  it("puts your side first, and hides it while a game of yours is on", async () => {
    expect(await rows(false)).toContain("Accuracy 71 v 88");
    expect(await rows(true)).not.toContain("Accuracy");
  });
});
