<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { PhTrash } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ETable } from "@eos-ui/vue/components/ETable";
import { EVisuallyHidden } from "@eos-ui/vue/components/EVisuallyHidden";
import type { Dataset, Net } from "../api/types";
import { ago, bytes, shortDate } from "../format";
import { dataLine, datasetLine, positionsText, room, sourceWord } from "../sheets/training";
import { useTraining } from "../stores/training";
import ConfirmDialog from "./ConfirmDialog.vue";
import DatagenDialog from "./DatagenDialog.vue";
import ImportDatasetDialog from "./ImportDatasetDialog.vue";
import Term from "./Term.vue";

/**
 * The brain: the net the champion plays with, the lineage of nets (each with its parent, the data it learned from and
 * the loss it ended at), the datasets with the room they may take, and the ways to make more data: generate it or
 * import it. Training itself is an idea of kind Network.
 */
const training = useTraining();
const toast = useToast();

const failed = ref<string | null>(null);
onMounted(() => training.load().catch((e: unknown) => (failed.value = e instanceof Error ? e.message : String(e))));

const championName = computed(() => training.champion?.name ?? "the network its code ships with");
const championTrained = computed(() => {
  const run = training.champion?.run;
  return run ? `, trained on ${positionsText(run.positions)} positions` : "";
});

const generating = ref(false);
const importing = ref(false);

const data = computed(() => training.data);
const fraction = computed(() =>
  data.value && data.value.cap_bytes ? (data.value.used_bytes / data.value.cap_bytes) * 100 : 0,
);
const free = computed(() =>
  data.value?.disk_free_bytes != null ? `${bytes(data.value.disk_free_bytes)} free on the disk` : "",
);
const left = computed(() => (data.value ? room(data.value) : 0));

const datasetName = (id: number | null | undefined) =>
  id == null ? "" : (training.datasets.find((d) => d.id === id)?.name ?? `dataset ${id}`);

function trainedOn(n: Net): string {
  const parts = [n.dataset != null ? datasetName(n.dataset) : "", n.run ? positionsText(n.run.positions) : ""].filter(
    Boolean,
  );
  return parts.length ? parts.join(" · ") : "—";
}

const toDelete = ref<Dataset | null>(null);
const deleting = ref(false);
const confirmDelete = computed({
  get: () => toDelete.value !== null,
  set: (v: boolean) => {
    if (!v) toDelete.value = null;
  },
});
async function remove() {
  const d = toDelete.value;
  if (!d) return;
  deleting.value = true;
  try {
    await training.remove(d.id);
    toast.show({ title: `Deleted ${d.name}`, message: `${bytes(d.size_bytes)} freed.`, tone: "success" });
    toDelete.value = null;
  } catch (e) {
    toast.show({ title: "It was not deleted", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  } finally {
    deleting.value = false;
  }
}
</script>

<template>
  <section class="sec" aria-labelledby="d-brain">
    <div class="sec-head">
      <h2 id="d-brain" class="part-title">Brain</h2>
      <p class="sec-note">The <Term k="net" text="network" /> Dameless plays with, and the data it learns from.</p>
    </div>
    <ENotice v-if="failed" tone="danger" title="The brain did not load">{{ failed }}</ENotice>
    <div v-else-if="!training.loaded" aria-busy="true"><ESkeleton :lines="4" label="Loading the networks" /></div>
    <template v-else>
      <p class="champ">
        The champion plays <strong class="mono">{{ championName }}</strong
        >{{ championTrained }}.
      </p>

      <div v-if="training.nets.length" class="group">
        <h3 class="part-title">Networks</h3>
        <ETable caption="Networks, newest first" hide-caption dense class="brain-table">
          <thead>
            <tr>
              <th scope="col">Network</th>
              <th scope="col">Trained on</th>
              <th scope="col" class="e-num"><Term k="loss" /></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="n in training.nets" :key="n.id">
              <th scope="row">
                <span class="mono">{{ n.name }}</span>
                <span class="sec-note from" :title="ago(n.created_at)">
                  {{
                    [
                      n.parent != null ? `from ${training.netName(n.parent) ?? `net ${n.parent}`}` : "",
                      shortDate(n.created_at),
                      training.champion?.id === n.id ? "champion" : "",
                    ]
                      .filter(Boolean)
                      .join(" · ")
                  }}
                </span>
              </th>
              <td>{{ trainedOn(n) }}</td>
              <td class="e-num mono">{{ n.run?.final_loss != null ? n.run.final_loss.toFixed(4) : "—" }}</td>
            </tr>
          </tbody>
        </ETable>
      </div>
      <p v-else class="sec-note">No network has been trained here yet. Train one with a New idea of kind Network.</p>

      <div v-if="data" class="data">
        <div class="sec-head">
          <h3 class="part-title">Datasets</h3>
          <p v-if="free" class="sec-note">{{ free }}</p>
        </div>
        <EProgress :value="fraction" label="Data used" :value-text="dataLine(data).replace('Data: ', '')" />
        <ETable v-if="training.datasets.length" caption="Datasets" hide-caption dense class="brain-table">
          <thead>
            <tr>
              <th scope="col">Dataset</th>
              <th scope="col" class="e-num">Positions</th>
              <th scope="col"><EVisuallyHidden>Delete</EVisuallyHidden></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="d in training.datasets" :key="d.id">
              <th scope="row">
                <span class="mono">{{ d.name }}</span>
                <span class="sec-note from">{{ sourceWord(d) }} · {{ d.format }} · {{ bytes(d.size_bytes) }}</span>
              </th>
              <td class="e-num mono">{{ positionsText(d.positions) }}</td>
              <td>
                <EIconButton :icon="PhTrash" :label="`Delete ${d.name}`" tooltip size="sm" @click="toDelete = d" />
              </td>
            </tr>
          </tbody>
        </ETable>
        <p v-else class="sec-note">No data yet. Generate some, or import a shard.</p>
        <div class="sheet-actions">
          <EButton variant="outline" size="sm" :disabled="left <= 0" @click="generating = true">Generate data</EButton>
          <EButton variant="outline" size="sm" :disabled="left <= 0" @click="importing = true">
            Import a dataset
          </EButton>
          <span v-if="left <= 0" class="sec-note">The data folder is full. Delete a dataset first.</span>
        </div>
      </div>
    </template>

    <DatagenDialog v-model:open="generating" />
    <ImportDatasetDialog v-model:open="importing" />
    <ConfirmDialog
      v-model:open="confirmDelete"
      :title="`Delete ${toDelete?.name ?? 'the dataset'}?`"
      :description="
        toDelete ? `${datasetLine(toDelete)}. It cannot be brought back, only made or imported again.` : undefined
      "
      confirm="Delete dataset"
      danger
      :busy="deleting"
      @confirm="remove"
    />
  </section>
</template>

<style scoped>
.champ {
  margin: 0;
}

.data {
  display: grid;
  gap: var(--e-gap-snug);
}

.group {
  display: grid;
  gap: var(--e-gap-tight);
}

/* Cells wrap on a phone instead of widening the table past its frame. */
.brain-table :is(th, td) {
  white-space: normal;
}

.brain-table .e-num {
  white-space: nowrap;
}

.from {
  display: block;
}
</style>
