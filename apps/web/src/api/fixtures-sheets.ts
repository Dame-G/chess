/**
 * The fixtures' answers for the sheets: profiles and comparisons, the leaderboard, the connection and the machine.
 * `fixtures.ts` hands this module the data it shares (games, work, connections) and the way to speak on the event
 * stream; everything here is invented too.
 */
import { error, reply } from "./fixtures-http";
import { settingsRoute } from "./fixtures-settings";
import { leaders } from "./leaders";
import type {
  Classification,
  ClassCounts,
  Connections,
  ConnectionStatus,
  EventKind,
  EventPayloads,
  Game,
  GameId,
  Moment,
  Phase,
  Player,
  PlayerKey,
  Priority,
  Profile,
  ProfileFilter,
  Speed,
  Status,
  ThemeKind,
  ThemeStat,
  LeaderboardEntry,
  WeakSpot,
  Wdl,
  Work,
} from "./types";

export interface SheetContext {
  minutesAgo: (m: number) => string;
  rng: (seed: number) => () => number;
  emit: <K extends EventKind>(kind: K, topic: string, data: EventPayloads[K]) => void;
  work: Work[];
  connections: Connections;
  status: Status;
  games: Map<GameId, { game: Game }>;
  /** The position after the first `plies` moves of a game written in SAN, as a FEN. */
  fenAfter: (san: string, plies: number) => string;
  lines: { italian: string; sicilian: string; qgd: string; london: string; caro: string };
  /** `unlinked`: You has not logged in to Lichess yet. */
  mode: string | null;
}

type Handler = (
  method: string,
  path: string,
  url: URL,
  body: Record<string, unknown>,
) => Response | undefined | Promise<Response>;

// ---- Players

interface Roster {
  name: string;
  title: string | null;
  rating: Partial<Record<Speed, number>>;
  games: number;
  reviewed: number;
  accuracy: number;
  win: number;
  draw: number;
  seed: number;
}

const SHARE: Partial<Record<Speed, number>> = { bullet: 0.18, blitz: 0.55, rapid: 0.24, classical: 0.03 };
const SPEED_ACCURACY: Partial<Record<Speed, number>> = { bullet: -4.2, blitz: 0, rapid: 2.8, classical: 3.5 };

const LEADERBOARD: Record<string, [string, string | null, number][]> = {
  bullet: [
    ["Zhigalko_Sergei", "GM", 3218],
    ["nihalsarin2004", "GM", 3187],
    ["DrNykterstein", "GM", 3172],
    ["penguingim1", "GM", 3139],
    ["Msb2", "GM", 3101],
    ["Ghost_of_Morphy", null, 3077],
    ["opperwezen", "IM", 3044],
    ["lance5500", "LM", 3012],
    ["Fins", "GM", 2987],
    ["Jurasek", null, 2961],
  ],
  blitz: [
    ["DrNykterstein", "GM", 3098],
    ["nihalsarin2004", "GM", 3071],
    ["Zhigalko_Sergei", "GM", 3040],
    ["Msb2", "GM", 3012],
    ["lovlas", "IM", 2985],
    ["penguingim1", "GM", 2963],
    ["Fins", "GM", 2940],
    ["opperwezen", "IM", 2912],
    ["may6enexttime", "GM", 2893],
    ["BabaYagaa", null, 2871],
  ],
  rapid: [
    ["DrNykterstein", "GM", 2980],
    ["alireza2003", "GM", 2951],
    ["nihalsarin2004", "GM", 2930],
    ["Msb2", "GM", 2904],
    ["lovlas", "IM", 2880],
    ["Fins", "GM", 2861],
    ["penguingim1", "GM", 2837],
    ["zhigalko_sergei", "GM", 2815],
    ["Oleksandr_Bortnyk", "GM", 2794],
    ["Jurasek", null, 2768],
  ],
};

const OPENING_FAMILIES = [
  "Italian Game",
  "Sicilian Defense",
  "French Defense",
  "Caro-Kann Defense",
  "Queen's Gambit Declined",
  "London System",
  "Scandinavian Defense",
  "Philidor Defense",
];

