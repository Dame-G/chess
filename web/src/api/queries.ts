import { useQuery } from "@pinia/colada";
import { api } from "./runner";
import type { components } from "./schema";

export type Job = components["schemas"]["Job"];

/** Unwraps an openapi-fetch result: the data, or an error with the runner's message. */
export async function unwrap<T>(call: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<T> {
  const { data, error, response } = await call;
  if (data !== undefined) return data;
  const message = (error as { error?: string } | undefined)?.error;
  throw new Error(message ?? `${response.status} ${response.statusText}`);
}

export const useBuilds = () => useQuery({ key: ["builds"], query: () => unwrap(api.GET("/api/builds")) });

export const useJobs = () =>
  useQuery({ key: ["jobs"], query: () => unwrap(api.GET("/api/jobs", { params: { query: { limit: 20 } } })) });
