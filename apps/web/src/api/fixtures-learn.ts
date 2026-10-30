/**
 * The fixtures' answers for the player side and the brain: drills, levels, your repertoire, progress, nets, datasets
 * and a network idea's training. `fixtures.ts` hands this module the data it shares and the way to speak on the event
 * stream; everything here is invented too, and each answer is typed through `reply()` so it cannot drift from the
 * server's shapes.
 */
import { parseSan } from "chessops/san";
import { playUci, setup, uciOf, view } from "../board/position";
import { error, reply } from "./fixtures-http";
import type {
  Dataset,
  Departure,
  Drill,
  DrillOutcome,
  EventKind,
  EventPayloads,
  GameId,
  HitRatePoint,
  Idea,
  LevelRating,
  Net,
  ProgressChart,
  Repertoire,
  RepertoireMove,
  RepertoireNode,
  Side,
  Status,
  TrainStep,
  Work,
} from "./types";

export interface LearnContext {
  minutesAgo: (m: number) => string;
  emit: <K extends EventKind>(kind: K, topic: string, data: EventPayloads[K]) => void;
  work: Work[];
  status: Status;
  ideas: Idea[];
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
      theme: { opening: s.opening, phase: s.phase },
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

/** SM-2 with two answers, as `core::drills::grade` has it. */
function grade(d: Drill, outcome: DrillOutcome, now: string): Drill {
  const pass = outcome === "pass";
  const interval = !pass ? 1 : d.reps === 0 ? 1 : d.reps === 1 ? 6 : Math.round(d.interval_days * d.ease);
  const ease = Math.max(1.3, d.ease + (pass ? 0.1 : -0.32));
  return {
    ...d,
    reps: pass ? d.reps + 1 : 0,
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
    passes += p;
    answers += n;
    const date = new Date(Date.now() - day * 86_400_000).toISOString().slice(0, 10);
    out.push({ date, answers: n, passes: p, rate: p / n, rolling: passes / answers });
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
    version: null,
  }));
  const accuracy = Array.from({ length: 24 }, (_, i) => ({
    at: at(70 - i * 3),
    value: 73 + i * 0.28 + ((i * 3) % 4),
    game: null,
    low: null,
    high: null,
    label: null,
    version: null,
  }));
  const hit = Array.from({ length: 12 }, (_, i) => ({
    at: at(14 - i),
    value: 0.42 + i * 0.016,
    game: null,
    low: null,
    high: null,
    label: null,
    version: null,
  }));
  const elo = Array.from({ length: 14 }, (_, i) => ({
    at: at(64 - i * 4.6),
    value: 2152 + i * 12 + (i % 3) * 3,
    game: null,
    low: 2152 + i * 12 - 20 - i,
    high: 2152 + i * 12 + 20 + i,
    label: `v${i + 1}`,
    version: i + 1,
  }));
  return {
    series: [
      { player: "you", metric: "rating", points: rating },
      { player: "you", metric: "accuracy", points: accuracy },
      { player: "you", metric: "drill_hit_rate", points: hit },
      { player: "dameless", metric: "elo", points: elo },
    ],
  };
}

// ---- Nets and datasets

const GB = 1_000_000_000;

function nets(ctx: LearnContext): Net[] {
  return [
    {
      id: 2,
      name: "smoke-1",
      sha256: "b6a1f0c27e9d4c5aa83d51ce40f9d17b6c2e0a8b7a4d3f5e21c9d08f6b7a3e12",
      path: "nets/b6a1f0c2.nnue",
      arch: "threats-768-16-32-1",
      size_bytes: 59_300_000,
      parent: 1,
      dataset: 1,
      run: { idea: null, work: 9, superbatches: 20, positions: 65_536_000, final_loss: 0.0213 },
      created_at: ctx.minutesAgo(60 * 20),
    },
    {
      id: 1,
      name: "v60-7f587dfb",
      sha256: "7f587dfb9c1d4e6a8b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f607182",
      path: "nets/7f587dfb.nnue",
      arch: "threats-768-16-32-1",
      size_bytes: 59_300_000,
      parent: null,
      dataset: null,
      run: null,
      created_at: ctx.minutesAgo(60 * 24 * 30),
    },
  ];
}

function datasets(ctx: LearnContext): Dataset[] {
  return [
    {
      id: 2,
      name: "trials-4",
      format: "viriformat",
      positions: 1_840_000,
      games: 22_800,
      size_bytes: 7_900_000,
      source: { type: "trials", idea: 2 },
      score_units: "reckless-internal",
      path: "data/trials-4",
      created_at: ctx.minutesAgo(60 * 22),
      updated_at: ctx.minutesAgo(60 * 22),
    },
    {
      id: 1,
      name: "dfrc-1",
      format: "viriformat",
      positions: 232_000_000,
      games: 2_900_000,
      size_bytes: 998 * 1_000_000,
      source: { type: "datagen", settings: { threads: 8, soft_nodes: 5000 } },
      score_units: "reckless-internal",
      path: "data/dfrc-1",
      created_at: ctx.minutesAgo(60 * 26),
      updated_at: ctx.minutesAgo(60 * 26),
    },
  ];
}

