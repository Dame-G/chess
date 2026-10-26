import { stream } from "../api/stream";
import { useConnections } from "./connections";
import { useGames } from "./games";
import { useIdeas } from "./ideas";
import { usePolicy } from "./policy";
import { useVersions } from "./versions";
import { useWork } from "./work";

/**
 * Loads every store with its one GET and opens the event stream that keeps them current. Call once, after Pinia is
 * installed. A store that fails to load stays empty; the stream's reconnect reloads it.
 */
export function startStores() {
  const stores = [useGames(), useIdeas(), useWork(), useConnections(), usePolicy(), useVersions()];
  stream.start();
  return Promise.allSettled(stores.map((s) => s.load()));
}
