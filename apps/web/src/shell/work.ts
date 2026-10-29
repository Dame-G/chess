import type { Work, WorkKind } from "../api/types";
import { formatNumber } from "../format";

const KIND: Record<WorkKind, string> = {
  build: "Building",
  trial: "Trial",
  tune: "Tune",
  review: "Review",
  import: "Import",
  promote: "Promoting",
  calibrate: "Calibrating",
  datagen: "Data",
  train: "Training",
  dataset_import: "Download",
  upstream: "Upstream",
};

/** What a work item is doing in a few words, for the header: "Trial 412 pairs", "Review 2 of 20 games", "Building". */
export function shortWork(w: Pick<Work, "kind" | "progress" | "status">): string {
  const what = KIND[w.kind];
  const p = w.progress;
  if (!p) return what;
  const count = p.total ? `${formatNumber(p.done)} of ${formatNumber(p.total)}` : formatNumber(p.done);
  return `${what} ${count} ${p.unit}${w.status === "paused" ? " paused" : ""}`;
}
