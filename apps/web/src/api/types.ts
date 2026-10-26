/**
 * The API's types, mirroring `crates/core` as docs/design.md writes them. One file, so that once the server's OpenAPI
 * document exists each of these becomes an alias of `components["schemas"][...]` in `schema.d.ts` and nothing else in
 * the app changes. Serde names are snake_case; enums with data are tagged with `type`.
 */

// ---- Identity

export type GameId = number;
export type IdeaId = number;
export type TrialId = number;
export type WorkId = number;
export type VersionId = number;
export type BuildId = number;
export type ReviewId = number;
/** ISO-8601 UTC. */
export type Timestamp = string;

export type Side = "white" | "black";
/** A choice when starting a game. */
export type Color = "white" | "black" | "random";

/** Who sits at a board, as its string key: "you", "dameless", "lichess:<id>", "stockfish:<1-8>". */
export type PlayerKey = "you" | "dameless" | `lichess:${string}` | `stockfish:${number}`;

/** A value for each side. */
export interface SidePair<T> {
  white: T;
  black: T;
}

// ---- Scores and time

/** Always from White's point of view. `mate: 0` means the side to move is mated. */
export type Score = { cp: number } | { mate: number };

export type TimeControl =
  | { type: "clock"; initial_ms: number; increment_ms: number }
  | { type: "correspondence"; days: number }
  | { type: "movetime"; ms: number }
  | { type: "nodes"; nodes: number };

export type Speed = "ultra_bullet" | "bullet" | "blitz" | "rapid" | "classical" | "correspondence";

// ---- Games

export interface Seat {
  player: PlayerKey;
  /** "You" is shown as the Lichess username when linked. */
  name: string;
  version: VersionId | null;
  build: BuildId | null;
  rating: number | null;
  title: string | null;
}

export type ConnectionKey = "you" | "bot";

export type Source =
  | { type: "local" }
  | { type: "lichess"; connection: ConnectionKey; id: string; rated: boolean; speed: Speed }
  | { type: "import"; owner: PlayerKey; id: string; rated: boolean; speed: Speed }
  | { type: "trial"; trial: TrialId; index: number };

export type GameStatus = "starting" | "live" | "finished" | "aborted";
export type Outcome = "white_wins" | "black_wins" | "draw";
export type Ending =
  | "checkmate"
  | "resignation"
  | "timeout"
  | "stalemate"
  | "repetition"
  | "fifty_moves"
  | "insufficient_material"
  | "agreement"
  | "adjudication"
  | "abandoned"
  | "aborted"
  | "engine_failure"
  | "illegal_move"
  | "other";

export interface Opening {
  eco: string;
  name: string;
  ply: number;
}

/** What lists, events and the Games rail use. */
export interface GameSummary {
  id: GameId;
  white: Seat;
  black: Seat;
  source: Source;
  time: TimeControl;
  status: GameStatus;
  outcome: Outcome | null;
  ending: Ending | null;
  opening: Opening | null;
  plies: number;
  started_at: Timestamp;
  ended_at: Timestamp | null;
  /** The side You played, if any. */
  you: Side | null;
  /** From the newest review. */
  accuracy: SidePair<number> | null;
  /** The lichess.org link. */
  url: string | null;
}

export interface Move {
  uci: string;
  san: string;
  /** The mover's clock after the move. */
  clock_ms: number | null;
  think_ms: number | null;
  /** The mover's engine's own score (White POV), engines only. */
  eval: Score | null;
  depth: number | null;
  nodes: number | null;
}

export interface Offers {
  draw: Side | null;
  takeback: Side | null;
}

/** One set of actions for every game. `Game.actions` lists what You may do now. */
export type SeatAction =
  | "move"
  | "offer_draw"
  | "accept_draw"
  | "decline_draw"
  | "takeback"
  | "accept_takeback"
  | "decline_takeback"
  | "resign"
  | "abort"
  | "claim_victory"
  | "claim_draw";

/** The full record. */
export interface Game {
  summary: GameSummary;
  /** `null` is the standard start. */
  start_fen: string | null;
  moves: Move[];
  /** Live: the clock each side holds now, in ms. */
  clocks: SidePair<number> | null;
  offers: Offers;
  actions: SeatAction[];
}

// ---- Versions

export type Line =
  { type: "champion"; number: number } | { type: "idea"; idea: IdeaId } | { type: "upstream" } | { type: "other" };

/** An estimate with its 95 % interval. */
export interface Estimate {
  elo: number;
  low: number;
  high: number;
}

export interface Version {
  id: VersionId;
  /** "v14" on the champion line, else the branch name. */
  name: string;
  commit: string;
  branch: string | null;
  line: Line;
  /** The champion it was built on. */
  parent: VersionId | null;
  /** The commit's Bench: line. */
  bench: number | null;
  /** Against its parent, from the deciding trial. */
  elo: Estimate | null;
  created_at: Timestamp;
}

