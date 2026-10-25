import { useQuery } from "@pinia/colada";
import type { MaybeRefOrGetter } from "vue";
import { toValue } from "vue";
import { api } from "./runner";
import type { components } from "./schema";

export type Job = components["schemas"]["Job"];
export type Build = components["schemas"]["Build"];
export type Experiment = components["schemas"]["Experiment"];
export type Test = components["schemas"]["Test"];

/** Unwraps an openapi-fetch result: the data, or an error with the runner's message. */
export async function unwrap<T>(call: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<T> {
  const { data, error, response } = await call;
  if (data !== undefined) return data;
  const message = (error as { error?: string } | undefined)?.error;
  throw new Error(message ?? `${response.status} ${response.statusText}`);
}

export const useStatus = () =>
  useQuery({
    key: ["status"],
    query: async () => {
      const { data, response } = await api.GET("/api/status");
      if (!data) throw new Error(response.status === 401 ? "token" : `status ${response.status}`);
      return data;
    },
    // A runner that is not there yet is checked again, so starting one connects the page without a reload.
    refetchOnWindowFocus: true,
  });

export const useBuilds = () => useQuery({ key: ["builds"], query: () => unwrap(api.GET("/api/builds")) });

export const useBranches = () => useQuery({ key: ["branches"], query: () => unwrap(api.GET("/api/branches")) });

export const useJobs = () =>
  useQuery({ key: ["jobs"], query: () => unwrap(api.GET("/api/jobs", { params: { query: { limit: 20 } } })) });

export const useExperiments = () =>
  useQuery({
    key: ["experiments"],
    query: () => unwrap(api.GET("/api/experiments", { params: { query: { limit: 200 } } })),
  });

export const useExperiment = (id: MaybeRefOrGetter<number>) =>
  useQuery({
    key: () => ["experiment", toValue(id)],
    enabled: () => toValue(id) > 0,
    query: () => unwrap(api.GET("/api/experiments/{id}", { params: { path: { id: toValue(id) } } })),
  });

export const useTest = (id: MaybeRefOrGetter<number>) =>
  useQuery({
    key: () => ["test", toValue(id)],
    enabled: () => toValue(id) > 0,
    query: () => unwrap(api.GET("/api/tests/{id}", { params: { path: { id: toValue(id) } } })),
  });

export const useGames = (id: MaybeRefOrGetter<number>) =>
  useQuery({
    key: () => ["games", toValue(id)],
    enabled: () => toValue(id) > 0,
    query: () => unwrap(api.GET("/api/tests/{id}/games", { params: { path: { id: toValue(id) } } })),
  });

/** The engine to use when none is chosen: the newest build of `main`, else the newest build. */
export function defaultBuild(builds: readonly Build[] | undefined): Build | undefined {
  return builds?.find((b) => b.ref === "main") ?? builds?.[0];
}
