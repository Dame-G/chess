import { Chess } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { INITIAL_FEN, makeFen, parseFen } from "chessops/fen";
import { parsePgn, startingPosition } from "chessops/pgn";
import { makeSan, makeSanVariation, parseSan } from "chessops/san";
import { makeUci, parseUci } from "chessops/util";
import type { Color, Key } from "@lichess-org/chessground/types";
import { signed } from "../format";

/** A position as the runner takes it: a starting FEN (or `startpos`) and the UCI moves played from it. */
export interface Line {
  fen: string;
  moves: string[];
}

export interface Position {
  /** The FEN after the moves. */
  fen: string;
  turn: Color;
  check: boolean;
  /** Legal moves for the board: from-square to destinations. */
  dests: Map<Key, Key[]>;
  lastMove?: [Key, Key];
  /** Moves that were not legal, with everything after them, are dropped. */
  moves: string[];
}

function setup(fen: string): Chess | undefined {
  const parsed = parseFen(fen === "startpos" ? INITIAL_FEN : fen);
  if (parsed.isErr) return undefined;
  const pos = Chess.fromSetup(parsed.value);
  return pos.isOk ? pos.value : undefined;
}

export function isValidFen(fen: string): boolean {
  return setup(fen) !== undefined;
}

/** Plays a line from its FEN. `undefined` when the FEN is not a legal position. */
export function play(line: Line): Position | undefined {
  const pos = setup(line.fen);
  if (!pos) return undefined;
  const played: string[] = [];
  let lastMove: [Key, Key] | undefined;
  for (const uci of line.moves) {
    const move = parseUci(uci);
    if (!move || !pos.isLegal(move)) break;
    pos.play(move);
    played.push(makeUci(move));
    lastMove = [uci.slice(0, 2) as Key, uci.slice(2, 4) as Key];
  }
  return {
    fen: makeFen(pos.toSetup()),
    turn: pos.turn,
    check: pos.isCheck(),
    dests: chessgroundDests(pos) as Map<Key, Key[]>,
    lastMove,
    moves: played,
  };
}

/** A UCI move from the board, promoting to a queen when a pawn reaches the last rank. */
export function boardMove(position: Position, from: Key, to: Key): string {
  const pawn = position.fen.split(" ")[0]!;
  const rank = to[1];
  const piece = pieceAt(pawn, from);
  const promotes = (piece === "P" && rank === "8") || (piece === "p" && rank === "1");
  return `${from}${to}${promotes ? "q" : ""}`;
}

function pieceAt(boardFen: string, square: Key): string | undefined {
  const file = square.charCodeAt(0) - 97;
  const rank = 8 - Number(square[1]);
  const row = boardFen.split("/")[rank] ?? "";
  let col = 0;
  for (const ch of row) {
    if (/\d/.test(ch)) col += Number(ch);
    else if (col++ === file) return ch;
  }
  return undefined;
}

/** A principal variation in SAN (`1. e4 e5 2. Nf3`), as far as its moves are legal. */
export function sanLine(fen: string, pv: string[]): string {
  const pos = setup(fen);
  if (!pos) return pv.join(" ");
  const moves = [];
  for (const uci of pv) {
    const move = parseUci(uci);
    if (!move) break;
    moves.push(move);
  }
  return makeSanVariation(pos, moves);
}

export type Score = { cp: number } | { mate: number };

/** A score for display from White's side: `+0.35`, `−1.20`, `M3` (White mates in 3), `−M2`. Engines report it for the side to move. */
export function whiteScore(score: Score, turn: Color): string {
  const sign = turn === "white" ? 1 : -1;
  if ("mate" in score) return `${score.mate * sign < 0 ? "−" : ""}M${Math.abs(score.mate)}`;
  const pawns = (score.cp * sign) / 100;
  return signed(pawns);
}

/** A score from White's side in pawns, with mates as ±100 so a graph or a bar can draw them. */
export function whitePawns(score: Score, turn: Color): number {
  const sign = turn === "white" ? 1 : -1;
  if ("mate" in score) return (score.mate > 0 ? 100 : -100) * sign;
  return (score.cp * sign) / 100;
}

/** The SAN of each move of a line, as far as its moves are legal. */
export function sanMoves(line: Line): string[] {
  const pos = setup(line.fen);
  if (!pos) return [];
  const sans: string[] = [];
  for (const uci of line.moves) {
    const move = parseUci(uci);
    if (!move || !pos.isLegal(move)) break;
    sans.push(makeSan(pos, move));
    pos.play(move);
  }
  return sans;
}

