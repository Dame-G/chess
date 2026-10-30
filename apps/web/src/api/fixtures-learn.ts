/**
 * The fixtures' answers for the player side: drills, levels, your repertoire and progress. `fixtures.ts` hands this module the data it shares and the way to speak on the event
 * stream; everything here is invented too, and each answer is typed through `reply()` so it cannot drift from the
 * server's shapes.
 */
import { parseSan } from "chessops/san";
import { playUci, setup, uciOf, view } from "../board/position";
import { error, reply } from "./fixtures-http";
import type {
  Departure,
  Drill,
  DrillHint,
  DrillOutcome,
  EventKind,
  EventPayloads,
  GameId,
  HitRatePoint,
  LevelRating,
  Prep,
  PrepLine,
  ProgressChart,
  Repertoire,
  RepertoireMove,
  RepertoireNode,
  Side,
  ThemeKind,
  WeakSpot,
  Status,
  Work,
} from "./types";

export interface LearnContext {
  minutesAgo: (m: number) => string;
  emit: <K extends EventKind>(kind: K, topic: string, data: EventPayloads[K]) => void;
  work: Work[];
  status: Status;
  /** Ids of games You played, to link the drills and the repertoire's examples to. */
  yourGames: () => GameId[];
}

type Handler = (method: string, path: string, url: URL, body: Record<string, unknown>) => Response | undefined;

const key = (fen: string) => fen.split(" ").slice(0, 4).join(" ");

/** Plays SAN moves from the start; answers each move's UCI and the FEN before the last one. */
function walk(text: string) {
  const pos = setup("startpos")!;
  const ucis: string[] = [];
  const fens: string[] = [view(pos).fen];
  for (const san of text.split(/\s+/).filter(Boolean)) {
    const move = parseSan(pos, san);
    if (!move) throw new Error(`fixture: ${san} is not legal in ${text}`);
    ucis.push(uciOf(pos, move));
    pos.play(move);
    fens.push(view(pos).fen);
  }
  return { ucis, fens };
}

// ---- Drills

interface DrillSpec {
  /** The game up to the position, in SAN. */
  before: string;
  /** The move that was missed, then the line that follows it. */
  best: string;
  line: string;
  played: string;
  swing: number;
  opening: string;
  phase: "opening" | "middlegame" | "endgame";
  kind: ThemeKind;
  detail?: string;
  due: number;
}

const DRILLS: DrillSpec[] = [
  {
    before: "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6 d3 d6 O-O a6",
    best: "Re1",
    line: "Re1 O-O Bb3 Ba7",
    played: "h3",
    swing: 14,
    opening: "Italian Game",
    phase: "opening",
    kind: "opening_error",
    due: -2,
  },
  {
    before: "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6 Be3 e5 Nb3 Be6 f3 Be7 Qd2 O-O",
    best: "O-O-O",
    line: "O-O-O Nbd7 g4 b5",
    played: "g4",
    swing: 19,
    opening: "Sicilian Defense",
    phase: "middlegame",
    kind: "king_safety",
    detail: "king moved, castling lost",
    due: -1,
  },
  {
    before: "d4 d5 c4 e6 Nc3 Nf6 Bg5 Be7 e3 O-O Nf3 h6 Bh4 b6",
    best: "cxd5",
    line: "cxd5 exd5 Bxf6 Bxf6",
    played: "Be2",
    swing: 11,
    opening: "Queen's Gambit Declined",
    phase: "opening",
    kind: "opening_error",
    due: 0,
  },
  {
    before: "d4 Nf6 Bf4 d5 e3 c5 c3 Nc6 Nd2 e6 Ngf3 Bd6 Bg3 O-O Bd3 b6",
    best: "Ne5",
    line: "Ne5 Bb7 f4 Qc7",
    played: "h3",
    swing: 22,
    opening: "London System",
    phase: "middlegame",
    kind: "missed_tactic",
    detail: "wins material",
    due: 0,
  },
  {
    before: "e4 c6 d4 d5 e5 Bf5 Nf3 e6 Be2 Nd7 O-O Ne7 Nbd2 h6",
    best: "Nb3",
    line: "Nb3 Bg6 Bd3 Bxd3",
    played: "c4",
    swing: 16,
    opening: "Caro-Kann Defense",
    phase: "opening",
    kind: "time_trouble",
    detail: "7 s left",
    due: 0,
  },
  {
    before: "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6 d3 d6 O-O a6 Bb3",
    best: "O-O",
    line: "O-O h3 h6 Nbd2",
    played: "Ba7",
    swing: 12,
    opening: "Italian Game",
    phase: "opening",
    kind: "hanging_piece",
    detail: "a bishop",
    due: 3,
  },
  {
    before: "e4 e5 Nf3 d6 d4 Bg4 dxe5 Bxf3 Qxf3 dxe5 Bc4 Nf6 Qb3 Qe7 Nc3 c6 Bg5 b5",
    best: "Nxb5",
    line: "Nxb5 cxb5 Bxb5 Nbd7",
    played: "Bxb5",
    swing: 31,
    opening: "Philidor Defense",
    phase: "middlegame",
    kind: "missed_tactic",
    detail: "winning capture",
    due: 5,
  },
];

