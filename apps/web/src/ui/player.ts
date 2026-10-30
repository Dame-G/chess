import type { Seat, Side } from "../api/types";
import { isLevelSeat, seatName } from "../format";

/** Someone who is not at a board: a name on a profile or in a list, with the key that opens their page. */
export interface PlayerRef {
  name: string;
  /** `you`, `engine:stockfish`, `lichess:name`: where a link goes. */
  key?: string;
  title?: string | null;
  rating?: number | null;
}

export type PlayerLike = Seat | PlayerRef;

const isSeat = (p: PlayerLike): p is Seat => "player" in p;

/** What to call them: a seat as the server names it (`Stockfish 1650`), a reference as it is written. */
export const playerName = (p: PlayerLike): string => (isSeat(p) ? seatName(p) : p.name);

/** The key of their page, if they have one. */
export const playerKey = (p: PlayerLike): string | undefined => (isSeat(p) ? p.player : p.key);

/** Where their page is: your profile, or a player's. */
export const playerPath = (key: string): string => (key === "you" ? "/you" : `/players/${key}`);

/** Who sits at a side of a game: you are "You", a level's name carries its rating, so it is shown once. */
export function sideRef(game: { you?: Side | null; white: Seat; black: Seat }, side: Side): PlayerRef {
  const seat = game[side];
  return {
    name: game.you === side ? "You" : seatName(seat),
    key: seat.player,
    title: seat.title,
    rating: isLevelSeat(seat) ? null : seat.rating,
  };
}