function wdl(games: number, win: number, draw: number): Wdl {
  const wins = Math.round(games * win);
  const draws = Math.round(games * draw);
  const losses = Math.max(0, games - wins - draws);
  const played = wins + draws + losses;
  return {
    games,
    wins,
    draws,
    losses,
    score: played ? Math.round((1000 * (wins + draws / 2)) / played) / 10 : null,
  };
}

const classes: Classification[] = [
  "brilliant",
  "great",
  "best",
  "excellent",
  "good",
  "book",
  "inaccuracy",
  "mistake",
  "blunder",
  "miss",
];

function emptyCounts(): ClassCounts {
  return Object.fromEntries(classes.map((c) => [c, 0])) as ClassCounts;
}

const round = (n: number, d = 1) => Math.round(n * 10 ** d) / 10 ** d;

export function createSheetRoutes(ctx: SheetContext): Handler {
  const { emit, minutesAgo } = ctx;

  // ---- The people

  const roster = new Map<string, Roster>();
  roster.set("you", {
    name: "dame_g",
    title: null,
    rating: { bullet: 1722, blitz: 1840, rapid: 1846, classical: 1790 },
    games: 64,
    reviewed: 41,
    accuracy: 79.4,
    win: 0.53,
    draw: 0.08,
    seed: 101,
  });
  roster.set("engine:stockfish", {
    name: "Stockfish",
    title: null,
    rating: { bullet: 2281, blitz: 2304, rapid: 2310 },
    games: 180,
    reviewed: 180,
    accuracy: 93.6,
    win: 0.71,
    draw: 0.15,
    seed: 202,
  });
  roster.set("lichess:cutemouse83", {
    name: "cutemouse83",
    title: null,
    rating: { blitz: 1871, rapid: 1862 },
    games: 0,
    reviewed: 0,
    accuracy: 82.1,
    win: 0.5,
    draw: 0.06,
    seed: 303,
  });
  roster.set("lichess:knightowl", {
    name: "knightowl",
    title: "FM",
    rating: { blitz: 1802, rapid: 1790 },
    games: 22,
    reviewed: 22,
    accuracy: 84.9,
    win: 0.55,
    draw: 0.1,
    seed: 404,
  });

  // A long name and a thousand games, to see how the player sheets and Compare hold up.
  roster.set("lichess:the_quick_brown_fox_jumps_over", {
    name: "The_Quick_Brown_Fox_Jumps_Over",
    title: "GM",
    rating: { bullet: 2701, blitz: 2744, rapid: 2688, classical: 2610 },
    games: 1243,
    reviewed: 1243,
    accuracy: 91.2,
    win: 0.63,
    draw: 0.2,
    seed: 505,
  });

  // States to look at: `lichess:broken…` fails to load, `lichess:slow…` loads slowly, `lichess:nogames` has no games
  // here; You is the same with `?fixtures=youbroken`, `?fixtures=slow`, `?fixtures=newcomer` (no games at all) and `?fixtures=unreviewed` (3,708 games, none reviewed).
  roster.set("lichess:nogames", {
    name: "nogames",
    title: null,
    rating: { rapid: 1500 },
    games: 0,
    reviewed: 0,
    accuracy: 80,
    win: 0.5,
    draw: 0.05,
    seed: 606,
  });
  if (ctx.mode === "newcomer") Object.assign(roster.get("you")!, { games: 0, reviewed: 0 });
  // The real first run: the whole Lichess history imported (3,708 games) and not one of them reviewed yet.
  if (ctx.mode === "unreviewed") Object.assign(roster.get("you")!, { games: 3708, reviewed: 0 });
  const profileQuirk = (key: string): "broken" | "slow" | null =>
    /^lichess:broken/.test(key) || (key === "you" && ctx.mode === "youbroken")
      ? "broken"
      : /^lichess:slow/.test(key) || (key === "you" && ctx.mode === "slow")
        ? "slow"
        : null;

  /** A name Lichess knows: anything shaped like a name, except the ones the fixtures say are not there. */
  const exists = (key: string) => !/^lichess:(nosuch|nobody|zzz)/.test(key);

  function entry(key: string): Roster | undefined {
    const known = roster.get(key);
    if (known) return known;
    const m = key.match(/^lichess:(.+)$/);
    if (!m || !exists(key)) return undefined;
    const board = Object.values(LEADERBOARD)
      .flat()
      .find(([n]) => n.toLowerCase() === m[1]);
    const seed = [...m[1]!].reduce((a, c) => a * 31 + c.charCodeAt(0), 7) % 9973;
    const r = ctx.rng(seed);
    const entry: Roster = {
      name: board?.[0] ?? m[1]!,
      title: board?.[1] ?? null,
      rating: { blitz: board?.[2] ?? 1500 + Math.round(r() * 700), rapid: 1500 + Math.round(r() * 700) },
      games: 0,
      reviewed: 0,
      accuracy: 78 + r() * 12,
      win: 0.5 + (r() - 0.5) * 0.2,
      draw: 0.08,
      seed,
    };
    roster.set(key, entry);
    return entry;
  }

  function player(key: string): Player | undefined {
    const r = entry(key);
    if (!r) return undefined;
    const ratings: Player["ratings"] = {};
    for (const [speed, rating] of Object.entries(r.rating) as [keyof Player["ratings"], number][])
      ratings[speed] = { rating, games: 120 + Math.round(rating / 10), provisional: false };
    return {
      key: key as PlayerKey,
      name: r.name,
      title: r.title,
      ratings,
      lichess: key.startsWith("lichess:") ? r.name : key === "you" ? r.name : null,
    };
  }

  /** The games, from the roster, that a filter lets through. */
  function build(key: string, filter: ProfileFilter): Profile | undefined {
    const r = entry(key);
    const p = player(key);
    if (!r || !p) return undefined;
    const shares = (Object.keys(SHARE) as Speed[]).filter(
      (s) => r.rating[s] !== undefined && (!filter.speeds?.length || filter.speeds.includes(s)),
    );
    const shareSum = shares.reduce((a, s) => a + (SHARE[s] ?? 0), 0);
    const ratedShare = filter.rated == null ? 1 : filter.rated ? 0.82 : 0.18;
    const games = Math.min(filter.limit, Math.round(r.games * shareSum * ratedShare));
    const reviewed = r.games ? Math.min(games, Math.round((r.reviewed / r.games) * games)) : 0;
    const rnd = ctx.rng(r.seed);
    const rating = r.rating.blitz ?? r.rating.rapid ?? 1500;

    const results = {
      overall: wdl(games, r.win, r.draw),
      white: wdl(Math.ceil(games / 2), r.win + 0.04, r.draw),
      black: wdl(Math.floor(games / 2), r.win - 0.04, r.draw),
    };

    const acc = r.accuracy;
    const accOf = (offset: number) => (reviewed ? round(Math.min(99.4, acc + offset)) : null);
    const by_speed = shares.map((s) => {
      const g = Math.round((games * (SHARE[s] ?? 0)) / (shareSum || 1));
      const rv = Math.min(g, Math.round((reviewed * g) / (games || 1)));
      return {
        speed: s,
        games: g,
        reviewed: rv,
        results: wdl(g, r.win, r.draw),
        accuracy: rv ? round(Math.min(99.4, acc + (SPEED_ACCURACY[s] ?? 0))) : null,
        acpl: rv ? round((100 - (acc + (SPEED_ACCURACY[s] ?? 0))) * 1.7) : null,
      };
    });

    const per = (x: number) => (reviewed ? round(x, 2) : 0);
    const lossy = Math.max(0.1, (96 - acc) / 5);
    const rates = {
      brilliant: per(0.05 + (acc - 70) / 400),
      great: per(0.3 + (acc - 70) / 120),
      best: per(11 + (acc - 70) / 3),
      excellent: per(7),
      good: per(8 - (acc - 70) / 8),
      book: per(4.4),
      inaccuracy: per(lossy * 1.3),
      mistake: per(lossy * 0.75),
      blunder: per(lossy * 0.42),
      miss: per(lossy * 0.2),
    };
    const counts = emptyCounts();
    for (const c of classes) counts[c] = Math.round(rates[c] * reviewed);

    const avgOpp = games ? Math.round(rating - 18 + (rnd() - 0.5) * 40) : null;
    const decided = results.overall.wins + results.overall.draws + results.overall.losses;
    const performance =
      avgOpp !== null && decided
        ? Math.round(avgOpp + (400 * (results.overall.wins - results.overall.losses)) / decided)
        : null;

    const families = OPENING_FAMILIES.slice(0, 7).map((name, i) => {
      const share = [0.26, 0.2, 0.15, 0.13, 0.1, 0.09, 0.07][i]!;
      const g = Math.round(games * share);
      return { name, results: wdl(g, r.win + (rnd() - 0.5) * 0.3, r.draw) };
    });

    const span = Math.min(games, 60);
    const trend: Profile["trend"] = [];
    let walk = rating - 26;
    for (let i = 0; i < span; i++) {
      walk += (rnd() - 0.46) * 9;
      const scored = rnd() < r.win + r.draw / 2 ? (rnd() < r.draw / (r.win + r.draw) ? 0.5 : 1) : 0;
      trend.push({
        game: 1000 + i,
        played_at: minutesAgo((span - i) * 60 * 30 + Math.round(rnd() * 600)),
        accuracy:
          i >= span - Math.round((reviewed / (games || 1)) * span)
            ? round(acc + (rnd() - 0.5) * 16 + (i - span / 2) * 0.05)
            : null,
        rating: Math.round(walk),
        points: scored,
      });
    }

    const moment = (
      game: GameId,
      ply: number,
      san: string,
      classification: Classification,
      before: number,
      after: number,
      opponent: string,
      sacrifice = 0,
    ): Moment => ({
      game,
      ply,
      san,
      side: "white",
      classification,
      win_before: before,
      win_after: after,
      sacrifice,
      opponent,
      played_at: minutesAgo(60 * (game + 3)),
      url: `https://lichess.org/g${game}#${ply}`,
    });
    const has = reviewed > 0;
    const moments: Profile["moments"] = {
      best: has
        ? [
            moment(3, 24, "Rxf7", "brilliant", 52.4, 71.8, "Stockfish 1404", 2),
            moment(4, 31, "Qd6+", "great", 41.2, 59.3, "cutemouse83"),
          ]
        : [],
      blunders: has
        ? [
            moment(5, 39, "Qh5", "blunder", 64.0, 18.5, "Stockfish level 5"),
            moment(4, 22, "Bxh7", "blunder", 55.1, 21.0, "cutemouse83"),
            moment(7, 47, "Rd1", "blunder", 47.8, 19.4, "knightowl"),
          ]
        : [],
      most_accurate_game: has
        ? { game: 3, accuracy: 94.1, opponent: "Stockfish 1404", outcome: "white_wins", played_at: minutesAgo(60 * 20) }
        : null,
      least_accurate_game: has
        ? { game: 5, accuracy: 58.3, opponent: "Stockfish level 5", outcome: "draw", played_at: minutesAgo(60 * 48) }
        : null,
    };

    const phase = (
      phase: Phase,
      fen: string,
      played: string,
      best: string,
      swing: number,
      ply: number,
      recurring: number,
      opening: string,
      game: GameId,
      kind: ThemeKind,
      detail: string | null = null,
    ): WeakSpot => ({
      game,
      ply,
      fen,
      played,
      best,
      swing,
      opening,
      phase,
      recurring,
      kind,
      detail,
    });
    const L = ctx.lines;
    const weak_spots: WeakSpot[] =
      key === "you" && has
        ? [
            phase("opening", ctx.fenAfter(L.italian, 9), "h6", "Bb3", 14.2, 10, 4, "Italian Game", 3, "opening_error"),
            phase("opening", ctx.fenAfter(L.italian, 7), "d6", "O-O", 11.6, 8, 4, "Italian Game", 4, "opening_error"),
            phase(
              "opening",
              ctx.fenAfter(L.sicilian, 7),
              "a6",
              "Be2",
              12.9,
              8,
              3,
              "Sicilian Defense",
              7,
              "opening_error",
            ),
            phase(
              "middlegame",
              ctx.fenAfter(L.caro, 17),
              "Qh5",
              "Nbd2",
              45.5,
              18,
              2,
              "Caro-Kann Defense",
              5,
              "hanging_piece",
              "a queen",
            ),
            phase(
              "middlegame",
              ctx.fenAfter(L.qgd, 21),
              "Rd1",
              "Bxf6",
              28.4,
              22,
              2,
              "Queen's Gambit Declined",
              5,
              "missed_tactic",
              "winning capture",
            ),
            phase(
              "endgame",
              ctx.fenAfter(L.london, 31),
              "Kf2",
              "Kd2",
              17.3,
              32,
              1,
              "London System",
              7,
              "endgame_technique",
            ),
            phase(
              "middlegame",
              ctx.fenAfter(L.qgd, 25),
              "Bd3",
              "Qc2",
              22.6,
              26,
              2,
              "Queen's Gambit Declined",
              6,
              "time_trouble",
              "9 s left",
            ),
            phase(
              "middlegame",
              ctx.fenAfter(L.sicilian, 21),
              "g4",
              "O-O-O",
              19.8,
              22,
              3,
              "Sicilian Defense",
              7,
              "king_safety",
              "mate in 3",
            ),
            phase("middlegame", ctx.fenAfter(L.london, 21), "h3", "Ne5", 12.2, 22, 2, "London System", 4, "other"),
          ]
        : [];
    const themes: ThemeStat[] = key === "you" && has ? themeStats(weak_spots, reviewed) : [];

    return {
      player: p,
      filter,
      games,
      reviewed,
      pending_review: games - reviewed,
      unreviewed_total: r.games - r.reviewed,
      ...(key === "you" ? { history_complete: r.games > 0 } : {}),
      first_game: games ? minutesAgo(span * 60 * 30 + 600) : null,
      last_game: games ? minutesAgo(30) : null,
      rating: games ? rating : null,
      results,
      accuracy: {
        overall: accOf(0),
        opening: accOf(4.6),
        middlegame: accOf(-3.1),
        endgame: accOf(-1.8),
        by_speed,
      },
      acpl: reviewed ? round((100 - acc) * 1.7) : null,
      counts,
      counts_per_game: rates,
      judgements: {
        inaccuracy: counts.inaccuracy,
        mistake: counts.mistake + Math.round(counts.miss / 2),
        blunder: counts.blunder + Math.round(counts.miss / 2),
      },
      opponents: { average_rating: avgOpp, performance },
      openings: families.filter((f) => f.results.games > 0),
      time: {
        games: reviewed,
        average_ms: reviewed ? Math.round(4200 + (rnd() - 0.5) * 1500) : null,
        opening_ms: reviewed ? 1900 : null,
        middlegame_ms: reviewed ? 5600 : null,
        endgame_ms: reviewed ? 3400 : null,
        time_trouble_blunders: reviewed ? Math.round(reviewed * 0.18) : 0,
        time_trouble_blunders_per_game: reviewed ? 0.18 : 0,
      },
      trend,
      longest_win_streak: games ? 3 + Math.round(rnd() * 5) : 0,
      moments,
      weak_spots,
      themes,
    };
  }

  // ---- Background work for players

  function startWork(
    title: string,
    kind: "import" | "review",
    key: string,
    total: number,
    then: () => void,
    priority: Priority = kind === "review" ? "review" : "background",
  ) {
    const id = Math.max(...ctx.work.map((w) => w.id)) + 1;
    const w: Work = {
      id,
      kind,
      status: "running",
      priority,
      title,
      subject: { type: "player", key: key as PlayerKey },
      cores: 0,
      progress: { done: 0, total, unit: "games", eta_seconds: 6, text: null },
      paused_for: null,
      error: null,
      created_at: new Date().toISOString(),
      started_at: new Date().toISOString(),
      ended_at: null,
    };
    ctx.work.push(w);
    emit("work.upserted", "work", { ...w });
    let step = 0;
    const timer = setInterval(() => {
      if (w.status === "stopped") {
        clearInterval(timer);
        return;
      }
      step++;
      w.progress = {
        done: Math.round((total * step) / 4),
        total,
        unit: "games",
        eta_seconds: (4 - step) * 1.5,
        text: null,
      };
      if (step >= 4) {
        clearInterval(timer);
        then();
        w.status = "done";
        w.ended_at = new Date().toISOString();
      }
      emit("work.upserted", "work", { ...w });
    }, 1500);
    return w;
  }

  // ---- Connections

  const youAccount: ConnectionStatus["account"] = ctx.connections.you.account;
  if (ctx.mode === "unlinked") {
    ctx.connections.you = {
      key: "you",
      configured: false,
      account: null,
      state: "off",
      error: null,
      playing: [],
      challenges: [],
      you: null,
    };
  }
  const connectionsEmit = (c: ConnectionStatus) => {
    emit("connection.upserted", `connection:${c.key}`, structuredClone(c));
    return reply("/api/connections/you/seek", "post", structuredClone(c));
  };

  const status = ctx.status;
  status.machine = { ...status.machine, data_dir: "C:\\Users\\dame\\AppData\\Local\\chess" };

  return (method, path, url, body) => {
    let m: RegExpMatchArray | null;
    if (method === "GET") {
      const edge = settingsRoute(ctx.mode, path);
      if (edge) return edge;
      if (path === "/api/token") return reply("/api/token", "get", { token: "dmls_4f8c0a2b9d7e41c6a3b5e8f20d19c7a4" });
      if (path === "/api/players/top") {
        if (ctx.mode === "topbroken") return error(502, "upstream", "Lichess did not answer.");
        const speed = url.searchParams.get("speed") ?? "blitz";
        const rows = (LEADERBOARD[speed] ?? LEADERBOARD["blitz"]!).map(
          ([username, title, rating], i): LeaderboardEntry => ({
            key: `lichess:${username.toLowerCase()}`,
            username,
            title,
            rating,
            progress: Math.round(((i * 7) % 13) - 4),
          }),
        );
        return reply("/api/players/top", "get", rows);
      }
      if ((m = path.match(/^\/api\/players\/([^/]+)\/profile$/))) {
        const key = decodeURIComponent(m[1]!);
        const filter = filterOf(url);
        const quirk = profileQuirk(key);
        if (quirk === "broken") return error(500, "internal", "The profile could not be added up. Try again.");
        const p = build(key, filter);
        const answer = p
          ? reply("/api/players/{key}/profile", "get", p)
          : error(404, "not_found", "Lichess has no such player.");
        // A slow answer, to see the loading state: `lichess:slow…`, or You with `?fixtures=slow`.
        return quirk === "slow" ? new Promise<Response>((done) => setTimeout(() => done(answer), 6000)) : answer;
      }
      if ((m = path.match(/^\/api\/players\/([^/]+)$/))) {
        const p = player(decodeURIComponent(m[1]!));
        return p ? reply("/api/players/{key}", "get", p) : error(404, "not_found", "Lichess has no such player.");
      }
      if (path === "/api/compare") {
        const keys = (url.searchParams.get("players") ?? "you,engine:stockfish").split(",").filter(Boolean);
        const filter = filterOf(url);
        if (keys.some((k) => profileQuirk(k) === "broken"))
          return error(500, "internal", "The comparison could not be added up. Try again.");
        const profiles = keys.map((k) => build(k, filter)).filter((p): p is Profile => !!p);
        if (profiles.length !== keys.length) return error(404, "not_found", "Lichess has no such player.");
        return reply("/api/compare", "get", { profiles, leaders: leaders(profiles) });
      }
      if (path === "/api/connections") {
        // `?fixtures=slow` reads the connections late, to see what a sheet says meanwhile.
        if (ctx.mode !== "slow") return undefined;
        const answer = reply("/api/connections", "get", structuredClone(ctx.connections));
        return new Promise<Response>((done) => setTimeout(() => done(answer), 3000));
      }
      if (path === "/api/diagnostics")
        return reply("/api/diagnostics", "get", {
          ok: true,
          items: [
            { key: "fastchess", level: "ok", ok: true, title: "fastchess", detail: "fastchess 1.4.0 is installed." },
            {
              key: "book",
              level: "ok",
              ok: true,
              title: "Opening book",
              detail: "UHO_Lichess_4852_v1.epd is installed.",
            },
            { key: "database", level: "ok", ok: true, title: "Database", detail: "chess.db, 41 MB." },
            { key: "disk", level: "ok", ok: true, title: "Disk", detail: "212 GB free on the data drive." },
          ],
        });
    }

    if (method === "POST") {
      if (path === "/api/reviews" && typeof body["player"] === "string") {
        if (status.policy.assist !== "allowed") return error(409, "fair_play", status.policy.reason ?? "");
        const key = body["player"];
        const r = entry(key);
        if (!r) return error(404, "not_found", "No such player.");
        const background = body["background"] === true;
        const all = background && body["limit"] == null;
        const limit = all ? Infinity : Number(body["limit"] ?? 20);
        const todo = Math.min(limit, r.games - r.reviewed);
        const w = startWork(
          all ? `Reviewing all ${todo} of ${key === "you" ? "your" : `${r.name}'s`} games` : `Reviewing ${todo} games`,
          "review",
          key,
          todo,
          () => {
            r.reviewed += todo;
          },
          background ? "background" : "review",
        );
        return reply("/api/reviews", "post", structuredClone(w), 202);
      }
      if ((m = path.match(/^\/api\/players\/([^/]+)\/import$/))) {
        if (status.policy.assist === "off") return error(409, "fair_play", status.policy.reason ?? "");
        const key = decodeURIComponent(m[1]!);
        const r = entry(key);
        if (!r) return error(404, "not_found", "Lichess has no such player.");
        // Your history comes in whole when no `max` is given: about 3,700 games here.
        const max =
          body["max"] != null ? Math.min(Number(body["max"]), 1000) : key === "you" ? Math.max(r.games, 3708) : 50;
        const title = key === "you" ? "Importing your Lichess games" : `Importing ${r.name}'s games`;
        const w = startWork(title, "import", key, max, () => {
          r.games = Math.max(r.games, max);
        });
        return reply("/api/players/{key}/import", "post", structuredClone(w), 202);
      }
      if (path === "/api/connections/you/login") {
        const you = ctx.connections.you;
        setTimeout(() => {
          Object.assign(you, {
            configured: true,
            account: youAccount,
            state: "online",
            you: {
              scopes: ["board:play", "challenge:read", "challenge:write"],
              missing_scopes: [],
              expires_at: minutesAgo(-60 * 24 * 365),
              elsewhere: [],
              seek: null,
              seek_note: null,
            },
          });
          emit("connection.upserted", "connection:you", structuredClone(you));
        }, 600);
        return reply("/api/connections/you/login", "post", {
          url: "/settings?lichess=connected",
          expires_in_seconds: 600,
        });
      }
      if ((m = path.match(/^\/api\/connections\/you\/challenges\/([^/]+)\/(accept|decline)$/))) {
        const c = ctx.connections.you;
        const found = c.challenges.find((x) => x.id === m![1]);
        if (!found) return error(404, "not_found", "That challenge is gone.");
        c.challenges = c.challenges.filter((x) => x !== found);
        emit("challenge", "connection:you", {
          challenge: structuredClone(found),
          event: m[2] === "accept" ? "accepted" : "declined",
          reason: m[2] === "accept" ? null : "generic",
          detail: null,
        });
        return reply("/api/connections/{key}/challenges/{id}/accept", "post", structuredClone(c));
      }
      if (path === "/api/connections/you/seek") {
        const you = ctx.connections.you;
        if (!you.configured) return error(409, "conflict", "Log in with Lichess first.");
        if (you.you) you.you.seek_note = "Looking for an opponent on Lichess.";
        return connectionsEmit(you);
      }
    }

    if (method === "DELETE") {
      if (path === "/api/connections/you") {
        const c = ctx.connections.you;
        Object.assign(c, { configured: false, account: null, state: "off", error: null, playing: [], challenges: [] });
        return connectionsEmit(c);
      }
      if ((m = path.match(/^\/api\/connections\/you\/challenges\/([^/]+)$/))) {
        const c = ctx.connections.you;
        const found = c.challenges.find((x) => x.id === m![1]);
        if (!found) return error(404, "not_found", "That challenge is gone.");
        c.challenges = c.challenges.filter((x) => x !== found);
        emit("challenge", "connection:you", {
          challenge: structuredClone(found),
          event: "canceled",
          reason: null,
          detail: null,
        });
        return reply("/api/connections/{key}/challenges/{id}", "delete", structuredClone(c));
      }
    }
    return undefined;
  };
}

