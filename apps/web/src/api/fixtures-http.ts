/**
 * How the fixtures answer: JSON, the server's error shape, and `reply`, which types each answer by the route's
 * success response in the OpenAPI document, so the stand-in server cannot drift from the real one's shapes.
 */
import type { paths } from "./schema";

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), { status, headers: { "Content-Type": "application/json" } });
}

export function error(status: number, code: string, message: string): Response {
  return json({ error: { code, message } }, status);
}

type Method = "get" | "post" | "put" | "delete";
type JsonOf<R> = R extends { content: { "application/json": infer B } } ? B : never;

/** What a route answers when it works, read from the server's OpenAPI document. */
export type Answer<P extends keyof paths, M extends Method> = paths[P][M] extends { responses: infer R }
  ? JsonOf<R[keyof R & (200 | 201 | 202)]>
  : never;

/** A route's answer, typed by the schema. */
export function reply<P extends keyof paths, M extends Method>(
  _route: P,
  _method: M,
  data: Answer<P, M>,
  status = 200,
): Response {
  return json(data, status);
}
