import { reactive } from "vue";
import createClient from "openapi-fetch";
import type { paths } from "./schema";
import type { ErrorBody, ErrorCode } from "./types";

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);
const STORAGE = "chess.server";
const DEFAULT_SERVER = "http://127.0.0.1:7979";

export interface Connection {
  /** The server's URL, or "" for the page's own origin. */
  url: string;
  /** The server's token. The page the server serves carries it too; the hosted app is given it by hand. */
  token: string;
}

/** The token the server puts in the page it serves (`<meta name="chess-token">`), if any. */
function pageToken(): string {
  try {
    return document.querySelector<HTMLMetaElement>('meta[name="chess-token"]')?.content ?? "";
  } catch {
    return "";
  }
}

/**
 * Where the server is. Served by `chess serve` itself (or the dev server, which proxies it), the page and the API
 * share an origin: the server lets its own page in by the browser's `Sec-Fetch-Site`, and the page carries the token
 * for browsers that send none. On chess.dame.gg the paired server is remembered with its token in this browser.
 */
function initialConnection(location: Pick<Location, "hostname">, saved: string | null): Connection {
  if (LOOPBACK.has(location.hostname) || location.hostname.endsWith(".localhost"))
    return { url: "", token: pageToken() };
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

/**
 * openapi-fetch builds a `Request` against a placeholder origin; this hands its path, method, headers and body to
 * `serverFetch`, which knows the server (the page's own, or a paired one) and the transport (fixtures).
 */
async function retarget(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const body = request.method === "GET" || request.method === "HEAD" ? "" : await request.text();
  try {
    return await serverFetch(url.pathname + url.search, {
      method: request.method,
      headers: request.headers,
      body: body || undefined,
      signal: request.signal,
    });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    throw new ApiError(0, "network", "The server cannot be reached. Start it with chess serve.");
  }
}

/** The API, typed by the server's OpenAPI document: `client.GET("/api/games/{id}", { params: { path: { id } } })`. */
export const client = createClient<paths>({ baseUrl: "http://chess.invalid", fetch: retarget });

interface Answer<T> {
  data?: T;
  error?: unknown;
  response: Response;
}

/**
 * The answer's body, or an `ApiError` with the server's code and message. Every store goes through it:
 * `const games = await ok(client.GET("/api/games", { params: { query: { limit: 300 } } }))`.
 */
export async function ok<T>(pending: Promise<Answer<T>>): Promise<T> {
  const { data, error, response } = await pending;
  if (!response.ok) {
    const body = (error as ErrorBody | undefined)?.error;
    throw new ApiError(
      response.status,
      body?.code ?? "bad_request",
      body?.message ?? `${response.status} ${response.statusText}`,
    );
  }
  return data as T;
}