/** How a hundred reviewed games of yours break down by theme: the figures `themeStats` scales to the games reviewed. */
const THEME_BASE: {
  kind: ThemeKind;
  count: number;
  games: number;
  swing: number;
  causes?: [string, number, number][];
  openings?: [string, number, number][];
}[] = [
  {
    kind: "hanging_piece",
    count: 92,
    games: 61,
    swing: 2390,
    causes: [
      ["a knight", 34, 880],
      ["a bishop", 27, 690],
      ["a rook", 19, 520],
    ],
  },
  {
    kind: "missed_tactic",
    count: 76,
    games: 54,
    swing: 1710,
    causes: [
      ["winning capture", 29, 640],
      ["fork", 22, 520],
      ["mate", 9, 230],
    ],
  },
  { kind: "time_trouble", count: 66, games: 40, swing: 1580 },
  {
    kind: "opening_error",
    count: 58,
    games: 44,
    swing: 880,
    openings: [
      ["Italian Game", 21, 330],
      ["Sicilian Defense", 17, 270],
      ["Caro-Kann Defense", 9, 120],
    ],
  },
  { kind: "endgame_technique", count: 41, games: 33, swing: 760 },
  {
    kind: "king_safety",
    count: 22,
    games: 19,
    swing: 610,
    causes: [
      ["mate", 12, 390],
      ["king moved, castling lost", 10, 220],
    ],
  },
  { kind: "other", count: 105, games: 70, swing: 1450 },
];

