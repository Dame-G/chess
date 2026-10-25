import { computed, reactive } from "vue";
import createClient from "openapi-fetch";
import type { paths } from "./schema";

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);
const STORAGE = "chess.runner";
export const DEFAULT_RUNNER = "http://127.0.0.1:7878";

export interface Connection {
  /** The runner's URL, or "" for the page's own origin. */
  url: string;
  /** The runner's token. Only the hosted UI needs it. */
  token: string;
}

/**
 * Where the runner is. Served by the runner itself (or the dev server, which proxies it), the page and the API share
 * an origin and need nothing else. On chess.dame.gg the runner is remembered with its token in this browser.
 */
export function initialConnection(location: Pick<Location, "hostname">, saved: string | null): Connection {
  if (LOOPBACK.has(location.hostname)) return { url: "", token: "" };
  try {
    const parsed = JSON.parse(saved ?? "") as Partial<Connection>;
    return { url: parsed.url || DEFAULT_RUNNER, token: parsed.token ?? "" };
  } catch {
    return { url: DEFAULT_RUNNER, token: "" };
  }
}

function readSaved(): string | null {
  try {
    return localStorage.getItem(STORAGE);
  } catch {
    return null;
  }
}

export const connection = reactive<Connection>(initialConnection(location, readSaved()));

/** Whether this page reaches the runner on another origin, with the token. */
export const isRemote = computed(() => connection.url !== "");

export function connect(url: string, token: string) {
  connection.url = url.replace(/\/+$/, "");
  connection.token = token.trim();
  try {
    localStorage.setItem(STORAGE, JSON.stringify(connection));
  } catch {}
}

/** Adds the token, and tells the browser a public page means to reach a local address (Local Network Access). */
export function runnerFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const headers = new Headers(input instanceof Request ? input.headers : init?.headers);
  if (connection.token) headers.set("Authorization", `Bearer ${connection.token}`);
  const local = isRemote.value && LOOPBACK.has(new URL(connection.url).hostname);
  return fetch(input, { ...init, headers, ...(local ? { targetAddressSpace: "local" } : {}) });
}

/** The typed API client. Paths are resolved against the current connection on every call, so reconnecting needs no new client. */
export const api = createClient<paths>({
  baseUrl: "",
  fetch: (request: Request) => {
    const { pathname, search } = new URL(request.url);
    return runnerFetch(new Request(connection.url + pathname + search, request));
  },
});
