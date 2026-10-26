import { useQuery } from "@pinia/colada";
import type { MaybeRefOrGetter } from "vue";
import { toValue } from "vue";
import { api } from "./runner";
import type { components } from "./schema";

export type Job = components["schemas"]["Job"];
export type Branch = components["schemas"]["Branch"];
export type Build = components["schemas"]["Build"];
export type Version = components["schemas"]["Version"];
export type VersionBuild = components["schemas"]["VersionBuild"];
export type Experiment = components["schemas"]["Experiment"];
export type Test = components["schemas"]["Test"];
export type TestDetail = components["schemas"]["TestDetail"];

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

/** One entry per branch with its builds, verdict, tune and bot state. Build, test and tune events keep it current. */
export const useVersions = () => useQuery({ key: ["versions"], query: () => unwrap(api.GET("/api/versions")) });

/** The address of a version. A name with a `/` stays one path segment. */
export const versionPath = (name: string) => `/versions/${encodeURIComponent(name)}`;

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

/** The engine to use when none is chosen: the newest build of `main`, else the newest build. A deleted build cannot play. */
export function defaultBuild(builds: readonly Build[] | undefined): Build | undefined {
  const live = builds?.filter((b) => !b.deleted_at);
  return live?.find((b) => b.ref === "main") ?? live?.[0];
}

/** Recent matches, newest first, without their moves. The app refreshes them as matches start and end. */
export const useMatches = () =>
  useQuery({
    key: ["matches"],
    query: () => unwrap(api.GET("/api/matches", { params: { query: { limit: 30 } } })),
  });

/** The Lichess bot: its account, whether it is online, the games it is playing and its settings. Events keep it current. */
export const useLichess = () =>
  useQuery({ key: ["lichess"], query: () => unwrap(api.GET("/api/lichess")), refetchOnWindowFocus: true });

/** The bot's recent Lichess games, newest first. Shares the `matches` key prefix, so match events refresh it. */
export const useLichessGames = () =>
  useQuery({
    key: ["matches", "lichess"],
    query: () => unwrap(api.GET("/api/matches", { params: { query: { limit: 30, source: "lichess" } } })),
  });

/** Bots online on Lichess now, when asked for (the bot has to be online to ask). */
export const useLichessBots = (enabled: MaybeRefOrGetter<boolean>) =>
  useQuery({
    key: ["lichess", "bots"],
    enabled: () => toValue(enabled),
    staleTime: 30_000,
    query: () => unwrap(api.GET("/api/lichess/bots")),
  });

export type Tune = components["schemas"]["Tune"];
export type TuneDetail = components["schemas"]["TuneDetail"];
export type Param = components["schemas"]["Param"];

/** SPSA tunes, newest first. Tune events keep it current. */
export const useTunes = () => useQuery({ key: ["tunes"], query: () => unwrap(api.GET("/api/tunes")) });

/** One tune with every finished iteration. `tune.progress` events append to it as the tune runs. */
export const useTune = (id: MaybeRefOrGetter<number>) =>
  useQuery({
    key: () => ["tune", toValue(id)],
    enabled: () => toValue(id) > 0,
    query: () => unwrap(api.GET("/api/tunes/{id}", { params: { path: { id: toValue(id) } } })),
  });

/** What a build declares to tune. Only a build made with the `spsa` feature has any. */
export const useTunables = (build: MaybeRefOrGetter<number>) =>
  useQuery({
    key: () => ["tunables", toValue(build)],
    enabled: () => toValue(build) > 0,
    staleTime: 60_000,
    query: () => unwrap(api.GET("/api/builds/{id}/tunables", { params: { path: { id: toValue(build) } } })),
  });
