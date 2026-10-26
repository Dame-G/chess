import { afterEach, describe, expect, it, vi } from "vitest";
import { createApp, h, nextTick, reactive, type App } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
import { createMemoryHistory, createRouter } from "vue-router";
import { createEos } from "@eos-ui/vue/plugin";
import "@eos-ui/vue/icons/builtin";
import Explorer from "../src/board/Explorer.vue";
import Tablebase from "../src/board/Tablebase.vue";
import { shares, standardUci } from "../src/board/useExplorer";
import { pieceCount, rowsOf, type TablebaseAnswer } from "../src/board/useTablebase";

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const CASTLE = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";

let app: App | undefined;
afterEach(() => {
  app?.unmount();
  app = undefined;
  vi.unstubAllGlobals();
});

type Reply = { status?: number; body: unknown };
type Routes = Record<string, Reply | ((url: URL) => Reply)>;

async function mount(component: object, props: object, routes: Routes) {
  const calls: URL[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: Request | string) => {
      const req = typeof input === "string" ? new Request(new URL(input, "http://x")) : input;
      const url = new URL(req.url);
      calls.push(url);
      const r = routes[url.pathname];
      const reply = typeof r === "function" ? r(url) : (r ?? { status: 404, body: { error: "no route" } });
      return Response.json(reply.body, { status: reply.status ?? 200 });
    }),
  );
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/:rest(.*)*", component: {} }] });
  const root = document.createElement("div");
  document.body.append(root);
  app = createApp({ render: () => h(component, props) })
    .use(createEos({ locale: "en-US", applyTheme: false }))
    .use(createPinia())
    .use(PiniaColada)
    .use(router);
  app.mount(root);
  await settle();
  return { root, calls };
}
const settle = async () => {
  await new Promise((r) => setTimeout(r, 60));
  await nextTick();
};

const me = { body: { account: { username: "damon" } } };
const answer = {
  white: 600,
  draws: 300,
  black: 100,
  opening: { eco: "B00", name: "King's Pawn" },
  moves: [
    { uci: "e2e4", san: "e4", white: 400, draws: 200, black: 50, averageRating: 2210 },
    { uci: "d2d4", san: "d4", white: 200, draws: 100, black: 50, averageRating: 2190 },
  ],
  topGames: [
    {
      id: "abcd1234",
      winner: "white",
      white: { name: "Alpha", rating: 2700 },
      black: { name: "Beta", rating: 2650 },
      year: 2019,
    },
  ],
};

