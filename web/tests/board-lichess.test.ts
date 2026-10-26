import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { createApp, nextTick, type App as VueApp } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
import { createEos } from "@eos-ui/vue/plugin";
import "@eos-ui/vue/icons/builtin";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { dispatch } from "../src/api/events";
import GameList from "../src/board/GameList.vue";
import type { Match, MatchSummary } from "../src/board/matchText";
import { isMine, offersOf } from "../src/board/lichessMe";

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

const fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const line = [
  ["e2e4", "e4", "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1", "white"],
  ["e7e5", "e5", "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2", "black"],
  ["g1f3", "Nf3", "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2", "white"],
] as const;

/** A game of the user's on Lichess, as the runner records it: white is you. */
function mineGame(over: Partial<Match> = {}, plies = 3): Match {
  const moves = line.slice(0, plies).map(([uci, san, fen_after, side], i) => ({
    ply: i + 1,
    side,
    uci,
    san,
    fen_after,
    clock_white_ms: 590_000,
    clock_black_ms: 595_000,
    eval: null,
    time_ms: 1000,
  }));
  return {
    id: 5,
    white: { name: "dameg", commit: "", human: true, lichess: "dameg", rating: 1650 },
    black: { name: "Alpha", commit: "", lichess: "alpha", rating: 1712, title: "NM" },
    time: { type: "clock", seconds: 600, increment: 5 },
    start_fen: "startpos",
    book: false,
    hash: 16,
    threads: 1,
    adjudicate: false,
    moves,
    created_at: "2026-10-26T10:00:00Z",
    finished_at: null,
    lichess: {
      id: "abcd1234",
      url: "https://lichess.org/abcd1234",
      rated: true,
      speed: "rapid",
      our_side: "white",
      account: "me",
    },
    turn: plies % 2 ? "black" : "white",
    clock_white_ms: 590_000,
    clock_black_ms: 595_000,
    status: "running",
    result: "*",
    reason: null,
    ...over,
  };
}

let game: Match = mineGame();
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
    [/^\/api\/matches\/5$/, () => json(game)],
    [/^\/api\/matches$/, () => json(url.searchParams.get("account") === "me" ? [] : [])],
    [/^\/api\/builds$/, () => json(fixture("builds.json"))],
    [/^\/api\/experiments$/, () => json(fixture("experiments.json"))],
    [/^\/api\/experiments\/\d+$/, () => json(fixture("experiment-3.json"))],
    [/^\/api\/lichess$/, () => json(fixture("lichess-off.json"))],
    [/^\/api\/analysis$/, () => new Response(JSON.stringify({ error: "Analysis is off" }), { status: 409 })],
    [/^\/api\/status$/, () => json(fixture("status.json"))],
    [/^\/api\/jobs$/, () => json(fixture("jobs.json"))],
    [/^\/api\/tunes$/, () => json(fixture("tunes.json"))],
    [/^\/api\/versions$/, () => json(fixture("versions.json"))],
    [/^\/api\/branches$/, () => json(fixture("branches.json"))],
  ];
  const hit = table.find(([p]) => p.test(path));
  return Promise.resolve(hit ? hit[1]() : json({ error: "not found" }));
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

describe("a live game of yours on Lichess", () => {
  it("shows the actions Lichess allows, and keeps analysis off", async () => {
    game = mineGame();
    locked = true;
    await open("/matches/5");
    expect(problems).toEqual([]);
    expect(text()).toContain("Alpha");
    expect(text()).toContain("Analysis is off while you’re playing on Lichess");
    expect(root.querySelector("[role=meter]")).toBeNull();
    expect(buttons()).toEqual(expect.arrayContaining(["Offer draw", "Takeback", "Resign"]));
    expect(buttons()).not.toContain("Abort");
    expect(buttons().join("|")).not.toMatch(/Rematch|Swap colours/);
    expect(calls.filter((c) => c.includes("/api/analys"))).toEqual([]);
    expect(text()).not.toContain("Engine version");
  });

  it("offers Abort before both sides have moved", async () => {
    game = mineGame({}, 1);
    locked = true;
    await open("/matches/5");
    expect(buttons()).toContain("Abort");
    expect(buttons()).not.toContain("Resign");
  });

  it("asks what to do with a draw offer", async () => {
    game = mineGame();
    game.lichess = { ...game.lichess!, draw_offer: "black" };
    locked = true;
    await open("/matches/5");
    expect(text()).toContain("Alpha offers a draw");
    expect(buttons()).toEqual(expect.arrayContaining(["Accept", "Decline"]));
    expect(buttons()).toContain("Offer draw");
  });

  it("counts down to a claim when the opponent leaves, then offers the claims", async () => {
    game = mineGame();
    locked = true;
    await open("/matches/5");
    dispatch({ id: 1, kind: "match.opponent_gone", data: { id: 5, gone: true, claim_win_in_seconds: 0 } });
    await settle();
    expect(text()).toContain("Alpha has left the game");
    expect(buttons()).toEqual(expect.arrayContaining(["Claim victory", "Call it a draw"]));
    dispatch({ id: 2, kind: "match.opponent_gone", data: { id: 5, gone: false } });
    await settle();
    expect(text()).not.toContain("has left the game");
  });

  it("follows an offer that arrives", async () => {
    game = mineGame();
    locked = true;
    await open("/matches/5");
    expect(text()).not.toContain("offers a draw");
    dispatch({ id: 1, kind: "match.offer", data: { id: 5, draw_offer: "black", takeback_offer: null } });
    await settle();
    expect(text()).toContain("Alpha offers a draw");
  });
});