function makeDrills(ctx: LearnContext): Drill[] {
  const games = ctx.yourGames();
  return DRILLS.map((s, i): Drill => {
    const { ucis, fens } = walk(s.before);
    const side: Side = ucis.length % 2 === 0 ? "white" : "black";
    const after = walk(`${s.before} ${s.line}`).ucis.slice(ucis.length);
    const playedUci = walk(`${s.before} ${s.played}`).ucis.at(-1)!;
    const answered = i < 2 || s.due > 0;
    return {
      id: i + 1,
      game: games[i % Math.max(1, games.length)] ?? 1,
      ply: ucis.length + 1,
      fen: fens.at(-1)!,
      key: key(fens.at(-1)!),
      side,
      best_moves: [after[0]!],
      best_san: [s.best],
      line: after,
      played: playedUci,
      played_san: s.played,
      swing: s.swing,
      theme: { opening: s.opening, phase: s.phase, kind: s.kind, detail: s.detail ?? null },
      due: ctx.minutesAgo(-s.due * 60 * 24 + (s.due <= 0 ? 60 : 0)),
      interval_days: answered ? 6 : 0,
      ease: 2.5,
      reps: answered ? 2 : 0,
      lapses: 0,
      created_at: ctx.minutesAgo(60 * 24 * 6),
      answered_at: answered ? ctx.minutesAgo(60 * 24) : null,
    };
  });
}

/** A drill as the server sends it before it is answered: the position and its theme, without the answer. */
function question(d: Drill): Drill {
  return { ...d, best_moves: [], best_san: [], line: [], played: "", played_san: "" };
}

/** `core::drills::hint`: what kind of move, then which piece (and where it stands), never the move. */
function hintOf(d: Drill, level: number): DrillHint {
  const san = d.best_san[0] ?? "";
  if (level < 2) {
    const text = san.includes("+") || san.includes("#") ? "A check" : san.includes("x") ? "A capture" : "A quiet move";
    return { level, text, from: null };
  }
  const piece = san.startsWith("O")
    ? "king"
    : (({ N: "knight", B: "bishop", R: "rook", Q: "queen", K: "king" } as Record<string, string>)[san[0] ?? ""] ??
      "pawn");
  return { level, text: `Move the ${piece}`, from: (d.best_moves[0] ?? "").slice(0, 2) || null };
}

/** SM-2 with two answers, as `core::drills::grade` has it. */
function grade(d: Drill, outcome: DrillOutcome, now: string, hinted = false): Drill {
  const pass = outcome === "pass";
  const half = pass && hinted;
  const interval = !pass || half ? 1 : d.reps === 0 ? 1 : d.reps === 1 ? 6 : Math.round(d.interval_days * d.ease);
  const ease = Math.max(1.3, d.ease + (half ? -0.14 : pass ? 0.1 : -0.32));
  return {
    ...d,
    reps: half ? d.reps : pass ? d.reps + 1 : 0,
    lapses: pass ? d.lapses : d.lapses + 1,
    interval_days: interval,
    ease,
    due: new Date(Date.parse(now) + interval * 86_400_000).toISOString(),
    answered_at: now,
  };
}

