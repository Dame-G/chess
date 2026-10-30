import { afterEach, describe, expect, it, vi } from "vitest";
import type { ConnectionStatus, Game, GameSummary, LevelsOverview, Seat } from "../src/api/types";
import { rematchRequest, strengthOf } from "../src/board/rematch";
import { botGameOf, botNow, botResults, botTally, botTrend } from "../src/parts/bot";
import { matches, queryOf } from "../src/rail/rail";

const seat = (player: string, extra: Partial<Seat> = {}): Seat => ({
  player,
  name: player === "you" ? "You" : player,
  rating: null,
  title: null,
  ...extra,
});

const blitz = { type: "clock", initial_ms: 180_000, increment_ms: 2000 } as const;

/** One of the bot's games on Lichess: Dameless on the given side against Alpha-Bot. */
function botGame(id: number, patch: Partial<GameSummary> = {}, botSide: "white" | "black" = "white"): GameSummary {
  const bot = seat("dameless", { name: "Dameless v14", rating: 2300 });
  const them = seat("lichess:alpha-bot", { name: "Alpha-Bot", rating: 2250, title: "BOT" });
  return {
    id,
    white: botSide === "white" ? bot : them,
    black: botSide === "white" ? them : bot,
    source: { type: "lichess", connection: "bot", id: `g${id}`, rated: true, speed: "blitz" },
    time: blitz,
    status: "finished",
    outcome: "white_wins",
    ending: "resignation",
    opening: null,
    plies: 60,
    started_at: new Date(Date.UTC(2026, 5, 1, 12, id)).toISOString(),
    ended_at: null,
    you: null,
    accuracy: null,
    url: null,
    ...patch,
  } as GameSummary;
}

describe("the bot's games in the rail", () => {
  it("belong under Dameless, which the server is asked for by player and not by who is You", () => {
    expect(matches(botGame(1), "dameless")).toBe(true);
    expect(matches(botGame(1), "all")).toBe(true);
    expect(matches(botGame(1), "you")).toBe(false);
    // The server's filter for the same rule: Dameless on either side of a game from a source people play.
    expect(queryOf("dameless")).toEqual({ player: "dameless", source: "local,lichess,import" });
  });
});

describe("what the bot is doing", () => {
  const status = (patch: Partial<ConnectionStatus> = {}, bot: Partial<NonNullable<ConnectionStatus["bot"]>> = {}) =>
    ({
      key: "bot",
      configured: true,
      account: null,
      state: "online",
      error: null,
      playing: [],
      challenges: [],
      you: null,
      bot: {
        online: true,
        settings: { version: "champion" },
        seek: { mode: "off", pending: null, note: null },
        bot_games_today: 3,
        bot_limit_until: null,
        ...bot,
      },
      ...patch,
    }) as unknown as ConnectionStatus;
  const name = (id: number) => (id === 7 ? "Alpha-Bot (2251)" : null);

  it("links the game it is playing, and says how many more there are", () => {
    expect(botNow(status({ playing: [7] }), name)).toMatchObject({
      text: "Playing Alpha-Bot (2251)",
      tone: "live",
      game: 7,
      more: 0,
    });
    expect(botNow(status({ playing: [7, 8] }), name).more).toBe(1);
    expect(botNow(status({ playing: [9] }), name).text).toBe("Playing a game");
  });

  it("says why it is idle", () => {
    expect(botNow(status(), name).text).toBe("Waiting for challenges · seeking off");
    const seeking = { seek: { mode: "always", pending: null, note: "Beta-Bot declined (too fast)." } } as const;
    expect(botNow(status({}, seeking), name).text).toBe("Seeking · Beta-Bot declined (too fast).");
    const waiting = {
      seek: {
        mode: "once",
        note: null,
        pending: { opponent: { name: "Gamma-Bot", rating: 2275 }, clock: { initial: 180, increment: 2 }, rated: true },
      },
    } as never;
    expect(botNow(status({}, waiting), name).text).toBe("Seeking: challenged Gamma-Bot (2275)");
  });

  it("says when the daily limit is reached and when it resets", () => {
    const until = new Date(2026, 5, 1, 14, 20).toISOString();
    const limited = botNow(status({}, { bot_limit_until: until as never }), name);
    expect(limited.text).toMatch(/^Daily bot-game limit reached, resets 14:20/);
    expect(limited.tone).toBe("warn");
  });

  it("says when it is not playing at all", () => {
    expect(botNow(status({ configured: false, bot: null }), name).text).toBe("No token yet");
    expect(botNow(status({}, { online: false }), name)).toMatchObject({ text: "Offline", tone: "off" });
    expect(botNow(status({}, { online: false, settings: { version: null } as never }), name).tone).toBe("warn");
    expect(botNow(status({ state: "error", error: "Lichess is down" }, { online: false }), name).text).toBe(
      "Lichess is down",
    );
  });
});

