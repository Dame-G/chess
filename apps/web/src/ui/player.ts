import type { Seat } from "../api/types";
import { seatName } from "../format";

/** Someone who is not at a board: a name on a profile or in a list, with the key that opens their page. */
export interface PlayerRef {
  name: string;
  /** `you`, `dameless`, `lichess:name`, `stockfish:5`: where a link goes. */
  key?: string;
  title?: string | null;
  rating?: number | null;
}

export type PlayerLike = Seat | PlayerRef;

const isSeat = (p: PlayerLike): p is Seat => "player" in p;

/** What to call them: a seat as people say it (`Dameless v14`), a reference as it is written. */
export const playerName = (p: PlayerLike): string => (isSeat(p) ? seatName(p) : p.name);

/** The key of their page, if they have one. */
export const playerKey = (p: PlayerLike): string | undefined => (isSeat(p) ? p.player : p.key);

/** Where their page is: your profile, Dameless's, or a player's. */
export const playerPath = (key: string): string =>
  key === "you" ? "/you" : key === "dameless" ? "/dameless" : `/players/${key}`;
