import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { client, ok } from "../api/client";
import type {
  DataStatus,
  DatagenRequest,
  Dataset,
  DatasetId,
  IdeaId,
  ImportRequest,
  Net,
  NetId,
  TrainView,
} from "../api/types";
import { listen, reloadOnReconnect } from "./listen";
import { useVersions } from "./versions";

/**
 * The brain: the nets Dameless can play with (a lineage, each with its parent and the data it trained on), the
 * datasets that feed training and the room they may take, and each net idea's training run: its loss, as it was
 * stored and as `train.progress` extends it. Loaded when the Dameless sheet or a net idea opens.
 */
export const useTraining = defineStore("training", () => {
  const versions = useVersions();
  const nets = ref<Net[]>([]);
  const data = ref<DataStatus | null>(null);
  const runs = ref<Record<IdeaId, TrainView>>({});
  const loaded = ref(false);
  let wanted = false;

  const datasets = computed(() => data.value?.datasets ?? []);
  const byId = computed(() => new Map(nets.value.map((n) => [n.id, n])));
  /** The net the engine's code ships with: the oldest one that has no parent and no training run. */
  const shipped = computed<Net | null>(
    () => [...nets.value].filter((n) => n.parent == null && !n.run).sort((a, b) => a.id - b.id)[0] ?? null,
  );
  /** The net the champion plays with: the one it was crowned with, else the one its code ships. */
  const champion = computed<Net | null>(() => {
    const id = versions.current?.net;
    return id == null ? shipped.value : (byId.value.get(id) ?? null);
  });

  async function load() {
    wanted = true;
    const [n, d] = await Promise.all([ok(client.GET("/api/nets")), ok(client.GET("/api/datasets"))]);
    nets.value = n;
    data.value = d;
    loaded.value = true;
  }

  async function loadData() {
    data.value = await ok(client.GET("/api/datasets"));
  }

  /** An idea's training: its spec, the steps so far and the net it made. */
  async function loadRun(idea: IdeaId) {
    runs.value[idea] = await ok(client.GET("/api/ideas/{id}/train", { params: { path: { id: idea } } }));
  }

  const generate = (req: DatagenRequest) => ok(client.POST("/api/datasets/datagen", { body: req }));
  /** Downloads and converts a dataset. The server refuses without `confirm: true`; the dialog has asked first. */
  const importDataset = (req: ImportRequest) => ok(client.POST("/api/datasets/import", { body: req }));

  async function remove(id: DatasetId): Promise<Dataset> {
    const gone = await ok(client.DELETE("/api/datasets/{id}", { params: { path: { id } }, body: { confirm: true } }));
    if (data.value) {
      data.value.datasets = data.value.datasets.filter((d) => d.id !== id);
      data.value.used_bytes = Math.max(0, data.value.used_bytes - gone.size_bytes);
    }
    return gone;
  }

  function upsertNet(n: Net) {
    const i = nets.value.findIndex((x) => x.id === n.id);
    if (i >= 0) nets.value[i] = n;
    else nets.value.unshift(n);
    for (const run of Object.values(runs.value)) if (run.net?.id === n.id) run.net = n;
  }

  listen("net.upserted", upsertNet);
  listen("dataset.upserted", (d) => {
    if (!data.value) return;
    const list = data.value.datasets;
    const i = list.findIndex((x) => x.id === d.id);
    if (i >= 0) list[i] = d;
    else list.unshift(d);
    data.value.used_bytes = list.reduce((sum, x) => sum + x.size_bytes, 0);
  });
  listen("train.progress", (p) => {
    if (p.idea == null) return;
    const run = runs.value[p.idea];
    if (!run) return;
    run.steps.push({
      seq: (run.steps.at(-1)?.seq ?? 0) + 1,
      superbatch: p.superbatch,
      batch: p.batch,
      loss: p.loss,
      lr: p.lr,
      grad_norm: p.grad_norm ?? null,
      positions_per_sec: p.positions_per_sec,
      elapsed_seconds: p.elapsed_seconds,
    });
  });
  // A run that ended, or a generation or import that finished, changed the disk and the nets.
  listen("work.upserted", (w) => {
    if (!wanted || w.status !== "done") return;
    if (w.kind === "datagen" || w.kind === "dataset_import") void loadData().catch(() => {});
  });
  reloadOnReconnect(async () => {
    if (wanted) await load();
  });

  const netName = (id: NetId | null | undefined) => (id == null ? undefined : byId.value.get(id)?.name);

  return {
    nets,
    data,
    datasets,
    runs,
    loaded,
    champion,
    load,
    loadData,
    loadRun,
    generate,
    importDataset,
    remove,
    netName,
  };
});