// ---- Ideas and trials

export type IdeaKind = "gain" | "neutral" | "speed" | "tune";
export type IdeaStatus = "queued" | "running" | "kept" | "dropped" | "stopped" | "failed";
export type Author = "you" | "claude";
export type Stage = "fingerprint" | "screen" | "short" | "long" | "speed" | "tune" | "confirm" | "custom";
export type TrialStatus = "waiting" | "running" | "passed" | "failed" | "stopped" | "skipped";
export type Preset = "vstc" | "stc" | "ltc" | "stc_hash" | "ltc_hash" | "smp" | "nodes";

export interface Llr {
  value: number;
  lower: number;
  upper: number;
}

export interface Scaling {
  copies: number;
  dev_nps: number;
  base_nps: number;
  measured_nps: number;
  reference_nps: number;
  scale: number;
}

export interface Trial {
  id: TrialId;
  idea: IdeaId;
  stage: Stage;
  status: TrialStatus;
  preset: Preset | null;
  /** As written (8+0.08). */
  time: TimeControl | null;
  /** After scaling (11+0.11). */
  played: TimeControl | null;
  scaling: Scaling | null;
  /** nElo. */
  bounds: [number, number] | null;
  /** LL, LD, DD|WL, WD, WW pairs. */
  penta: [number, number, number, number, number];
  wdl: [number, number, number];
  llr: Llr | null;
  elo: Estimate | null;
  nelo: Estimate | null;
  /** One sentence for people. */
  detail: string | null;
  started_at: Timestamp | null;
  ended_at: Timestamp | null;
}

/** What an idea carries of each of its trials. */
export type TrialSummary = Pick<Trial, "id" | "stage" | "status" | "time" | "penta" | "llr" | "elo" | "detail">;

export interface Idea {
  id: IdeaId;
  /** "Deeper LMR at high depth". */
  title: string;
  hypothesis: string;
  kind: IdeaKind;
  author: Author;
  /** The change (for a tune: the branch with the tunables). */
  branch: string;
  base: VersionId;
  version: VersionId | null;
  status: IdeaStatus;
  /** One sentence: "Kept: +3.1 ± 2.0 Elo at short games". */
  verdict: string | null;
  trials: TrialSummary[];
  /** Batches are tags. */
  tags: string[];
  /** Markdown. */
  notes: string | null;
  created_at: Timestamp;
  ended_at: Timestamp | null;
}

// ---- Reviews (the old shapes, every score White POV)

export type Classification =
  "brilliant" | "great" | "best" | "excellent" | "good" | "book" | "inaccuracy" | "mistake" | "blunder" | "miss";
export type Judgement = "inaccuracy" | "mistake" | "blunder";
export type Phase = "opening" | "middlegame" | "endgame";

export type ClassCounts = Record<Classification, number>;

export interface PhaseReview {
  moves: number;
  accuracy: number | null;
  acpl: number | null;
  timed_moves: number;
  time_ms: number;
}

export interface SideReview {
  moves: number;
  accuracy: number | null;
  acpl: number | null;
  counts: ClassCounts;
  judgements: Record<Judgement, number>;
  phases: Record<Phase, PhaseReview>;
  time_trouble_blunders: number;
}

export interface ReviewSummary {
  version: VersionId;
  nodes: number;
  white: SideReview;
  black: SideReview;
  middlegame_ply: number | null;
  endgame_ply: number | null;
}

export interface ReviewedMove {
  ply: number;
  side: Side;
  uci: string;
  san: string;
  eval_before: Score;
  eval_after: Score;
  best_uci: string;
  best_san: string;
  best_pv: string[];
  second_uci: string | null;
  eval_second: Score | null;
  /** The mover's. */
  win_before: number;
  win_after: number;
  accuracy: number;
  cp_loss: number;
  classification: Classification;
  judgement: Judgement | null;
  phase: Phase;
  sacrifice: number;
  clock_before_ms: number | null;
  depth: number;
}

export interface Review {
  id: ReviewId;
  game: GameId;
  version: VersionId;
  nodes: number;
  summary: ReviewSummary;
  plies: ReviewedMove[];
  created_at: Timestamp;
}

// ---- Players

export interface Rating {
  rating: number;
  games: number;
  provisional: boolean;
}
export type Ratings = Partial<Record<"bullet" | "blitz" | "rapid" | "classical", Rating>>;

export interface Player {
  key: PlayerKey;
  name: string;
  title: string | null;
  ratings: Ratings;
  /** The Lichess username. */
  lichess: string | null;
}

/** `GET /api/players/{key}/profile`, as far as the app reads it yet. The sheets deepen it. */
export interface Profile {
  player: Player;
  games: number;
  reviewed: number;
  accuracy: { overall: number | null };
}

// ---- Work

