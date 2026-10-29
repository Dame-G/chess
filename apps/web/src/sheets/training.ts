import type { DataStatus, Dataset, TrainSchedule, TrainSpec, TrainStep } from "../api/types";
import { aboutDuration, bytes, duration } from "../format";

/**
 * The numbers behind the Brain section: what generating data costs, what an import needs on disk, what a training
 * run takes, and the loss curve of a run. The figures are the R4 contract's: datagen makes about 4 to 5 million
 * positions an hour on the default threads and writes about 4.3 bytes a position; an import needs about 3.15 times
 * its download at the peak and keeps 2.15 times; the GPU trained at about 760k positions a second in the smoke run.
 */

/** Positions per hour on the default number of threads, low and high. */
const DATAGEN_PER_HOUR: [number, number] = [4_000_000, 5_000_000];
/** Bytes a position takes in viriformat. */
const BYTES_PER_POSITION = 4.3;
/** What an import needs at its peak and keeps, as multiples of its download. */
const IMPORT_PEAK = 3.15;
const IMPORT_KEPT = 2.15;
/** Positions per second the GPU trains at, as the smoke run measured it. */
const TRAIN_POSITIONS_PER_SECOND = 760_000;
/** Bytes of one bullet checkpoint, as the executor reserves them. */
const CHECKPOINT_BYTES = 720_000_000;

export interface DatagenEstimate {
  bytes: number;
  /** Seconds, at the faster and the slower of the quoted rates. */
  seconds: [number, number];
}

/**
 * What generating `positions` costs. `threads` scales the rate from the default (`defaultThreads`, half the machine's
 * cores), which the quoted figures are for. A hundred percent scaling is optimistic past the physical cores, so this is
 * a guide in words, not a promise.
 */
export function datagenEstimate(positions: number, threads: number, defaultThreads: number): DatagenEstimate {
  const scale = Math.max(1, threads) / Math.max(1, defaultThreads);
  const [low, high] = DATAGEN_PER_HOUR;
  return {
    bytes: Math.round(positions * BYTES_PER_POSITION),
    seconds: [(positions / (high * scale)) * 3600, (positions / (low * scale)) * 3600],
  };
}

/** `about 20 to 25 minutes`, `about 4 hours`. */
function rangeText(seconds: [number, number]): string {
  const [a, b] = seconds.map((s) => duration(s)) as [string, string];
  return a === b ? `about ${a}` : `about ${a} to ${b}`;
}

/** The sentence under the Generate dialog's fields. */
export function datagenText(e: DatagenEstimate): string {
  return `${bytes(e.bytes)} on disk, ${rangeText(e.seconds)} on this machine, in the background.`;
}

/** The same cost in one short line for a dialog's footer: `~86 MB · 4 to 5 h`. */
export function datagenSummary(e: DatagenEstimate): string {
  const [a, b] = e.seconds.map((s) => duration(s)) as [string, string];
  return `~${bytes(e.bytes)} · ${a === b ? a : `${a} to ${b}`}`;
}

/** What the data folder can still take: under the cap, and leaving the disk its floor. `null` when the disk is unknown. */
export function room(d: Pick<DataStatus, "cap_bytes" | "used_bytes" | "disk_free_bytes" | "keep_free_bytes">): number {
  const underCap = Math.max(0, d.cap_bytes - d.used_bytes);
  const onDisk = d.disk_free_bytes == null ? Infinity : Math.max(0, d.disk_free_bytes - d.keep_free_bytes);
  return Math.min(underCap, onDisk);
}

/** `Data: 1.2 of 10 GB`, with the disk's room when it is the tighter limit. */
export function dataLine(d: Pick<DataStatus, "cap_bytes" | "used_bytes">): string {
  const used = d.used_bytes / 1e9;
  const cap = d.cap_bytes / 1e9;
  const fmt = (n: number) => (n >= 100 || Number.isInteger(n) ? n.toFixed(0) : n.toFixed(1));
  return `Data: ${fmt(used)} of ${fmt(cap)} GB`;
}

/** Why an amount of new data does not fit, or `null` when it does. */
export function whyNoRoom(need: number, d: DataStatus): string | null {
  if (need <= room(d)) return null;
  const underCap = Math.max(0, d.cap_bytes - d.used_bytes);
  return need > underCap
    ? `That is more than the ${bytes(underCap)} left under the ${bytes(d.cap_bytes)} cap. Delete a dataset, or raise the cap in the server's settings.`
    : `The disk would be left with under ${bytes(d.keep_free_bytes)} free. Free some space first.`;
}

export interface ImportNeed {
  /** The most it holds on disk at once, while it converts. */
  peak: number;
  /** What stays when it is done. */
  kept: number;
}

export const importNeed = (downloadBytes: number): ImportNeed => ({
  peak: Math.round(downloadBytes * IMPORT_PEAK),
  kept: Math.round(downloadBytes * IMPORT_KEPT),
});

/**
 * The sentence the import must state before it can be confirmed: the size, where it goes, and what it needs. The
 * route refuses without `confirm: true`; the dialog turns that into this explicit go-ahead.
 */
export function importConfirmText(name: string, downloadBytes: number): string {
  const n = importNeed(downloadBytes);
  return `Download ${bytes(downloadBytes)} into your data folder (data/${name || "name"}). It needs up to ${bytes(n.peak)} while it converts, and keeps about ${bytes(n.kept)}.`;
}

/** The dataset name the data folder can hold: letters, digits, dashes and underscores. */
export const validName = (name: string): boolean => /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(name);

