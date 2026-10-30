import type { GameSummary, Seat } from "../../api/types";
import type { PlayerRef } from "../player";
import type { TableColumn } from "../table";

/** Sample data for the kit page: enough to show every state of every component, none of it real. */

export const youSeat: Seat = { player: "you", name: "You", rating: 1840 };
export const engineSeat: Seat = { player: "engine:stockfish", name: "Stockfish 17.1", rating: 3290 };
export const levelSeat: Seat = { player: "engine:stockfish", name: "Stockfish 1650", rating: 1650, level: 5 };
export const botSeat: Seat = { player: "lichess:alpha-bot", name: "Alpha-Bot", rating: 2410, title: "BOT" };
export const masterSeat: Seat = { player: "lichess:knightowl", name: "knightowl", rating: 2105, title: "FM" };
export const longSeat: Seat = {
  player: "lichess:a-very-long-username-that-keeps-going",
  name: "a-very-long-username-that-keeps-going",
  rating: 1345,
};

const base = {
  id: 1,
  plies: 38,
  started_at: "2026-10-28T10:00:00Z",
  time: { type: "clock", initial_ms: 300_000, increment_ms: 3_000 },
} as const;

export const liveGame: GameSummary = {
  ...base,
  white: youSeat,
  black: engineSeat,
  source: { type: "local" },
  status: "live",
  you: "white",
  opening: { name: "Italian Game", eco: "C50" },
};

export const wonGame: GameSummary = {
  ...base,
  id: 2,
  white: botSeat,
  black: youSeat,
  source: { type: "lichess", connection: "you", id: "abc123", rated: true, speed: "blitz" },
  status: "finished",
  you: "black",
  outcome: "black_wins",
  ending: "resignation",
  opening: { name: "Sicilian Defense: Najdorf Variation", eco: "B90" },
};

export const lostGame: GameSummary = {
  ...wonGame,
  id: 3,
  white: youSeat,
  black: masterSeat,
  you: "white",
  outcome: "black_wins",
  ending: "timeout",
};

export const strangersGame: GameSummary = {
  ...base,
  id: 4,
  white: masterSeat,
  black: longSeat,
  source: { type: "import", owner: "lichess:knightowl", id: "xyz789", rated: false, speed: "rapid" },
  status: "finished",
  outcome: "draw",
  ending: "repetition",
};

export interface LevelRow {
  level: number;
  name: string;
  rating: number;
  nodes: number;
  games: number;
  score: number | null;
}

export const levelRows: LevelRow[] = [
  { level: 1, name: "Level 1", rating: 820, nodes: 300, games: 240, score: 96.2 },
  { level: 2, name: "Level 2", rating: 1105, nodes: 1200, games: 232, score: 88.4 },
  { level: 3, name: "Level 3", rating: 1390, nodes: 5000, games: 226, score: 71.9 },
  { level: 4, name: "Level 4", rating: 1650, nodes: 20_000, games: 118, score: 52.1 },
  { level: 5, name: "Level 5", rating: 1910, nodes: 80_000, games: 64, score: null },
  { level: 6, name: "Level 6", rating: 2330, nodes: 400_000, games: 12, score: 8.3 },
];

export const levelColumns: TableColumn[] = [
  { key: "name", label: "Level" },
  { key: "rating", label: "Rating", fig: true, sort: true },
  { key: "nodes", label: "Nodes a move", fig: true, sort: true, hide: "sm" },
  { key: "games", label: "Games", fig: true, sort: true, hide: "md" },
  { key: "score", label: "Score", fig: true, sort: true },
];

/** A person who is not at a board, as a profile or a list names them. */
export const nameOnly: PlayerRef = { name: "Someone", key: "lichess:someone", rating: 1500 };