/** A network idea's training so far: the loss of 7 of 20 superbatches, in steps of ten batches. */
function trainSteps(upto: number): TrainStep[] {
  const steps: TrainStep[] = [];
  for (let sb = 1; sb <= upto; sb++) {
    for (let b = 20; b <= 200; b += 20) {
      const t = sb - 1 + b / 200;
      steps.push({
        seq: steps.length + 1,
        superbatch: sb,
        batch: b,
        loss: 0.0215 + 0.0205 * Math.exp(-t / 3.2) + 0.0006 * Math.sin(t * 9),
        lr: 1e-4 * (0.5 + 0.5 * Math.cos((Math.PI * t) / 20)),
        grad_norm: 0.8 + 0.2 * Math.sin(t),
        positions_per_sec: 742_000 + ((steps.length * 7919) % 30_000),
        elapsed_seconds: t * 4.3,
      });
    }
  }
  return steps;
}

export function createLearnRoutes(ctx: LearnContext): Handler {
  const { emit } = ctx;
  let drills: Drill[] | null = null;
  const theDrills = () => (drills ??= makeDrills(ctx));
  const answered: { at: string; outcome: DrillOutcome }[] = [];
  const netList = nets(ctx);
  const dataList = datasets(ctx);
  const live = 7;
  let liveTimer: ReturnType<typeof setInterval> | undefined;
  const steps = trainSteps(live);

  const explorerRefused = () => ctx.status.policy.assist === "off";

  function startWork(title: string, kind: Work["kind"], total: number, unit: string, seconds: number): Work {
    const id = Math.max(...ctx.work.map((w) => w.id)) + 1;
    const w: Work = {
      id,
      kind,
      status: "running",
      priority: "idea",
      title,
      subject: { type: "player", key: "dameless" },
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

  /** The live run of the network idea: a new reading every couple of seconds, until the schedule is done. */
  function followTraining() {
    if (liveTimer || live >= 20) return;
    liveTimer = setInterval(() => {
      const last = steps.at(-1)!;
      const next = last.batch >= 200 ? { sb: last.superbatch + 1, b: 20 } : { sb: last.superbatch, b: last.batch + 20 };
      if (next.sb > 20) {
        clearInterval(liveTimer);
        return;
      }
      const t = next.sb - 1 + next.b / 200;
      const step: TrainStep = {
        seq: last.seq + 1,
        superbatch: next.sb,
        batch: next.b,
        loss: 0.0215 + 0.0205 * Math.exp(-t / 3.2) + 0.0006 * Math.sin(t * 9),
        lr: 1e-4 * (0.5 + 0.5 * Math.cos((Math.PI * t) / 20)),
        grad_norm: 0.9,
        positions_per_sec: 750_000,
        elapsed_seconds: t * 4.3,
      };
      emit("train.progress", "ideas", {
        work: 9,
        idea: 9,
        superbatch: step.superbatch,
        superbatches: 20,
        batch: step.batch,
        batches_per_superbatch: 200,
        loss: step.loss,
        lr: step.lr,
        positions_per_sec: step.positions_per_sec,
        grad_norm: step.grad_norm,
        elapsed_seconds: step.elapsed_seconds,
      });
      // The server's copy of the steps keeps pace, so a reload shows where the run is.
      steps.push(step);
    }, 2000);
  }

  const data = () => {
    const used = dataList.reduce((s, d) => s + d.size_bytes, 0);
    return {
      cap_bytes: 10 * GB,
      used_bytes: used,
      disk_free_bytes: 212 * GB,
      keep_free_bytes: 15 * GB,
      datasets: structuredClone(dataList),
    };
  };

  return (method, path, url, body) => {
    let m: RegExpMatchArray | null;
    if (method === "GET") {
      if (path === "/api/drills/today") {
        const now = Date.now();
        const list = theDrills()
          .filter((d) => Date.parse(d.due) <= now + 3_600_000)
          .sort((a, b) => Date.parse(a.due) - Date.parse(b.due));
        return reply("/api/drills/today", "get", {
          drills: structuredClone(list),
          due: list.filter((d) => d.reps > 0 || d.answered_at).length,
          new: list.filter((d) => !d.answered_at).length,
          started_today: 0,
        });
      }
      if (path === "/api/drills") return reply("/api/drills", "get", structuredClone(theDrills()));
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
      if (path === "/api/nets") return reply("/api/nets", "get", structuredClone(netList));
      if (path === "/api/datasets") return reply("/api/datasets", "get", data());
      if ((m = path.match(/^\/api\/ideas\/(\d+)\/train$/))) {
        const idea = ctx.ideas.find((i) => i.id === Number(m![1]));
        if (!idea || idea.kind !== "net") return error(404, "not_found", "That idea does not train a network.");
        followTraining();
        return reply("/api/ideas/{id}/train", "get", {
          spec: {
            datasets: [1],
            parent: null,
            from_scratch: false,
            save_rate: 10,
            keep_checkpoints: 2,
            schedule: {
              superbatches: 20,
              batches_per_superbatch: 200,
              batch_size: 16_384,
              lr: 1e-4,
              final_lr: 1e-6,
              lr_schedule: "cosine",
              wdl: 0.3,
              final_wdl: null,
            },
          },
          steps: structuredClone(steps),
          net: null,
        });
      }
    }

    if (method === "POST") {
      if (path === "/api/drills") {
        return reply("/api/drills", "post", []);
      }
      if ((m = path.match(/^\/api\/drills\/(\d+)\/answer$/))) {
        const d = theDrills().find((x) => x.id === Number(m![1]));
        if (!d) return error(404, "not_found", "No such drill.");
        const move = String(body["move"] ?? "");
        const pos = setup(d.fen)!;
        const parsed = parseSan(pos, move);
        const uci = parsed ? uciOf(pos, parsed) : move;
        const outcome: DrillOutcome = d.best_moves.includes(uci) || d.best_san.includes(move) ? "pass" : "fail";
        const now = new Date().toISOString();
        const next = grade(d, outcome, now);
        drills = theDrills().map((x) => (x.id === d.id ? next : x));
        answered.push({ at: now, outcome });
        const isUci = /^[a-h][1-8][a-h][1-8][qrbn]?$/.test(move);
        const playedSan = isUci ? (playUci(setup(d.fen)!, move)?.san ?? move) : move;
        emit("drill.upserted", "player:you", structuredClone(next));
        emit("drill.answered", "player:you", { drill: d.id, outcome, at: now });
        return reply("/api/drills/{id}/answer", "post", {
          outcome,
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
      if (path === "/api/datasets/datagen") {
        const positions = Number(body["positions"] ?? 0);
        if (!(positions > 0)) return error(400, "bad_request", "Generate at least one position.");
        const name = String(body["name"] ?? "") || `datagen-${dataList.length + 1}`;
        const w = startWork(
          `Generating ${positions.toLocaleString("en-GB")} positions · ${name}`,
          "datagen",
          positions,
          "positions",
          8,
        );
        const id = Math.max(0, ...dataList.map((d) => d.id)) + 1;
        setTimeout(() => {
          const d: Dataset = {
            id,
            name,
            format: "viriformat",
            positions,
            games: Math.round(positions / 80),
            size_bytes: Math.round(positions * 4.3),
            source: { type: "datagen", settings: { threads: body["threads"] ?? 8 } },
            score_units: "reckless-internal",
            path: `data/${name}`,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          };
          dataList.unshift(d);
          emit("dataset.upserted", "training", structuredClone(d));
        }, 8000);
        return reply("/api/datasets/datagen", "post", structuredClone(w), 201);
      }
      if (path === "/api/datasets/import") {
        if (body["confirm"] !== true) return error(400, "bad_request", "Importing downloads a file: confirm it first.");
        const bytes = Number(body["expected_bytes"] ?? 0);
        const name = String(body["name"] ?? "import");
        const w = startWork(`Importing ${name}`, "dataset_import", bytes, "bytes", 8);
        const id = Math.max(0, ...dataList.map((d) => d.id)) + 1;
        setTimeout(() => {
          const d: Dataset = {
            id,
            name,
            format: "viriformat",
            positions: Math.round((bytes * 2.15) / 4.3),
            games: 0,
            size_bytes: Math.round(bytes * 2.15),
            source: { type: "import", url: String(body["url"] ?? "") },
            score_units: "reckless-internal",
            path: `data/${name}`,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          };
          dataList.unshift(d);
          emit("dataset.upserted", "training", structuredClone(d));
        }, 8000);
        return reply("/api/datasets/import", "post", structuredClone(w), 201);
      }
    }

    if (method === "DELETE") {
      if ((m = path.match(/^\/api\/drills\/(\d+)$/))) {
        drills = theDrills().filter((d) => d.id !== Number(m![1]));
        emit("drill.deleted", "player:you", { id: Number(m[1]) });
        return new Response(null, { status: 204 });
      }
      if ((m = path.match(/^\/api\/datasets\/(\d+)$/))) {
        if (body["confirm"] !== true) return error(400, "bad_request", "Deleting a dataset needs confirm: true.");
        const i = dataList.findIndex((d) => d.id === Number(m![1]));
        if (i < 0) return error(404, "not_found", "No such dataset.");
        const [gone] = dataList.splice(i, 1);
        return reply("/api/datasets/{id}", "delete", structuredClone(gone!));
      }
    }
    return undefined;
  };
}