function trend(): HitRatePoint[] {
  const out: HitRatePoint[] = [];
  let passes = 0;
  let answers = 0;
  for (let day = 13; day >= 0; day--) {
    const n = 4 + ((day * 5) % 5);
    const p = Math.round(n * (0.4 + (13 - day) * 0.018 + (day % 3 === 0 ? -0.05 : 0.04)));
    const hinted = day % 2 === 0 && p > 1 ? 1 : 0;
    passes += p - 0.5 * hinted;
    answers += n;
    const date = new Date(Date.now() - day * 86_400_000).toISOString().slice(0, 10);
    out.push({ date, answers: n, passes: p - hinted, hinted, rate: (p - 0.5 * hinted) / n, rolling: passes / answers });
  }
  return out;
}

// ---- Levels

const LADDER = [
  [1, 800, 300, 8, 15],
  [2, 1000, 300, 6, 10],
  [3, 1200, 500, 5, 6],
  [4, 1400, 800, 4, 3.5],
  [5, 1600, 1500, 3, 2],
  [6, 1800, 3000, 2, 1],
  [7, 2000, 6000, 1, 0],
  [8, 2200, 15000, 1, 0],
  [9, 2400, 40000, 1, 0],
  [10, 2600, 120000, 1, 0],
  [11, 2800, 400000, 1, 0],
  [12, 3000, null, 1, 0],
] as const;

/** The levels as the app has found them: the lower ones have played, the top ones have not. */
function levels(): LevelRating[] {
  const fitted: Record<number, [number, number, number]> = {
    1: [742, 61, 36],
    2: [968, 52, 44],
    3: [1181, 47, 58],
    4: [1404, 44, 61],
    5: [1633, 45, 57],
    6: [1822, 58, 40],
    7: [2037, 96, 18],
    // Calibration is still running for the top of the ladder: a wide margin, then few games, then nothing yet.
    8: [2190, 380, 20],
    9: [2440, 488, 12],
    10: [2559, 488, 40],
  };
  return LADDER.map(([id, nominal, nodes, multipv, temperature]) => {
    const f = fitted[id];
    return {
      level: { id, name: nodes == null ? "Full strength" : `Level ${id}`, nodes, multipv, temperature, nominal },
      games: f?.[2] ?? 0,
      rating: f ? { elo: f[0], low: f[0] - f[1], high: f[0] + f[1] } : null,
    };
  });
}

// ---- Your repertoire

interface Line {
  sans: string;
  games: number;
  wins: number;
  draws: number;
  /** The first move (0-based) that is out of theory. */
  out: number;
}

const WHITE_LINES: Line[] = [
  { sans: "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6 d3 d6 O-O a6", games: 11, wins: 6, draws: 1, out: 8 },
  { sans: "e4 e5 Nf3 Nc6 Bc4 Nf6 d3 Bc5 c3 d6", games: 5, wins: 3, draws: 0, out: 7 },
  { sans: "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6 Be3 e5", games: 8, wins: 3, draws: 1, out: 10 },
  { sans: "e4 c6 d4 d5 e5 Bf5 Nf3 e6 Be2", games: 5, wins: 3, draws: 1, out: 8 },
  { sans: "d4 d5 c4 e6 Nc3 Nf6 Bg5 Be7 e3 O-O Nf3", games: 6, wins: 3, draws: 2, out: 9 },
  { sans: "e4 e5 Nf3 d6 d4 Bg4 dxe5 Bxf3 Qxf3 dxe5 Bc4", games: 3, wins: 3, draws: 0, out: 6 },
];
const BLACK_LINES: Line[] = [
  { sans: "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6 Be3 e5", games: 11, wins: 5, draws: 1, out: 9 },
  { sans: "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6", games: 6, wins: 2, draws: 2, out: 6 },
  { sans: "d4 Nf6 Bf4 d5 e3 c5 c3 Nc6 Nd2 e6", games: 7, wins: 4, draws: 1, out: 9 },
  { sans: "e4 c6 d4 d5 e5 Bf5 Nf3 e6", games: 3, wins: 1, draws: 0, out: 5 },
];