describe("a finished game of yours", () => {
  it("opens with the opening, a link to Lichess and the evaluation Lichess recorded", async () => {
    game = mineGame({ status: "finished", result: "0-1", reason: "resigned" });
    game.lichess = { ...game.lichess!, opening: "C20 King's Pawn Game", our_side: "white" };
    game.moves = game.moves.map((m, i) => ({ ...m, eval: { cp: 20 + i } }));
    locked = false;
    await open("/matches/5");
    expect(problems).toEqual([]);
    expect(text()).toContain("C20 King's Pawn Game");
    expect(text()).toContain("You resigned");
    const link = [...root.querySelectorAll("a")].find((a) => a.textContent?.includes("Open on Lichess"));
    expect(link?.getAttribute("href")).toBe("https://lichess.org/abcd1234");
    expect(buttons()).toContain("Analyse this game");
    expect(root.querySelector("[role=meter]")).not.toBeNull();
    expect(text()).not.toContain("Analysis is off");
  });

  it("cannot be analysed while another game of yours is on", async () => {
    game = mineGame({ status: "finished", result: "0-1", reason: "resigned" });
    locked = true;
    await open("/matches/5");
    expect(buttons()).not.toContain("Analyse this game");
    expect(root.querySelector("[role=meter]")).toBeNull();
    expect(text()).toContain("Analysis is off while you’re playing on Lichess");
  });
});

describe("the free board", () => {
  it("shows no engine while a game of yours is on at Lichess", async () => {
    locked = true;
    await open(`/board?fen=${encodeURIComponent(fen)}&moves=e2e4`);
    expect(text()).toContain("Analysis is off while you’re playing on Lichess");
    expect(root.querySelector("[role=meter]")).toBeNull();
    expect(calls.filter((c) => c.includes("/api/analysis"))).toEqual([]);
    expect(text()).not.toContain("Engine version");
    expect(buttons()).not.toContain("Explorer");
  });

  it("has the explorer as a tab when no game of yours is on", async () => {
    locked = false;
    await open(`/board?fen=${encodeURIComponent(fen)}&moves=e2e4`);
    expect(buttons().some((b) => b.startsWith("Explorer"))).toBe(true);
  });
});

describe("the Games tab", () => {
  const summary = (id: number, over: Partial<MatchSummary> = {}): MatchSummary => ({
    id,
    white: { name: "dameg", commit: "", human: true, rating: 1650 },
    black: { name: "Alpha", commit: "", rating: 1712, title: "NM" },
    time: { type: "clock", seconds: 600, increment: 5 },
    start_fen: "startpos",
    status: "finished",
    result: "1-0",
    reason: "resigned",
    created_at: new Date(Date.now() - 3600_000).toISOString(),
    plies: 40,
    lichess: { id: "x", url: "u", rated: true, speed: "rapid", our_side: "white", account: "me" },
    ...over,
  });

  it("lists your games by opponent and rating, as won or lost", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    const app = createApp(GameList, {
      matches: [
        summary(1),
        summary(2, { result: "0-1" }),
        summary(3, {
          white: { build: 1, name: "main", commit: "abc", title: "BOT" },
          lichess: { id: "y", url: "u", rated: true, speed: "blitz", our_side: "white", account: "bot" },
        }),
      ],
    }).use(createEos({ locale: "en-US", applyTheme: false }));
    app.mount(host);
    await nextTick();
    const t = host.textContent ?? "";
    expect(t).toContain("NM Alpha");
    expect(t).toContain("1712");
    expect(t).toContain("Won");
    expect(t).toContain("Lost");
    expect(t).toContain("Mine");
    expect(t).toContain("Bot");
    app.unmount();
  });
});

describe("offers", () => {
  it("reads from your seat", () => {
    const m = mineGame();
    m.lichess = { ...m.lichess!, draw_offer: "black", takeback_offer: "white" };
    expect(offersOf(m, "white")).toEqual({ theirDraw: true, myDraw: false, theirTakeback: false, myTakeback: true });
    expect(isMine(m)).toBe(true);
    expect(isMine({ lichess: { ...m.lichess, account: "bot" } })).toBe(false);
  });
});
