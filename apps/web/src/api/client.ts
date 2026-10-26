import { reactive } from "vue";
import type { ApiErrorBody, ErrorCode } from "./types";

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);
const STORAGE = "dameless.server";
const DEFAULT_SERVER = "http://127.0.0.1:7979";

export interface Connection {
  /** The server's URL, or "" for the page's own origin. */
  url: string;
  /** The server's token. Only the hosted app needs it. */
  token: string;
}

/**
 * Where the server is. Served by `dameless serve` itself (or the dev server, which proxies it), the page and the API
 * share an origin and need nothing else. On chess.dame.gg the paired server is remembered with its token in this
 * browser.
 */
function initialConnection(location: Pick<Location, "hostname">, saved: string | null): Connection {
  if (LOOPBACK.has(location.hostname) || location.hostname.endsWith(".localhost")) return { url: "", token: "" };
  try {
    const parsed = JSON.parse(saved ?? "") as Partial<Connection>;
    return { url: parsed.url || DEFAULT_SERVER, token: parsed.token ?? "" };
  } catch {
    return { url: DEFAULT_SERVER, token: "" };
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

/** Pairs the hosted app with a server on this machine. */
export function connect(url: string, token: string) {
  connection.url = url.replace(/\/+$/, "");
  connection.token = token.trim();
  try {
    localStorage.setItem(STORAGE, JSON.stringify(connection));
  } catch {}
}

/** The transport: the browser's fetch, or the fixtures' stand-in server (see `fixtures.ts`). */
type Fetch = (input: string, init?: RequestInit) => Promise<Response>;
let transport: Fetch = (input, init) => fetch(input, init);

/** Swaps the transport. The fixtures mode calls it once, before anything loads; the stores never know. */
export function useTransport(next: Fetch) {
  transport = next;
}

/** Adds the token, and tells the browser a public page means to reach a local address (Local Network Access). */
export function serverFetch(path: string, init?: RequestInit): Promise<Response> {
  const headers = new Headers(init?.headers);
  if (connection.token) headers.set("Authorization", `Bearer ${connection.token}`);
  const remote = connection.url !== "";
  const local = remote && LOOPBACK.has(new URL(connection.url).hostname);
  return transport(`${connection.url}${path}`, { ...init, headers, ...(local ? { targetAddressSpace: "local" } : {}) });
}

/** A refusal from the server, with its code (`fair_play`, `conflict`…) and a sentence for people. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: ErrorCode | "network",
    message: string,
  ) {
    super(message);
  }
}

type Query = Record<string, string | number | boolean | undefined | null>;

function withQuery(path: string, query?: Query): string {
  if (!query) return path;
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(query)) if (v !== undefined && v !== null && v !== "") params.set(k, String(v));
  const text = params.toString();
  return text ? `${path}?${text}` : path;
}

async function request<T>(method: string, path: string, body?: unknown, query?: Query): Promise<T> {
  let res: Response;
  try {
    res = await serverFetch(withQuery(path, query), {
      method,
      headers: body === undefined ? undefined : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(0, "network", "The server cannot be reached. Start it with dameless serve.");
  }
  const text = await res.text();
  const json: unknown = text ? JSON.parse(text) : undefined;
  if (!res.ok) {
    const err = (json as ApiErrorBody | undefined)?.error;
    throw new ApiError(res.status, err?.code ?? "bad_request", err?.message ?? `${res.status} ${res.statusText}`);
  }
  return json as T;
}

/**
 * The API, one method per HTTP verb. A thin stand-in for openapi-fetch: once `schema.d.ts` is generated, these take
 * its `paths` and the type parameters go away.
 */
export const api = {
  get: <T>(path: string, query?: Query) => request<T>("GET", path, undefined, query),
  post: <T = void>(path: string, body?: unknown) => request<T>("POST", path, body ?? {}),
  put: <T = void>(path: string, body: unknown) => request<T>("PUT", path, body),
  delete: <T = void>(path: string) => request<T>("DELETE", path),
};