// ---- Your prep

interface PrepSpec {
  name: string;
  eco: string;
  sans: string;
  games: number;
  wins: number;
  draws: number;
  reviewed: number;
  /** Reviewed games with a mistake in the first twelve moves, the mistakes in all, and what they cost. */
  early: number;
  mistakes: number;
  swing: number;
}

const WHITE_PREP: PrepSpec[] = [
  {
    name: "Italian Game: Giuoco Piano",
    eco: "C54",
    sans: "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6",
    games: 14,
    wins: 8,
    draws: 1,
    reviewed: 12,
    early: 2,
    mistakes: 2,
    swing: 31.4,
  },
  {
    name: "Sicilian Defense: Najdorf Variation",
    eco: "B90",
    sans: "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6",
    games: 9,
    wins: 2,
    draws: 1,
    reviewed: 8,
    early: 4,
    mistakes: 6,
    swing: 112.6,
  },
  {
    name: "Queen's Gambit Declined: Orthodox Defense",
    eco: "D63",
    sans: "d4 d5 c4 e6 Nc3 Nf6 Bg5 Be7 e3 O-O",
    games: 7,
    wins: 3,
    draws: 2,
    reviewed: 7,
    early: 0,
    mistakes: 0,
    swing: 0,
  },
  {
    name: "Caro-Kann Defense: Advance Variation",
    eco: "B12",
    sans: "e4 c6 d4 d5 e5",
    games: 6,
    wins: 4,
    draws: 1,
    reviewed: 5,
    early: 2,
    mistakes: 3,
    swing: 47.5,
  },
  {
    name: "Philidor Defense",
    eco: "C41",
    sans: "e4 e5 Nf3 d6",
    games: 4,
    wins: 3,
    draws: 0,
    reviewed: 4,
    early: 0,
    mistakes: 0,
    swing: 0,
  },
];
const BLACK_PREP: PrepSpec[] = [
  {
    name: "Sicilian Defense: Najdorf Variation",
    eco: "B90",
    sans: "e4 c5 Nf3 d6 d4 cxd4 Nxd4 Nf6 Nc3 a6",
    games: 12,
    wins: 5,
    draws: 1,
    reviewed: 10,
    early: 3,
    mistakes: 4,
    swing: 66.3,
  },
  {
    name: "London System",
    eco: "D02",
    sans: "d4 Nf6 Bf4 d5 e3 c5 c3 Nc6",
    games: 8,
    wins: 5,
    draws: 1,
    reviewed: 8,
    early: 1,
    mistakes: 1,
    swing: 14.8,
  },
  {
    name: "Italian Game: Giuoco Piano",
    eco: "C54",
    sans: "e4 e5 Nf3 Nc6 Bc4 Bc5 c3 Nf6",
    games: 6,
    wins: 2,
    draws: 2,
    reviewed: 5,
    early: 0,
    mistakes: 0,
    swing: 0,
  },
];

