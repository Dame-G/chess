import type { Work } from "../api/types";
import { formatNumber } from "../format";

/** The work the app runs, in a word each; any other kind is named by its own word. */
const KIND: Record<string, string> = {
  review: "Review",
  import: "Import",
  calibrate: "Calibrating",
};

/** What a work item is doing in a few words, for the header: "Review 2 of 20 games", "Import 40 games". */
export function shortWork(w: Pick<Work, "kind" | "progress" | "status">): string {
  const what = KIND[w.kind] ?? w.kind[0]!.toUpperCase() + w.kind.slice(1);
  const p = w.progress;
  if (!p) return what;
  const count = p.total ? `${formatNumber(p.done)} of ${formatNumber(p.total)}` : formatNumber(p.done);
  return `${what} ${count} ${p.unit}${w.status === "paused" ? " paused" : ""}`;
}
