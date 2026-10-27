import { onScopeDispose } from "vue";
import { onEvent, onReconnect } from "../api/stream";
import type { EventKind, EventPayloads } from "../api/types";

/** Listens for one kind of event for as long as the store lives. Call it inside a store's setup. */
export function listen<K extends EventKind>(kind: K, handler: (data: EventPayloads[K]) => void) {
  onScopeDispose(onEvent(kind, handler));
}

/** Reloads the store after the stream came back from a drop. */
export function reloadOnReconnect(load: () => Promise<unknown>) {
  onScopeDispose(onReconnect(() => void load().catch(() => {})));
}