function prepLines(side: Side, specs: PrepSpec[], games: GameId[]): Prep {
  const lines: PrepLine[] = specs.map((s, i) => {
    const sans = s.sans.split(" ");
    const { ucis, fens } = walk(s.sans);
    const losses = s.games - s.wins - s.draws;
    const score = (s.wins + s.draws / 2) / s.games;
    const pgn = sans.map((m, n) => (n % 2 === 0 ? `${n / 2 + 1}. ${m}` : m)).join(" ");
    const worst: WeakSpot | null = s.mistakes
      ? {
          game: games[i % Math.max(1, games.length)] ?? 1,
          ply: Math.min(ucis.length + 1, 24),
          fen: fens.at(-1)!,
          played: "h3",
          best: "Re1",
          swing: Math.round((s.swing / s.mistakes) * 1.6 * 10) / 10,
          opening: s.name.split(":")[0]!,
          phase: "opening",
          recurring: s.mistakes,
          kind: "opening_error",
          detail: null,
        }
      : null;
    return {
      name: s.name,
      eco: s.eco,
      moves: ucis,
      pgn,
      fen: fens.at(-1)!,
      games: s.games,
      wins: s.wins,
      draws: s.draws,
      losses,
      score,
      reviewed: s.reviewed,
      early_games: s.early,
      early_mistakes: s.mistakes,
      early_swing: s.swing,
      worst,
      drill_games: games.slice(0, Math.max(1, s.early)).filter(() => s.early > 0),
      scores_badly: s.games >= 4 && score < 0.45,
      goes_wrong_early: s.reviewed >= 3 && s.early / s.reviewed >= 0.3,
      example: games[i % Math.max(1, games.length)] ?? 1,
    };
  });
  return { side, games: specs.reduce((n, s) => n + s.games, 0), lines };
}

interface Acc {
  fen: string;
  ply: number;
  games: number;
  moves: Map<string, { san: string; next: string; games: number; w: number; d: number; l: number; out: boolean }>;
}

function repertoire(side: Side, lines: Line[], examples: GameId[]): Repertoire {
  const nodes = new Map<string, Acc>();
  const departures: Departure[] = [];
  const startFen = walk("").fens[0]!;
  const get = (fen: string, ply: number) => {
    const k = key(fen);
    let n = nodes.get(k);
    if (!n) nodes.set(k, (n = { fen, ply, games: 0, moves: new Map() }));
    n.ply = Math.min(n.ply, ply);
    return n;
  };
  lines.forEach((line, li) => {
    const sans = line.sans.split(" ");
    const { ucis, fens } = walk(line.sans);
    const losses = line.games - line.wins - line.draws;
    get(startFen, 0).games += line.games;
    ucis.forEach((uci, i) => {
      const from = get(fens[i]!, i);
      const to = get(fens[i + 1]!, i + 1);
      to.games += line.games;
      const m = from.moves.get(uci) ?? {
        san: sans[i]!,
        next: key(fens[i + 1]!),
        games: 0,
        w: 0,
        d: 0,
        l: 0,
        out: false,
      };
      m.games += line.games;
      m.w += line.wins;
      m.d += line.draws;
      m.l += losses;
      if (i === line.out) m.out = true;
      from.moves.set(uci, m);
    });
    if (line.out < ucis.length) {
      const byYou = (line.out % 2 === 0) === (side === "white");
      departures.push({
        key: key(fens[line.out]!),
        fen: fens[line.out]!,
        ply: line.out + 1,
        uci: ucis[line.out]!,
        san: sans[line.out]!,
        by_you: byYou,
        games: line.games,
        score: (line.wins + line.draws / 2) / line.games,
        example: examples[li % Math.max(1, examples.length)] ?? 1,
      });
    }
  });
  const out: RepertoireNode[] = [...nodes.entries()]
    .sort((a, b) => a[1].ply - b[1].ply || b[1].games - a[1].games)
    .map(([k, n]) => {
      const moves: RepertoireMove[] = [...n.moves.entries()]
        .map(([uci, m]) => ({
          uci,
          san: m.san,
          games: m.games,
          share: m.games / n.games,
          wins: m.w,
          draws: m.d,
          losses: m.l,
          score: (m.w + m.d / 2) / m.games,
          next: m.next,
          leaves_theory: m.out,
        }))
        .sort((a, b) => b.games - a.games);
      return {
        key: k,
        fen: n.fen,
        ply: n.ply,
        games: n.games,
        your_turn: (n.ply % 2 === 0) === (side === "white"),
        theory: n.ply <= 6,
        moves,
      };
    });
  return {
    side,
    games: lines.reduce((s, l) => s + l.games, 0),
    options: { max_plies: 20, min_games: 2 },
    nodes: out,
    departures: departures.sort((a, b) => b.games - a.games),
  };
}

