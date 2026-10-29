<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { PhTrash } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import type { Dataset, Net } from "../api/types";
import { ago, bytes, formatDate } from "../format";
import { dataLine, datasetLine, positionsText, room, sourceWord } from "../sheets/training";
import { useTraining } from "../stores/training";
import { useConfirm } from "../ui/useConfirm";
import UiBadge from "../ui/UiBadge.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiMeter from "../ui/UiMeter.vue";
import UiSection from "../ui/UiSection.vue";
import UiTable from "../ui/UiTable.vue";
import DatagenDialog from "./DatagenDialog.vue";
import ImportDatasetDialog from "./ImportDatasetDialog.vue";
import Term from "./Term.vue";

/**
 * The brain: the net the champion plays with, the lineage of nets (each with its parent, the data it learned from and
 * the loss it ended at), the datasets with the room they may take, and the ways to make more data: generate it or
 * import it. Training itself is an idea of kind Net.
 */
const training = useTraining();
const toast = useToast();
const confirm = useConfirm();

const failed = ref<string | null>(null);
async function read() {
  failed.value = null;
  try {
    await training.load();
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
}
onMounted(read);

const championName = computed(() => training.champion?.name ?? "the network its code ships with");
const championTrained = computed(() => {
  const run = training.champion?.run;
  return run ? `, trained on ${positionsText(run.positions)} positions` : "";
});

const generating = ref(false);
const importing = ref(false);

const data = computed(() => training.data);
const left = computed(() => (data.value ? room(data.value) : 0));
const full = computed(() => !!data.value && (left.value <= 0 || data.value.used_bytes / data.value.cap_bytes > 0.9));
const free = computed(() =>
  data.value?.disk_free_bytes != null ? `${bytes(data.value.disk_free_bytes)} free on the disk` : "",
);

const datasetName = (id: number | null | undefined) =>
  id == null ? "" : (training.datasets.find((d) => d.id === id)?.name ?? `dataset ${id}`);

function trainedOn(n: Net): string {
  const parts = [n.dataset != null ? datasetName(n.dataset) : "", n.run ? positionsText(n.run.positions) : ""].filter(
    Boolean,
  );
  return parts.length ? parts.join(" · ") : "–";
}
const netFrom = (n: Net) =>
  [n.parent != null ? `from ${training.netName(n.parent) ?? `net ${n.parent}`}` : "", formatDate(n.created_at)]
    .filter(Boolean)
    .join(" · ");

const netColumns = [
  { key: "name", label: "Network" },
  { key: "trained", label: "Trained on", hide: "sm" as const },
  { key: "loss", label: "Loss", fig: true },
];
const datasetColumns = [
  { key: "name", label: "Dataset" },
  { key: "positions", label: "Positions", fig: true },
  { key: "size", label: "Size", fig: true, hide: "sm" as const },
  { key: "delete", label: "", width: "2.5rem" },
];

async function remove(d: Dataset) {
  const yes = await confirm({
    title: `Delete ${d.name}?`,
    body: `${datasetLine(d)}. It cannot be brought back, only made or imported again.`,
    confirmLabel: "Delete dataset",
    danger: true,
  });
  if (!yes) return;
  try {
    await training.remove(d.id);
    toast.show({ title: `Deleted ${d.name}`, message: `${bytes(d.size_bytes)} freed.`, tone: "success" });
  } catch (e) {
    toast.show({ title: "It was not deleted", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  }
}
</script>

<template>
  <div class="brain">
    <UiEmpty v-if="failed" tone="error" title="The brain did not load" :body="failed">
      <template #actions><EButton variant="outline" @click="read">Try again</EButton></template>
    </UiEmpty>
    <template v-else>
      <p class="champ">
        The champion plays <strong class="mono">{{ championName }}</strong
        >{{ championTrained }}.
        <span class="hint">A <Term k="net" text="network" /> is what it learned from the data below.</span>
      </p>

      <UiSection title="Networks">
        <UiTable
          label="Networks, newest first"
          :columns="netColumns"
          :rows="training.nets"
          row-key="id"
          :loading="!training.loaded"
          empty="No network has been trained here yet. Train one with a new idea of kind Net."
        >
          <template #head-loss><Term k="loss" /></template>
          <template #cell-name="{ row }">
            <span class="mono">{{ row.name }}</span>
            <UiBadge v-if="training.champion?.id === row.id" tone="act">Champion</UiBadge>
            <span class="sub" :title="ago(row.created_at)">{{ netFrom(row) }}</span>
          </template>
          <template #cell-trained="{ row }">{{ trainedOn(row) }}</template>
          <template #cell-loss="{ row }">
            {{ row.run?.final_loss != null ? row.run.final_loss.toFixed(4) : "–" }}
          </template>
        </UiTable>
      </UiSection>

      <UiSection title="Datasets">
        <template #aside>
          <EButton variant="outline" size="sm" :disabled="!data || left <= 0" @click="generating = true">
            Generate data
          </EButton>
          <EButton variant="outline" size="sm" :disabled="!data || left <= 0" @click="importing = true">
            Import a dataset
          </EButton>
        </template>
        <div v-if="data" class="meter">
          <UiMeter
            label="Data used"
            :value="data.used_bytes"
            :max="data.cap_bytes"
            :text="dataLine(data).replace('Data: ', '')"
            :tone="full ? 'warn' : undefined"
          />
          <p v-if="left <= 0" class="hint">The data folder is full. Delete a dataset first.</p>
          <p v-else-if="free" class="hint">{{ free }}</p>
        </div>
        <UiTable
          label="Datasets"
          :columns="datasetColumns"
          :rows="training.datasets"
          row-key="id"
          :loading="!training.loaded"
          empty="No data yet. Generate some, or import a shard."
        >
          <template #cell-name="{ row }">
            <span class="mono">{{ row.name }}</span>
            <span class="sub">{{ sourceWord(row) }} · {{ row.format }}</span>
          </template>
          <template #cell-positions="{ row }">{{ positionsText(row.positions) }}</template>
          <template #cell-size="{ row }">{{ bytes(row.size_bytes) }}</template>
          <template #head-delete><span class="sr-only">Delete</span></template>
          <template #cell-delete="{ row }">
            <EIconButton :icon="PhTrash" :label="`Delete ${row.name}`" tooltip size="sm" @click="remove(row)" />
          </template>
        </UiTable>
      </UiSection>
    </template>

    <DatagenDialog v-model:open="generating" />
    <ImportDatasetDialog v-model:open="importing" />
  </div>
</template>

<style scoped>
.brain {
  min-width: 0;
}

.champ {
  margin: 0 0 var(--sp-3);
}

.hint {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.meter {
  display: grid;
  gap: var(--sp-1);
  margin-block-end: var(--sp-3);
}

.sub {
  display: block;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