/** A name from the last part of a URL: `https://x/y/shard-03.binpack.zst` is `shard-03`. */
export function nameFromUrl(url: string): string {
  const last = url.split("?")[0]!.split("/").filter(Boolean).pop() ?? "";
  return last
    .replace(/\.(binpack|bin|vf|zst|gz|pgn)$/gi, "")
    .replace(/\.(binpack|bin|vf)$/gi, "")
    .replace(/[^A-Za-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

// ---- Training

export type Preset = "quick" | "long";

/** The schedules the dialog offers by name; the numbers sit under Advanced. */
export const SCHEDULES: Record<Preset, { label: string; schedule: TrainSchedule }> = {
  quick: {
    label: "Quick fine-tune",
    schedule: {
      superbatches: 20,
      batches_per_superbatch: 200,
      batch_size: 16_384,
      lr: 1e-4,
      final_lr: 1e-6,
      lr_schedule: "cosine",
      wdl: 0.3,
      final_wdl: null,
    },
  },
  long: {
    label: "Long fine-tune",
    schedule: {
      superbatches: 40,
      batches_per_superbatch: 6_104,
      batch_size: 16_384,
      lr: 1e-4,
      final_lr: 1e-6,
      lr_schedule: "cosine",
      wdl: 0.3,
      final_wdl: null,
    },
  },
};

/** Positions a run trains on, counting repeats. */
const positionsOf = (s: Pick<TrainSchedule, "superbatches" | "batches_per_superbatch" | "batch_size">): number =>
  s.superbatches * s.batches_per_superbatch * s.batch_size;

export interface TrainCost {
  positions: number;
  seconds: number;
  /** Disk for the checkpoints kept: `(keep + 1)` of them. */
  checkpointBytes: number;
}

export function trainCost(s: TrainSchedule, keepCheckpoints = 2): TrainCost {
  const positions = positionsOf(s);
  return {
    positions,
    seconds: positions / TRAIN_POSITIONS_PER_SECOND,
    checkpointBytes: (keepCheckpoints + 1) * CHECKPOINT_BYTES,
  };
}

/** `4.0 billion`, `65.5 million`. */
export function positionsText(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} billion`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e8 ? 0 : 1)} million`;
  return n.toLocaleString("en-GB");
}

/** The cost of a run in words. */
export function trainCostText(c: TrainCost): string {
  return `${positionsText(c.positions)} positions, ${aboutDuration(c.seconds)} on this GPU, and ${bytes(c.checkpointBytes)} of disk for the checkpoints while it runs.`;
}

/** Which preset a schedule is, or `null` when its numbers were changed. */
export function presetOf(s: TrainSchedule): Preset | null {
  for (const [key, p] of Object.entries(SCHEDULES) as [Preset, (typeof SCHEDULES)[Preset]][]) {
    const q = p.schedule;
    if (
      q.superbatches === s.superbatches &&
      q.batches_per_superbatch === s.batches_per_superbatch &&
      q.batch_size === s.batch_size &&
      q.lr === s.lr &&
      q.final_lr === s.final_lr &&
      q.wdl === s.wdl
    )
      return key;
  }
  return null;
}

/** A train spec as the idea dialog sends it. */
export function trainSpec(over: { datasets: number[]; parent: number | null; schedule: TrainSchedule }): TrainSpec {
  return { datasets: over.datasets, parent: over.parent, from_scratch: false, schedule: over.schedule };
}

// ---- The loss curve

export interface LossCurve {
  /** Superbatches completed, fractional within one. */
  x: number[];
  loss: number[];
}

/** The most points a loss chart draws; a long run keeps every k-th step and always the last. */
const MAX_POINTS = 400;

/** Where in the run a step is, in superbatches. */
export const stepPosition = (s: Pick<TrainStep, "superbatch" | "batch">, batchesPer: number): number =>
  s.superbatch - 1 + s.batch / Math.max(1, batchesPer);

/**
 * The run's loss against its progress, thinned to at most `MAX_POINTS` for the chart. Steps of a resumed run come
 * in order; a step that goes back (a resume that redid some batches) replaces what it overlaps.
 */
export function lossCurve(steps: readonly TrainStep[], batchesPer: number, limit = MAX_POINTS): LossCurve {
  const ordered: TrainStep[] = [];
  for (const s of steps) {
    const at = stepPosition(s, batchesPer);
    while (ordered.length && stepPosition(ordered[ordered.length - 1]!, batchesPer) >= at) ordered.pop();
    ordered.push(s);
  }
  const every = Math.max(1, Math.ceil(ordered.length / limit));
  const kept = ordered.filter((_, i) => i % every === 0 || i === ordered.length - 1);
  return { x: kept.map((s) => stepPosition(s, batchesPer)), loss: kept.map((s) => s.loss) };
}

/** Where the checkpoints are written, in words: "every 10 superbatches, the last 2 kept". */
export const checkpointText = (spec: Pick<TrainSpec, "save_rate" | "keep_checkpoints">): string => {
  const rate = spec.save_rate ?? 10;
  const keep = spec.keep_checkpoints ?? 2;
  return `every ${rate} superbatches, the last ${keep} kept`;
};

/** A dataset in a list: its positions and size. */
export const datasetLine = (d: Dataset): string => `${positionsText(d.positions)} positions · ${bytes(d.size_bytes)}`;

/** How a dataset came to be, in a word. */
export function sourceWord(d: Dataset): string {
  switch (d.source.type) {
    case "datagen":
      return "Generated";
    case "import":
      return "Imported";
    case "trials":
      return "From trial games";
    default:
      return "Other";
  }
}

/** Throughput as the run reports it: `760k positions/s`. */
export const rateText = (perSecond: number): string =>
  perSecond >= 1e6 ? `${(perSecond / 1e6).toFixed(2)}M positions/s` : `${Math.round(perSecond / 1000)}k positions/s`;