describe("how the bot has done", () => {
  it("reads a game from the bot's side, whichever colour it had", () => {
    expect(botGameOf(botGame(1))).toMatchObject({ result: "W", opponent: "Alpha-Bot", rating: 2250, own: 2300 });
    expect(botGameOf(botGame(2, {}, "black"))).toMatchObject({ result: "L" });
    expect(botGameOf(botGame(3, { outcome: "black_wins" }, "black"))).toMatchObject({ result: "W" });
    expect(botGameOf(botGame(4, { outcome: "draw" }))).toMatchObject({ result: "D" });
  });

  it("leaves out games that are not the bot's, and games not finished", () => {
    expect(botGameOf(botGame(1, { status: "live", outcome: null }))).toBeNull();
    expect(botGameOf(botGame(1, { source: { type: "local" } }))).toBeNull();
    expect(
      botGameOf(botGame(1, { source: { type: "lichess", connection: "you", id: "x", rated: true, speed: "blitz" } })),
    ).toBeNull();
  });

  it("lists the newest first and counts won, drawn and lost", () => {
    const games = [
      botGame(1),
      botGame(2, { outcome: "draw" }),
      botGame(3, { outcome: "black_wins" }),
      botGame(4, { source: { type: "local" } }),
    ];
    const results = botResults(games);
    expect(results.map((r) => r.id)).toEqual([3, 2, 1]);
    expect(botTally(results)).toEqual({ w: 1, d: 1, l: 1 });
    expect(botResults(games, 2)).toHaveLength(2);
  });

  it("follows the rating it went into each game with, and ends at the rating now", () => {
    const at = (id: number, own: number) => botGame(id, { white: seat("dameless", { rating: own }) });
    const results = botResults([at(1, 2300), at(2, 2310), at(3, 2295)]);
    const trend = botTrend(results, "blitz", 2320);
    expect(trend.rating).toEqual([2300, 2310, 2295, 2320]);
    expect(trend.change).toBe(20);
    expect(trend.x).toHaveLength(4);
    // No games at that speed: nothing to follow.
    expect(botTrend(results, "rapid", 2320)).toEqual({ x: [], rating: [], change: null });
  });
});

describe("playing again one level up or down", () => {
  const level = (id: number, nodes: number | null, nominal: number) => ({
    level: { id, name: `Level ${id}`, nodes, multipv: 1, temperature: 0, nominal },
    games: 40,
    rating: { elo: nominal, low: nominal - 100, high: nominal + 100 },
  });
  const levels = {
    levels: [level(5, 1500, 1600), level(6, 3000, 1800), level(12, null, 3000)],
    suggested: 5,
    you: null,
    updated_at: null,
  } as unknown as LevelsOverview;
  const game = (them: Partial<Seat>): Pick<Game, "summary" | "start_fen"> =>
    ({
      start_fen: null,
      summary: {
        id: 1,
        white: seat("you"),
        black: seat("dameless", { name: "Dameless 1600", rating: 1600, level: 5, version: 14, ...them }),
        source: { type: "local" },
        time: { type: "movetime", ms: 1000 },
        status: "finished",
        outcome: "white_wins",
        you: "white",
      },
    }) as never;
  const cx = { champion: 14, levels };

  it("reads the level from the seat, and full strength when it has none", () => {
    expect(strengthOf(game({}), levels)).toBe(5);
    expect(strengthOf(game({ level: null, name: "v14", rating: null }), levels)).toBe("full");
  });

  it("asks for the neighbouring level with the colours swapped and the same clock", () => {
    expect(rematchRequest(game({}), cx, 6)).toEqual({
      white: "dameless@6",
      black: "you",
      time: { type: "movetime", ms: 1000 },
    });
    expect(rematchRequest(game({}), cx, "full")?.white).toBe("dameless");
    expect(rematchRequest(game({}), cx)?.white).toBe("dameless@5");
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
