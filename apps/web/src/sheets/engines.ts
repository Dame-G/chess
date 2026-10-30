import type { Engine, SetupPlan } from "../api/types";
import { bytes } from "../format";

/**
 * UCI options written one per line as `Name=value` (`Threads=4`, `Hash = 256`), read into what `POST /api/engines`
 * takes. Blank lines are skipped; `error` names the first line that is not `Name=value`.
 */
export function readOptions(text: string): { options: Record<string, string>; error?: string } {
  const options: Record<string, string> = {};
  for (const [i, raw] of text.split(/\r?\n/).entries()) {
    const line = raw.trim();
    if (!line) continue;
    const at = line.indexOf("=");
    const name = at > 0 ? line.slice(0, at).trim() : "";
    if (!name) return { options, error: `Line ${i + 1} is not Name=value.` };
    options[name] = line.slice(at + 1).trim();
  }
  return { options };
}

/** What an engine reports in the list: what it calls itself, a Maia's rating, or that it is not installed. */
export function engineDetail(e: Pick<Engine, "installed" | "id_name" | "rating" | "path">): string {
  if (!e.installed) return "Not installed";
  const parts = [e.id_name ?? e.path ?? "", e.rating != null ? `plays at about ${e.rating}` : ""].filter(Boolean);
  return parts.join(" · ");
}

/**
 * Whether an engine can analyse: installed, and not one that plays like a person (a Maia, which has a rating and
 * searches one node). The analysis engine runs reviews, live evaluation, drills and the levels.
 */
export const canAnalyse = (e: Pick<Engine, "installed" | "rating" | "nodes">): boolean =>
  e.installed && e.rating == null && e.nodes !== 1;

/** What setup would download, as one line: "Stockfish 19 and Maia: 182 MB", or that nothing is left to download. */
export function setupLine(plan: SetupPlan): string {
  if (!plan.downloads.length) return "Everything setup installs is here.";
  const names = plan.downloads.map((d) => d.name);
  const list = names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : names[0]!;
  return plan.bytes ? `${list}: ${bytes(plan.bytes)}` : list;
}
