import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { createApp, nextTick, type App as VueApp } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
import { createEos } from "@eos-ui/vue/plugin";
import "@eos-ui/vue/icons/builtin";

// uPlot paints on a canvas, which the test DOM does not have: a minimal stand-in keeps the chart's own code running.
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

const fixture = (name: string) => readFileSync(join(process.cwd(), "tests", "fixtures", name), "utf8");
const json = (name: string) => new Response(fixture(name), { headers: { "content-type": "application/json" } });
const text = (name: string) => new Response(fixture(name), { headers: { "content-type": "text/plain" } });

/** The runner, as recorded: every route the pages ask for, answered from `tests/fixtures`. */
function runner(input: RequestInfo | URL): Promise<Response> {
  const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "http://x");
  const path = url.pathname;
  if (path === "/api/events") return new Promise(() => {});
  const routes: [RegExp, () => Response][] = [
    [/^\/api\/status$/, () => json("status.json")],
    [/^\/api\/experiments$/, () => json("experiments.json")],
    [/^\/api\/experiments\/3$/, () => json("experiment-3.json")],
    [/^\/api\/builds$/, () => json("builds.json")],
    [/^\/api\/branches$/, () => json("branches.json")],
    [/^\/api\/jobs$/, () => json("jobs.json")],
    [/^\/api\/jobs\/\d+\/log$/, () => new Response("log line\n")],
    [/^\/api\/tests\/2$/, () => json("test-2.json")],
    [/^\/api\/tests\/2\/games$/, () => json("games-2.json")],
    [/^\/api\/tests\/2\/games\/\d+$/, () => text("game-2-0.pgn")],
    [/^\/api\/analysis$/, () => Response.json({ session: 1 })],
    [
      /^\/api\/matches$/,
      () => json(url.searchParams.get("source") === "lichess" ? "matches-lichess.json" : "matches.json"),
    ],
    [/^\/api\/matches\/1$/, () => json("match-1.json")],
    [/^\/api\/matches\/2$/, () => json("match-2.json")],
    [/^\/api\/matches\/3$/, () => json("match-3.json")],
    [/^\/api\/matches\/4$/, () => json("match-4.json")],
    [/^\/api\/lichess$/, () => json(lichess)],
    [/^\/api\/lichess\/bots$/, () => json("bots.json")],
  ];
  const hit = routes.find(([pattern]) => pattern.test(path));
  if (!hit) missing.push(`${path}`);
  return Promise.resolve(hit ? hit[1]() : new Response(JSON.stringify({ error: "not found" }), { status: 404 }));
}

/** Which state the Lichess bot is in for the page under test. */
let lichess = "lichess-off.json";

const fen = encodeURIComponent("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
const routes: [string, string, string?][] = [
  ["/", "Open the board"],
  ["/board", "Start position"],
  [`/board?fen=${fen}&moves=e2e4`, "1. e4"],
  ["/games/2/0", "Nd7"],
  ["/matches/1", "Black (test/no-nmp) won on time"],
  ["/board?mode=play", "Play against"],
  ["/board?mode=engines", "Start match"],
  ["/matches/3", "Your move"],
  ["/matches/4", "You lost on time"],
  ["/lab", "test/no-nmp vs main"],
  ["/lab/3", "test/no-nmp vs main"],
  ["/lab/3?stage=bench", "test/no-nmp vs main"],
  ["/engines", "Analyse with it"],
  ["/engines/1", "Fingerprint matches"],
  ["/lichess", "Create the token"],
  ["/lichess", "is not a bot account yet", "lichess-person.json"],
  ["/lichess", "Stockfish-Bot", "lichess-connected.json"],
  ["/lichess", "Playing a game", "lichess-connected.json"],
  ["/lichess", "Challenged Alpha-Bot (1950) to 3+2 rated.", "lichess-seeking.json"],
  ["/lichess", "Find a game", "lichess-connected.json"],
  ["/matches/2", "Black resigned", "lichess-connected.json"],
  ["/matches/2", "View on Lichess", "lichess-connected.json"],
];

let mounted: VueApp | undefined;
const problems: string[] = [];
/** Requests the fixtures do not answer, so a new call cannot go unmocked. */
const missing: string[] = [];

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

describe("routes", () => {
  it.each(routes)(
    "%s renders its content without errors or warnings",
    { timeout: 30_000 },
    async (path, shows, bot) => {
      lichess = bot ?? "lichess-off.json";
      problems.length = 0;
      missing.length = 0;
      const { default: App } = await import("../src/App.vue");
      const { router } = await import("../src/router");
      const root = document.createElement("div");
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
      expect(problems).toEqual([]);
      expect(root.textContent).toContain(shows);
      expect(missing).toEqual([]);
    },
  );
});