describe("explorer", () => {
  it("lists moves with counts and the result bar, and plays a row", async () => {
    const { root, calls } = await mount(
      Explorer,
      { fen: START },
      {
        "/api/lichess/me": me,
        "/api/lichess/explorer": { body: answer },
      },
    );
    expect(root.textContent).toContain("King's Pawn");
    expect(root.textContent).toContain("e4");
    expect(root.textContent).toContain("650");
    expect(root.textContent).toContain("Alpha 2700 – Beta 2650");
    const call = calls.find((u) => u.pathname === "/api/lichess/explorer");
    expect(call?.searchParams.get("db")).toBe("lichess");
    expect(call?.searchParams.get("fen")).toBe(START);
    expect(call?.searchParams.get("speeds")).toBe("blitz,rapid,classical");
    const bar = root.querySelector('[aria-label="White 60%, draws 30%, black 10%"]');
    expect(bar).not.toBeNull();
    expect(root.querySelector("a[href='https://lichess.org/abcd1234']")).not.toBeNull();
  });

  it("emits the move in standard UCI", async () => {
    const castle = { ...answer, moves: [{ uci: "e1h1", san: "O-O", white: 5, draws: 3, black: 2 }], topGames: [] };
    const { root } = await mount(
      Explorer,
      { fen: CASTLE },
      { "/api/lichess/me": me, "/api/lichess/explorer": { body: castle } },
    );
    const row = [...root.querySelectorAll("button")].find((b) => b.textContent?.includes("O-O"));
    expect(row).toBeDefined();
    let played = "";
    app?.unmount();
    // Re-mount with a listener.
    const again = await mount(
      Explorer,
      { fen: CASTLE, onPlay: (u: string) => (played = u) },
      { "/api/lichess/me": me, "/api/lichess/explorer": { body: castle } },
    );
    [...again.root.querySelectorAll("button")].find((b) => b.textContent?.includes("O-O"))?.click();
    expect(played).toBe("e1g1");
  });

  it("asks to log in on 409", async () => {
    const { root } = await mount(
      Explorer,
      { fen: START },
      {
        "/api/lichess/me": { status: 409, body: { error: "x" } },
        "/api/lichess/explorer": {
          status: 409,
          body: { error: "Lichess's opening explorer needs a login: connect your Lichess account first" },
        },
      },
    );
    expect(root.textContent).toContain("Log in to your Lichess account to use the explorer");
    expect(root.querySelector("a[href='/lichess']")).not.toBeNull();
  });

  it("is calm when the fair-play lock is on", async () => {
    const { root } = await mount(
      Explorer,
      { fen: START },
      {
        "/api/lichess/me": me,
        "/api/lichess/explorer": { status: 409, body: { error: "Analysis is off while you're playing on Lichess" } },
      },
    );
    expect(root.textContent).toContain("Analysis is off while you're playing on Lichess");
    expect(root.textContent).not.toContain("Log in");
  });

  it("says when nobody has reached the position, and when it is rate-limited", async () => {
    const empty = await mount(
      Explorer,
      { fen: START },
      {
        "/api/lichess/me": me,
        "/api/lichess/explorer": { body: { white: 0, draws: 0, black: 0, moves: [] } },
      },
    );
    expect(empty.root.textContent).toContain("No games");
    app?.unmount();
    const limited = await mount(
      Explorer,
      { fen: START },
      {
        "/api/lichess/me": me,
        "/api/lichess/explorer": { status: 429, body: { error: "slow" } },
      },
    );
    expect(limited.root.textContent).toContain("slow down");
  });

  it("asks nothing when disabled", async () => {
    const { root, calls } = await mount(Explorer, { fen: START, disabled: true }, { "/api/lichess/me": me });
    expect(root.querySelector(".explorer")).toBeNull();
    expect(calls.filter((u) => u.pathname === "/api/lichess/explorer")).toHaveLength(0);
  });

  it("asks once while the position changes quickly, and keeps the old rows meanwhile", async () => {
    const props = reactive({ fen: START, wait: 40 });
    const { root, calls } = await mount(Explorer, props, {
      "/api/lichess/me": me,
      "/api/lichess/explorer": { body: answer },
    });
    const asked = () => calls.filter((u) => u.pathname === "/api/lichess/explorer");
    expect(asked()).toHaveLength(1);
    props.fen = CASTLE;
    await nextTick();
    props.fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
    await nextTick();
    expect(root.textContent).toContain("e4");
    await settle();
    expect(asked()).toHaveLength(2);
    expect(asked()[1]?.searchParams.get("fen")).toBe("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
  });
});

describe("explorer helpers", () => {
  it("rounds shares to 100", () => {
    expect(shares(1, 1, 1)).toEqual({ total: 3, white: 33, draws: 34, black: 33 });
    expect(shares(0, 0, 0).total).toBe(0);
  });
  it("turns king-takes-rook into the standard castling move", () => {
    expect(standardUci(CASTLE, "e1h1")).toBe("e1g1");
    expect(standardUci(CASTLE, "e1a1")).toBe("e1c1");
    expect(standardUci(CASTLE, "e1d1")).toBe("e1d1");
  });
});

const KPK = "8/8/8/4k3/8/8/4P3/4K3 w - - 0 1";
const won: TablebaseAnswer = {
  category: "win",
  dtz: 4,
  dtm: 9,
  moves: [
    { uci: "e1d1", san: "Kd1", category: "draw", dtz: 0, dtm: 0 },
    { uci: "e2e4", san: "e4", category: "loss", dtz: -3, dtm: -7 },
    { uci: "e2e3", san: "e3", category: "loss", dtz: -3, dtm: -5 },
    { uci: "e1f1", san: "Kf1", category: "win", dtz: 2, dtm: 12 },
  ],
};

describe("tablebase", () => {
  it("counts pieces", () => {
    expect(pieceCount(START)).toBe(32);
    expect(pieceCount(KPK)).toBe(3);
  });

  it("sorts best first, turning each outcome round for the mover", () => {
    const rows = rowsOf(KPK, won);
    expect(rows.map((r) => r.san)).toEqual(["e3", "e4", "Kd1", "Kf1"]);
    expect(rows.map((r) => r.outcome)).toEqual(["win", "win", "draw", "loss"]);
  });

  it("renders nothing and asks nothing above seven pieces", async () => {
    const { root, calls } = await mount(Tablebase, { fen: START }, {});
    expect(root.querySelector(".tb")).toBeNull();
    expect(calls).toHaveLength(0);
  });

  it("shows the verdict and moves, and plays one", async () => {
    let played = "";
    const { root } = await mount(
      Tablebase,
      { fen: KPK, onPlay: (u: string) => (played = u) },
      { "/api/lichess/tablebase": { body: won } },
    );
    expect(root.textContent).toContain("Win");
    expect(root.textContent).toContain("Mate in 5");
    const buttons = [...root.querySelectorAll("button")];
    expect(buttons[0]?.textContent).toContain("e3");
    buttons[0]?.click();
    expect(played).toBe("e2e3");
  });

  it("handles the fair-play lock", async () => {
    const { root } = await mount(
      Tablebase,
      { fen: KPK },
      {
        "/api/lichess/tablebase": { status: 409, body: { error: "Analysis is off while you're playing on Lichess" } },
      },
    );
    expect(root.textContent).toContain("Analysis is off while you're playing on Lichess");
  });
});
