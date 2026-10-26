import type { Version, VersionBuild } from "../../api/queries";
import { signed } from "../../format";
import type { Status } from "../improve/improve";

/** How a version stands against main, as a layman asks it: did it pass? */
export function verdictStatus(v: Version): Status {
  const verdict = v.verdict;
  if (!verdict) return { label: "Untested", figure: "", tone: "neutral" };
  const e = verdict.estimate;
  const figure = e ? `${signed(e.elo, 1)} ± ${e.elo_error.toFixed(1)} Elo` : "";
  switch (verdict.status) {
    case "queued":
      return { label: "Queued", figure: "", tone: "neutral" };
    case "running":
      return { label: "Running", figure, tone: "info" };
    case "cancelled":
      return { label: "Stopped", figure, tone: "neutral" };
    case "passed":
      return { label: "Passed", figure, tone: "success" };
    default:
      return { label: "Rejected", figure, tone: "danger" };
  }
}

/** The tune's progress as a badge: "Tuning 240/1000", or what became of it. */
export function tuneBadge(v: Pick<Version, "tune">): string {
  const t = v.tune;
  if (!t) return "";
  switch (t.status) {
    case "queued":
      return "Tune queued";
    case "running":
      return `Tuning ${t.iteration}/${t.iterations}`;
    case "done":
      return "Tuned";
    case "failed":
      return "Tune failed";
    default:
      return `Tune stopped at ${t.iteration}/${t.iterations}`;
  }
}

/** Whether the version can be removed at all: the fork's own branches, never main. */
export const deletable = (v: Pick<Version, "is_main" | "upstream">) => !v.is_main && !v.upstream;

/** Its builds that still have a binary. */
export const liveBuilds = (v: Pick<Version, "builds">): VersionBuild[] => v.builds.filter((b) => !b.deleted_at);

/** Fork branches first (`main` leading), then upstream's, newest commit first inside each. */
export function sortVersions(list: readonly Version[]): Version[] {
  return list.toSorted(
    (a, b) =>
      Number(a.upstream) - Number(b.upstream) || Number(b.is_main) - Number(a.is_main) || b.date.localeCompare(a.date),
  );
}

/** What the library shows: the fork's versions; hidden ones and upstream's only when asked (or open); what matches the search. */
export function libraryOf(list: readonly Version[], query: string, showHidden: boolean, open = ""): Version[] {
  const q = query.trim().toLowerCase();
  return sortVersions(list).filter(
    (v) =>
      (showHidden || (!v.hidden && !v.upstream) || v.name === open) &&
      (!q || v.name.toLowerCase().includes(q) || v.subject.toLowerCase().includes(q)),
  );
}

/** The version that a path names: its branch, or (an old address) the one that has the build of that number. */
export function resolveVersion(list: readonly Version[], param: string): Version | undefined {
  const named = list.find((v) => v.name === param);
  if (named) return named;
  const id = /^\d+$/.test(param) ? Number(param) : 0;
  if (!id) return undefined;
  const owners = list.filter((v) => v.builds.some((b) => b.id === id));
  return owners.find((v) => !v.upstream) ?? owners[0];
}

/** Whether a build's own bench agrees with its commit's `Bench:` line. */
export type Check = "match" | "differs" | "unchecked";
export const checkOf = (b: Pick<VersionBuild, "bench_ok">): Check =>
  b.bench_ok == null ? "unchecked" : b.bench_ok ? "match" : "differs";

/** "412 MB", "1.3 GB". */
export function bytesText(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} kB`;
}