/** Your mistakes grouped by theme as the server groups them, scaled to `reviewed` games, with the weak spots as examples. */
function themeStats(spots: WeakSpot[], reviewed: number): ThemeStat[] {
  const k = reviewed / 100;
  const scaled = (n: number) => Math.max(1, Math.round(n * k));
  const total = THEME_BASE.reduce((s, t) => s + t.swing, 0);
  const named = (rows?: [string, number, number][]) =>
    (rows ?? []).map(([name, count, swing]) => ({
      name,
      count: scaled(count),
      swing: Math.round(swing * k * 10) / 10,
    }));
  return THEME_BASE.map((t) => {
    const swing = Math.round(t.swing * k * 10) / 10;
    return {
      kind: t.kind,
      count: scaled(t.count),
      games: Math.min(reviewed, scaled(t.games)),
      swing,
      per_game: Math.round((swing / reviewed) * 10) / 10,
      share: Math.round((t.swing / total) * 1000) / 1000,
      causes: named(t.causes),
      openings: named(t.openings),
      examples: spots
        .filter((s) => s.kind === t.kind)
        .sort((a, b) => b.swing - a.swing)
        .slice(0, 3),
    };
  });
}

function filterOf(url: URL): ProfileFilter {
  const speeds = (url.searchParams.get("speeds") ?? "").split(",").filter(Boolean) as Speed[];
  const rated = url.searchParams.get("rated");
  return {
    speeds,
    rated: rated === "true" ? true : rated === "false" ? false : null,
    since: url.searchParams.get("since"),
    limit: Number(url.searchParams.get("limit") ?? 200),
  };
}