export type WorkKind = "build" | "trial" | "tune" | "review" | "import" | "promote";
export type WorkStatus = "queued" | "running" | "paused" | "done" | "failed" | "stopped";
/** Lower runs first: live 0, interactive 1, review 2, idea 3. */
export type Priority = "live" | "interactive" | "review" | "idea";

export type Subject =
  | { type: "idea"; id: IdeaId }
  | { type: "trial"; id: TrialId }
  | { type: "game"; id: GameId }
  | { type: "player"; key: PlayerKey }
  | { type: "version"; id: VersionId };

export interface Progress {
  done: number;
  total: number | null;
  unit: string;
  eta_seconds: number | null;
  text: string | null;
}

export interface Work {
  id: WorkId;
  kind: WorkKind;
  status: WorkStatus;
  priority: Priority;
  /** "Testing deeper-lmr · short games". */
  title: string;
  subject: Subject;
  /** Granted now. */
  cores: number;
  progress: Progress | null;
  /** "Your Lichess game" | "Live analysis". */
  paused_for: string | null;
  error: string | null;
  created_at: Timestamp;
  started_at: Timestamp | null;
  ended_at: Timestamp | null;
}

// ---- Connections (minimal: the sheets deepen it)

export type ConnectionState = "off" | "connecting" | "online" | "error";

export interface Account {
  id: string;
  username: string;
  title: string | null;
  ratings: Ratings;
}

export interface ConnectionStatus {
  key: ConnectionKey;
  configured: boolean;
  account: Account | null;
  state: ConnectionState;
  error: string | null;
  /** The games being played through this connection. */
  playing: GameId[];
  /** The bot only: whether it is set to be online. */
  online?: boolean;
}

export interface Connections {
  you: ConnectionStatus;
  bot: ConnectionStatus;
}

export interface Challenge {
  id: string;
  connection: ConnectionKey;
  direction: "incoming" | "outgoing";
  opponent: { name: string; title: string | null; rating: number | null };
  rated: boolean;
  speed: Speed;
  time: TimeControl | null;
  color: Color;
  url: string;
}

// ---- Policy (fair play)

export type Assist = "allowed" | "explorer_only" | "off";

export interface Policy {
  assist: Assist;
  /** "Analysis is off while you're playing on Lichess". */
  reason: string | null;
}

/** `GET /api/status`. */
export interface Status {
  version: string;
  cores: number;
  policy: Policy;
}

// ---- Evaluation

export interface EvalLine {
  score: Score;
  pv: string[];
}

export interface EvalInfo {
  session: string;
  depth: number;
  lines: EvalLine[];
  nodes: number;
  nps: number;
}

// ---- Errors

export type ErrorCode =
  "bad_request" | "unauthorized" | "not_found" | "conflict" | "fair_play" | "rate_limited" | "upstream";

export interface ApiErrorBody {
  error: { code: ErrorCode; message: string };
}

// ---- Events

export interface GameMoved {
  id: GameId;
  ply: number;
  move: Move;
  clocks: SidePair<number> | null;
  fen: string;
}

/** A takeback: the game now has `plies` moves. */
export interface GameUndone {
  id: GameId;
  plies: number;
}

export interface GameChanged {
  id: GameId;
  offers: Offers;
  actions: SeatAction[];
  clocks: SidePair<number> | null;
  opponent_gone: { claim_in_seconds: number } | null;
}

export interface GameThinking {
  id: GameId;
  ply: number;
  side: Side;
  depth: number;
  score: Score;
  nodes: number;
  nps: number;
  pv: string[];
}

export interface TrialProgress {
  id: TrialId;
  penta: Trial["penta"];
  wdl: Trial["wdl"];
  llr: Llr | null;
  elo: Estimate | null;
}

export interface TuneProgress {
  idea: IdeaId;
  iteration: number;
  values: number[];
  result: number;
}

export interface WorkLog {
  id: WorkId;
  line: string;
}

export interface ReviewUpserted {
  game: GameId;
  accuracy: SidePair<number>;
}

export type ChallengeEvent = Challenge & { event: "created" | "accepted" | "declined" | "canceled" };

/** Each kind with its payload: a closed set, as `core::events`. */
export interface EventPayloads {
  "game.upserted": GameSummary;
  "game.moved": GameMoved;
  "game.undone": GameUndone;
  "game.changed": GameChanged;
  "game.thinking": GameThinking;
  "idea.upserted": Idea;
  "trial.upserted": Trial;
  "trial.progress": TrialProgress;
  "tune.progress": TuneProgress;
  "work.upserted": Work;
  "work.log": WorkLog;
  "version.upserted": Version;
  "review.upserted": ReviewUpserted;
  "connection.upserted": ConnectionStatus;
  challenge: ChallengeEvent;
  "policy.changed": Policy;
  "eval.info": EvalInfo;
}

export type EventKind = keyof EventPayloads;

/** One event of the stream, its payload typed by its kind. */
export type AppEvent = {
  [K in EventKind]: { id: number; topic: string; kind: K; data: EventPayloads[K] };
}[EventKind];
