import { Chess, type Position as ChessPosition } from "chessops/chess";
import { chessgroundDests } from "chessops/compat";
import { INITIAL_FEN, makeFen, parseFen } from "chessops/fen";
import { parsePgn, startingPosition } from "chessops/pgn";
import { makeSan, makeSanVariation, parseSan } from "chessops/san";
import { makeUci, parseUci } from "chessops/util";
import { isDrop, type Move } from "chessops/types";
import type { Color, Key } from "@lichess-org/chessground/types";

export { INITIAL_FEN };

/** A position as the server takes it: a starting FEN and the UCI moves played from it. */
export interface Line {
  fen: string;
  moves: string[];
}

/** What the board needs to show a position. */
export interface Position {
  /** The FEN after the moves. */
  fen: string;
  turn: Color;
  check: boolean;
  /** Legal moves for the board: from-square to destinations. */
  dests: Map<Key, Key[]>;
  lastMove?: [Key, Key];
}

/**
 * A move as the engine takes it. chessops writes castling as the king taking its own rook (`e1h1`); engines want the
 * king's two-square step (`e1g1`). Chess960 castling, where the king does not start on the e-file, keeps chessops' form.
 * Call it before the move is played.
 */
export function uciOf(pos: ChessPosition, move: Move): string {
  if (!isDrop(move)) {
    const king = pos.board.get(move.from);
    const target = pos.board.get(move.to);
    const file = move.to % 8;
    if (
      king?.role === "king" &&
      target?.role === "rook" &&
      target.color === king.color &&
      move.from % 8 === 4 &&
      (file === 0 || file === 7) &&
      Math.floor(move.from / 8) === Math.floor(move.to / 8)
    ) {
      return makeUci({ from: move.from, to: move.to > move.from ? move.from + 2 : move.from - 2 });
    }
  }
  return makeUci(move);
}

/** A chessops position from a FEN; `undefined` when it is not a legal one. */
export function setup(fen: string): Chess | undefined {
  const parsed = parseFen(fen === "startpos" ? INITIAL_FEN : fen);
  if (parsed.isErr) return undefined;
  const pos = Chess.fromSetup(parsed.value);
  return pos.isOk ? pos.value : undefined;
}

export function isValidFen(fen: string): boolean {
  return setup(fen) !== undefined;
}

/** The board's view of a chessops position, with the last move to highlight. */
export function view(pos: ChessPosition, lastUci?: string): Position {
  return {
    fen: makeFen(pos.toSetup()),
    turn: pos.turn,
    check: pos.isCheck(),
    dests: chessgroundDests(pos) as Map<Key, Key[]>,
    lastMove: lastUci ? [lastUci.slice(0, 2) as Key, lastUci.slice(2, 4) as Key] : undefined,
  };
}

/**
 * Plays one move given in UCI (either castling form) on a position. Returns the move as engines write it and its SAN,
 * or `undefined` when it is not legal there.
 */
export function playUci(pos: ChessPosition, uci: string): { uci: string; san: string } | undefined {
  const move = parseUci(uci);
  if (!move || !pos.isLegal(move)) return undefined;
  const standard = uciOf(pos, move);
  const san = makeSan(pos, move);
  pos.play(move);
  return { uci: standard, san };
}

/** A UCI move from the board, promoting to a queen when a pawn reaches the last rank. */
export function boardMove(position: Position, from: Key, to: Key): string {
  const board = position.fen.split(" ")[0]!;
  const rank = to[1];
  const piece = pieceAt(board, from);
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

/** A line as PGN: a FEN header when it does not start from the start, then numbered moves. */
export function toPgn(line: Line, headers: Record<string, string> = {}): string {
  const fen = line.fen === "startpos" ? INITIAL_FEN : line.fen;
  const pos = setup(fen);
  if (!pos) return "";
  const moves = line.moves.map(parseUci).filter((m) => m !== undefined);
  const head = Object.entries({ ...headers, ...(fen === INITIAL_FEN ? {} : { SetUp: "1", FEN: fen }) })
    .map(([k, v]) => `[${k} "${v.replace(/"/g, "'")}"]`)
    .join("\n");
  return `${head ? `${head}\n\n` : ""}${makeSanVariation(pos, moves)} ${headers["Result"] ?? "*"}`.trim();
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

/** Reads a pasted FEN or PGN (its main line) and says what is wrong with it in words, for a live check as one types. */
export function inspectPosition(text: string): Reading {
  const trimmed = text.trim();
  if (!trimmed) return { empty: true };
  const parsed = parseFen(trimmed);
  if (parsed.isOk) {
    const pos = Chess.fromSetup(parsed.value);
    if (pos.isErr) return { error: setupProblem[pos.error.message] ?? "That is not a legal position." };
    return { line: { fen: makeFen(pos.value.toSetup()), moves: [] }, kind: "fen" };
  }
  // Seven slashes and a side to move: they meant a FEN, so say what is wrong with it rather than "not a PGN".
  if (/^\S+(\/\S+){7}(\s|$)/.test(trimmed)) {
    return {
      error: "The board part of that FEN does not have eight ranks of eight squares, or it has an unknown piece.",
    };
  }
  const game = parsePgn(trimmed)[0];
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
    moves.push(uciOf(pos, move));
    pos.play(move);
  }
  if (!moves.length && !game.headers.has("FEN")) {
    return { error: note ?? "No moves found. Paste a FEN, or a PGN with moves." };
  }
  return { line: { fen, moves }, kind: "pgn", note };
}