/** A line as PGN: a FEN header when it does not start from the start, then numbered moves. */
export function toPgn(line: Line): string {
  const fen = line.fen === "startpos" ? INITIAL_FEN : line.fen;
  const pos = setup(fen);
  if (!pos) return "";
  const moves = line.moves.map(parseUci).filter((m) => m !== undefined);
  const headers = fen === INITIAL_FEN ? "" : `[SetUp "1"]\n[FEN "${fen}"]\n\n`;
  return `${headers}${makeSanVariation(pos, moves)} *`.trim();
}

/** Reads a FEN, or a PGN's main line. `undefined` when it is neither. */
export function readPosition(text: string): Line | undefined {
  const trimmed = text.trim();
  if (!trimmed) return undefined;
  if (isValidFen(trimmed)) return { fen: trimmed, moves: [] };
  const game = parsePgn(trimmed)[0];
  if (!game) return undefined;
  const start = startingPosition(game.headers);
  if (start.isErr) return undefined;
  const pos = start.value;
  const fen = makeFen(pos.toSetup());
  const moves: string[] = [];
  for (const node of game.moves.mainline()) {
    const move = parseSan(pos, node.san);
    if (!move) break;
    moves.push(makeUci(move));
    pos.play(move);
  }
  if (!moves.length && !game.headers.has("FEN")) return undefined;
  return { fen: fen === INITIAL_FEN ? "startpos" : fen, moves };
}

/** What a pasted text came to: the position it holds, or the one thing to fix. */
export type Reading =
  | { empty: true }
  | { empty?: false; line: Line; kind: "fen" | "pgn"; note?: string }
  | { empty?: false; error: string };

const setupProblem: Record<string, string> = {
  ERR_EMPTY: "There are no pieces on the board.",
  ERR_KINGS: "Each side needs exactly one king.",
  ERR_OPPOSITE_CHECK: "The side that is not to move is in check, which cannot happen.",
  ERR_PAWNS_ON_BACKRANK: "A pawn stands on the first or last rank.",
};

/** Reads a pasted FEN or PGN and says what is wrong with it in words, for a live check as the person types. */
export function inspectPosition(text: string, which = 0): Reading {
  const trimmed = text.trim();
  if (!trimmed) return { empty: true };
  const parsed = parseFen(trimmed);
  if (parsed.isOk) {
    const pos = Chess.fromSetup(parsed.value);
    if (pos.isErr) return { error: setupProblem[pos.error.message] ?? "That is not a legal position." };
    return { line: { fen: trimmed, moves: [] }, kind: "fen" };
  }
  // Seven slashes and a side to move: they meant a FEN, so say what is wrong with it rather than "not a PGN".
  if (/^\S+(\/\S+){7}(\s|$)/.test(trimmed)) {
    return {
      error: "The board part of that FEN does not have eight ranks of eight squares, or it has an unknown piece.",
    };
  }
  const game = parsePgn(trimmed)[which] ?? parsePgn(trimmed)[0];
  if (!game) return { error: "Paste a FEN, or a PGN with moves." };
  const start = startingPosition(game.headers);
  if (start.isErr) return { error: "The FEN in that PGN's header is not a legal position." };
  const pos = start.value;
  const fen = makeFen(pos.toSetup());
  const moves: string[] = [];
  let note: string | undefined;
  let n = 0;
  for (const node of game.moves.mainline()) {
    n++;
    const move = parseSan(pos, node.san);
    if (!move) {
      note = `Move ${n}, ${node.san}, is not legal there, so the game stops before it.`;
      break;
    }
    moves.push(makeUci(move));
    pos.play(move);
  }
  if (!moves.length && !game.headers.has("FEN")) {
    return { error: note ?? "No moves found. Paste a FEN, or a PGN with moves." };
  }
  return { line: { fen: fen === INITIAL_FEN ? "startpos" : fen, moves }, kind: "pgn", note };
}

/** One game of a PGN file, as a list shows it. */
export interface PgnGame {
  label: string;
  result: string;
  moves: number;
}

/** The games of a PGN with more than one, so the person can choose; empty for a FEN or a single game. */
export function listGames(text: string): PgnGame[] {
  const trimmed = text.trim();
  if (!trimmed || isValidFen(trimmed)) return [];
  const games = parsePgn(trimmed);
  if (games.length < 2) return [];
  return games.map((g, i) => {
    const white = g.headers.get("White") ?? "?";
    const black = g.headers.get("Black") ?? "?";
    const moves = [...g.moves.mainline()].length;
    return { label: i + 1 + ". " + white + " – " + black, result: g.headers.get("Result") ?? "*", moves };
  });
}