// ---- Progress

function progress(ctx: LearnContext): ProgressChart {
  const day = 86_400_000;
  const at = (daysAgo: number) => new Date(Date.now() - daysAgo * day).toISOString();
  const games = ctx.yourGames();
  const rating = Array.from({ length: 24 }, (_, i) => ({
    at: at(70 - i * 3),
    value: 1700 + i * 5.5 + ((i * 7) % 5) * 4 - 6,
    game: games[i % Math.max(1, games.length)] ?? null,
    low: null,
    high: null,
    label: null,
  }));
  const accuracy = Array.from({ length: 24 }, (_, i) => ({
    at: at(70 - i * 3),
    value: 73 + i * 0.28 + ((i * 3) % 4),
    game: null,
    low: null,
    high: null,
    label: null,
  }));
  const hit = Array.from({ length: 12 }, (_, i) => ({
    at: at(14 - i),
    value: 0.42 + i * 0.016,
    game: null,
    low: null,
    high: null,
    label: null,
  }));
  return {
    series: [
      { player: "you", metric: "rating", points: rating },
      { player: "you", metric: "accuracy", points: accuracy },
      { player: "you", metric: "drill_hit_rate", points: hit },
    ],
  };
}

export function createLearnRoutes(ctx: LearnContext): Handler {
  const { emit } = ctx;
  let drills: Drill[] | null = null;
  const theDrills = () => (drills ??= makeDrills(ctx));
  const answered: { at: string; outcome: DrillOutcome }[] = [];
  /** The deepest hint asked for each drill not answered yet. */
  const hints = new Map<number, number>();

  const explorerRefused = () => ctx.status.policy.assist === "off";

  function startWork(title: string, kind: Work["kind"], total: number, unit: string, seconds: number): Work {
    const id = Math.max(...ctx.work.map((w) => w.id)) + 1;
    const w: Work = {
      id,
      kind,
      status: "running",
      priority: "background",
      title,
      subject: { type: "player", key: "you" },
      cores: 0,
      progress: { done: 0, total, unit, eta_seconds: seconds, text: null },
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
      step++;
      w.progress = { done: Math.round((total * step) / 4), total, unit, eta_seconds: (4 - step) * 2, text: null };
      if (step >= 4) {
        clearInterval(timer);
        w.status = "done";
        w.ended_at = new Date().toISOString();
      }
      emit("work.upserted", "work", { ...w });
    }, 2000);
    return w;
  }

  return (method, path, url, body) => {
    let m: RegExpMatchArray | null;
    if (method === "GET") {
      if (path === "/api/drills/today") {
        const now = Date.now();
        const theme = url.searchParams.get("theme");
        const games = (url.searchParams.get("games") ?? "").split(",").filter(Boolean).map(Number);
        const maxPly = Number(url.searchParams.get("max_ply") ?? Infinity);
        // A theme or a line is a session on it: whatever is due first, then new ones, then those not due yet.
        const session = theme !== null || games.length > 0;
        const list = theDrills()
          .filter((d) => (session ? true : Date.parse(d.due) <= now + 3_600_000))
          .filter((d) => (theme ? d.theme.kind === theme : true))
          .filter((d) => (games.length ? games.includes(d.game) && d.ply <= maxPly : true))
          .sort((a, b) => Date.parse(a.due) - Date.parse(b.due))
          .slice(0, session ? 10 : 30);
        return reply("/api/drills/today", "get", {
          drills: list.map(question),
          due: list.filter((d) => d.reps > 0 || d.answered_at).length,
          new: list.filter((d) => !d.answered_at).length,
          started_today: 0,
          total: theDrills().length,
        });
      }
      if (path === "/api/drills") return reply("/api/drills", "get", theDrills().map(question));
      if (path === "/api/players/you/prep") {
        if (explorerRefused()) return error(409, "fair_play", ctx.status.policy.reason ?? "");
        const side = (url.searchParams.get("side") ?? "white") as Side;
        return reply(
          "/api/players/you/prep",
          "get",
          prepLines(side, side === "white" ? WHITE_PREP : BLACK_PREP, ctx.yourGames()),
        );
      }
      if (path === "/api/drills/trend") return reply("/api/drills/trend", "get", trend());
      if (path === "/api/levels")
        return reply("/api/levels", "get", {
          levels: levels(),
          suggested: 6,
          you: { entrant: "you", estimate: { elo: 1846, low: 1752, high: 1940 }, games: 14 },
          updated_at: ctx.minutesAgo(180),
        });
      if (path === "/api/levels/plan")
        return reply("/api/levels/plan", "get", [
          { a: 6, b: 7, games: 40, priority: 5.1 },
          { a: 7, b: 8, games: 40, priority: 4.2 },
          { a: 8, b: 9, games: 40, priority: 3.6 },
        ]);
      if (path === "/api/players/you/repertoire") {
        if (explorerRefused()) return error(409, "fair_play", ctx.status.policy.reason ?? "");
        const side = (url.searchParams.get("side") ?? "white") as Side;
        const examples = ctx.yourGames();
        return reply(
          "/api/players/you/repertoire",
          "get",
          side === "white" ? repertoire("white", WHITE_LINES, examples) : repertoire("black", BLACK_LINES, examples),
        );
      }
      if (path === "/api/progress") return reply("/api/progress", "get", progress(ctx));
    }

    if (method === "POST") {
      if (path === "/api/drills") {
        return reply("/api/drills", "post", []);
      }
      if ((m = path.match(/^\/api\/drills\/(\d+)\/hint$/))) {
        const d = theDrills().find((x) => x.id === Number(m![1]));
        if (!d) return error(404, "not_found", "No such drill.");
        const level = Math.min(2, (hints.get(d.id) ?? 0) + 1);
        hints.set(d.id, level);
        return reply("/api/drills/{id}/hint", "post", hintOf(d, level));
      }
      if ((m = path.match(/^\/api\/drills\/(\d+)\/answer$/))) {
        const d = theDrills().find((x) => x.id === Number(m![1]));
        if (!d) return error(404, "not_found", "No such drill.");
        const move = String(body["move"] ?? "");
        const pos = setup(d.fen)!;
        const parsed = parseSan(pos, move);
        const uci = parsed ? uciOf(pos, parsed) : move;
        const outcome: DrillOutcome = d.best_moves.includes(uci) || d.best_san.includes(move) ? "pass" : "fail";
        const hinted = hints.delete(d.id) && outcome === "pass";
        const now = new Date().toISOString();
        const next = grade(d, outcome, now, hinted);
        drills = theDrills().map((x) => (x.id === d.id ? next : x));
        answered.push({ at: now, outcome });
        const isUci = /^[a-h][1-8][a-h][1-8][qrbn]?$/.test(move);
        const playedSan = isUci ? (playUci(setup(d.fen)!, move)?.san ?? move) : move;
        emit("drill.upserted", "player:you", structuredClone(question(next)));
        emit("drill.answered", "player:you", { drill: d.id, outcome, at: now });
        return reply("/api/drills/{id}/answer", "post", {
          outcome,
          hinted,
          drill: structuredClone(next),
          best_moves: d.best_moves,
          best_san: d.best_san,
          line: d.line,
          played_san: playedSan,
        });
      }
      if (path === "/api/levels/calibrate") {
        if (ctx.work.some((w) => w.kind === "calibrate" && (w.status === "running" || w.status === "queued")))
          return error(409, "conflict", "Calibration is already queued.");
        const w = startWork("Calibrating levels 6–7, 7–8, 8–9 · 120 games", "calibrate", 120, "games", 8);
        return reply("/api/levels/calibrate", "post", structuredClone(w), 201);
      }
    }

    if (method === "DELETE") {
      if ((m = path.match(/^\/api\/drills\/(\d+)$/))) {
        drills = theDrills().filter((d) => d.id !== Number(m![1]));
        emit("drill.deleted", "player:you", { id: Number(m[1]) });
        return new Response(null, { status: 204 });
      }
    }
    return undefined;
  };
}
