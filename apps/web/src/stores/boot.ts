import { stream } from "../api/stream";
import { useConnections } from "./connections";
import { useGames } from "./games";
import { useIdeas } from "./ideas";
import { usePolicy } from "./policy";
import { useVersions } from "./versions";
import { useWork } from "./work";

/**
 * Loads every store with its one GET and opens the event stream that keeps them current. Call once, after Pinia is
 * installed. The status comes first: its `next_event` is where the stream starts, so the events that led to what the
 * stores load are not replayed (a replayed `game.upserted` of a game long over would announce it again), and nothing
 * after it is missed. A store that fails to load stays empty; the stream's reconnect reloads it.
 */
export async function startStores() {
  const policy = usePolicy();
  const stores = [useGames(), useIdeas(), useWork(), useConnections(), useVersions()];
  const status = await policy.load().then(
    () => policy.status,
    () => null,
  );
  stream.start(2000, status ? status.next_event - 1 : undefined);
  return Promise.allSettled(stores.map((s) => s.load()));
}
