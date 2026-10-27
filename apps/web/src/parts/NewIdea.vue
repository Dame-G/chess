<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECombobox } from "@eos-ui/vue/components/ECombobox";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { EStack } from "@eos-ui/vue/components/EStack";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import type { LrSchedule, NewIdeaRequest, Preset, Tunable } from "../api/types";
import { tuneCost, tuneCostText, tuneSpec } from "../sheets/tune";
import {
  positionsText,
  presetOf,
  SCHEDULES,
  trainCost,
  trainCostText,
  trainSpec,
  type Preset as SchedulePreset,
} from "../sheets/training";
import { useIdeas } from "../stores/ideas";
import { usePolicy } from "../stores/policy";
import { useTraining } from "../stores/training";
import { useVersions } from "../stores/versions";
import Term from "./Term.vue";

/**
 * New idea: one dialog for every kind. A gain should play stronger; a neutral change should play the same, simpler;
 * a speed change should only be faster; a tune moves the engine's constants by many short games; a custom trial is any
 * preset and bounds, or a fixed number of pairs. The branch is picked from the versions the fork has.
 */
const open = defineModel<boolean>("open", { required: true });

const ideas = useIdeas();
const versions = useVersions();
const policy = usePolicy();
const training = useTraining();
const router = useRouter();
const toast = useToast();

type Kind = "gain" | "neutral" | "speed" | "tune" | "net" | "custom";
const KINDS: { value: Kind; label: string; text: string }[] = [
  {
    value: "gain",
    label: "Gain",
    text: "Should play stronger. Tried as a gain: the bench must change, then quick games, then short games.",
  },
  {
    value: "neutral",
    label: "Neutral",
    text: "Should play the same, with simpler code. Tried for no loss, with the same funnel as a gain.",
  },
  {
    value: "speed",
    label: "Speed",
    text: "Should only be faster. The search must not change, then speeds are compared. No games.",
  },
  {
    value: "tune",
    label: "Tune",
    text: "Moves the engine's constants by playing many short games, then confirms the result.",
  },
  {
    value: "net",
    label: "Network",
    text: "Trains a new network on your data, then tries the champion with it: the bench must change, then quick games, then short games.",
  },
  {
    value: "custom",
    label: "Custom trial",
    text: "One trial with the settings you choose, for checks that are not an idea.",
  },
];

const kind = ref<Kind>("gain");
const title = ref("");
const hypothesis = ref("");
const branch = ref("");
const long = ref(false);
const tags = ref("");
const busy = ref(false);

// Tune
const available = ref<Tunable[]>([]);
const chosen = ref<string[]>([]);
const iterations = ref<number | null>(1000);
const pairs = 8;

// Network
const datasetIds = ref<string[]>([]);
const parent = ref("champion");
const preset = ref<SchedulePreset | "custom">("quick");
const schedule = ref({ ...SCHEDULES.quick.schedule });
/** A number of the schedule as a field: it keeps its value when the field is emptied. */
const field = (key: "superbatches" | "batches_per_superbatch" | "batch_size" | "lr" | "final_lr" | "wdl") =>
  computed({
    get: () => schedule.value[key],
    set: (v: number | null) => {
      if (v != null) schedule.value[key] = v;
    },
  });
const superbatches = field("superbatches");
const batchesEach = field("batches_per_superbatch");
const batchSize = field("batch_size");
const learningRate = field("lr");
const finalRate = field("final_lr");
const resultWeight = field("wdl");
const rateFalls = computed({
  get: () => schedule.value.lr_schedule ?? "cosine",
  set: (v: string) => (schedule.value.lr_schedule = v as LrSchedule),
});

// Custom
const gamePreset = ref<Preset>("stc");
const stop = ref<"sprt" | "pairs">("sprt");
const lower = ref<number | null>(0);
const upper = ref<number | null>(3);
const fixedPairs = ref<number | null>(2000);

const branches = computed(() => {
  const seen = new Set<string>();
  for (const v of versions.list) if (v.branch && v.branch !== "main") seen.add(v.branch);
  return [...seen].sort().map((value) => ({ value, label: value }));
});
const kindText = computed(() => KINDS.find((k) => k.value === kind.value)?.text ?? "");
const hasLong = computed(
  () => kind.value === "gain" || kind.value === "neutral" || kind.value === "tune" || kind.value === "net",
);
const cost = computed(() => tuneCost(iterations.value ?? 0, pairs, policy.status?.cores ?? 8));
const tunableOptions = computed(() => available.value.map((t) => ({ value: t.name, label: t.name })));
const datasetOptions = computed(() =>
  training.datasets.map((d) => ({ value: String(d.id), label: `${d.name} · ${positionsText(d.positions)} positions` })),
);
const parentOptions = computed(() => [
  {
    value: "champion",
    label: training.champion ? `The champion's network (${training.champion.name})` : "The champion's network",
  },
  ...training.nets.map((n) => ({ value: String(n.id), label: n.name })),
]);
const schedulePresets = (Object.keys(SCHEDULES) as SchedulePreset[]).map((value) => ({
  value,
  label: SCHEDULES[value].label,
}));
const lrSchedules: { value: LrSchedule; label: string }[] = [
  { value: "cosine", label: "Cosine" },
  { value: "linear", label: "Linear" },
  { value: "step", label: "Step" },
  { value: "constant", label: "Constant" },
];
const trainCostSentence = computed(() => trainCostText(trainCost(schedule.value)));
const noGpu = computed(() => (policy.status?.ledger?.gpus ?? 0) === 0);
watch(preset, (p) => {
  if (p !== "custom") schedule.value = { ...SCHEDULES[p].schedule };
});
/** Changing a number under Advanced leaves the presets: the segment shows what it is now. */
watch(
  schedule,
  (s) => {
    const known = presetOf(s);
    if (known) preset.value = known;
    else if (preset.value !== "custom") preset.value = "custom";
  },
  { deep: true },
);
const presets: { value: Preset; label: string }[] = [
  { value: "vstc", label: "Quick screen, 4+0.04" },
  { value: "stc", label: "Short games, 8+0.08" },
  { value: "ltc", label: "Long games, 40+0.4" },
  { value: "stc_hash", label: "Short games, little hash" },
  { value: "ltc_hash", label: "Long games, little hash" },
  { value: "smp", label: "Long games, 6 threads" },
  { value: "nodes", label: "25,000 nodes a move" },
];

watch(open, (isOpen) => {
  if (isOpen) {
    kind.value = "gain";
    title.value = "";
    hypothesis.value = "";
    branch.value = "";
    long.value = false;
    tags.value = "";
    chosen.value = [];
    datasetIds.value = [];
    parent.value = "champion";
    preset.value = "quick";
    schedule.value = { ...SCHEDULES.quick.schedule };
    gamePreset.value = "stc";
    void training.load().catch(() => {});
  }
});
let typing: ReturnType<typeof setTimeout> | undefined;
watch([kind, branch], () => {
  clearTimeout(typing);
  if (kind.value !== "tune" || !branch.value) return;
  // The branch is typed a letter at a time: ask once it stops.
  typing = setTimeout(async () => {
    try {
      available.value = await ideas.tunables(branch.value);
    } catch {
      available.value = [];
    }
  }, 350);
});

const missing = computed(() => {
  if (!title.value.trim()) return "Give it a title.";
  if (kind.value !== "net" && !branch.value.trim()) return "Pick the branch that holds the change.";
  if (kind.value !== "custom" && !hypothesis.value.trim()) return "Say what it should do and why.";
  return "";
});

async function create() {
  const idea: NewIdeaRequest = {
    title: title.value.trim(),
    hypothesis: hypothesis.value.trim(),
    kind: kind.value === "custom" ? "gain" : kind.value,
    // A network idea tests the champion's code with a new net: no branch needed.
    branch: branch.value.trim(),
    author: "you",
    tags: tags.value
      .split(/[,\s]+/)
      .map((t) => t.trim())
      .filter(Boolean),
    ...(hasLong.value && long.value ? { long: true } : {}),
    ...(kind.value === "tune"
      ? {
          tune: tuneSpec({
            params: chosen.value.map((name) => ({ name })),
            iterations: iterations.value ?? 1000,
            pairs,
          }),
        }
      : {}),
    ...(kind.value === "net"
      ? {
          train: trainSpec({
            datasets: datasetIds.value.map(Number),
            parent: parent.value === "champion" ? null : Number(parent.value),
            schedule: schedule.value,
          }),
        }
      : {}),
    ...(kind.value === "custom"
      ? {
          custom: {
            preset: gamePreset.value,
            ...(stop.value === "sprt"
              ? { bounds: [lower.value ?? 0, upper.value ?? 3] }
              : { pairs: fixedPairs.value ?? 1000 }),
          },
        }
      : {}),
  };
  busy.value = true;
  try {
    const made = await ideas.create(idea);
    open.value = false;
    toast.show({ title: "Idea queued", message: made.title, tone: "success" });
    await router.push(`/dameless/ideas/${made.id}`);
  } catch (e) {
    toast.show({
      title: "The idea was not queued",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <EDialog v-model:open="open" title="New idea" description="A change to try against the champion." size="lg">
    <EStack gap="base">
      <ESegmented v-model="kind" label="Kind" :options="KINDS.map((k) => ({ value: k.value, label: k.label }))" />
      <p class="sec-note">{{ kindText }}</p>

      <ETextField v-model="title" label="Title" placeholder="Deeper LMR at high depth" autocomplete="off" />
      <ETextarea v-model="hypothesis" label="What it should do, and why" :optional="kind === 'custom'" :rows="3" />
      <ECombobox
        v-if="kind !== 'net'"
        v-model="branch"
        label="Branch"
        freeform
        :options="branches"
        placeholder="Pick one, or type a branch of the fork"
        hint="Commit the change on a branch with its Bench: line first."
      />

      <template v-if="kind === 'tune'">
        <ECombobox
          v-model="chosen"
          label="Constants to tune"
          multiple
          :options="tunableOptions"
          :placeholder="available.length ? `All ${available.length} on the branch` : 'Pick the branch first'"
          hint="Leave empty to tune every constant the branch declares."
        />
        <ENumberField v-model="iterations" label="Iterations" :min="100" :max="20000" :step="100" />
        <p class="sec-note">
          Each iteration plays {{ pairs }} game pairs. {{ tuneCostText(cost) }} The result still has to pass a
          <Term k="sprt" text="confirmation" /> before it is kept.
        </p>
      </template>

      <template v-if="kind === 'net'">
        <ENotice v-if="noGpu" tone="warning" title="No GPU found">
          Training needs a GPU the app can use. Check Settings, Health.
        </ENotice>
        <ECombobox
          v-model="datasetIds"
          label="Data"
          multiple
          :options="datasetOptions"
          placeholder="Every dataset in the engine's units"
          hint="Leave empty to train on all the data that is in the engine's own units."
        />
        <ESelect v-model="parent" label="Start from" :options="parentOptions" />
        <ESegmented
          v-model="preset"
          label="Schedule"
          :options="[...schedulePresets, ...(preset === 'custom' ? [{ value: 'custom', label: 'Custom' }] : [])]"
        />
        <p class="sec-note">{{ trainCostSentence }}</p>
        <EDisclosure title="Advanced">
          <EStack gap="base">
            <div class="pair">
              <ENumberField v-model="superbatches" label="Superbatches" :min="1" :step="5" />
              <ENumberField v-model="batchesEach" label="Batches each" :min="1" :step="100" />
              <ENumberField v-model="batchSize" label="Batch size" :min="256" :step="1024" />
              <ESelect v-model="rateFalls" label="Learning rate falls" :options="lrSchedules" />
              <ENumberField v-model="learningRate" label="Learning rate" :step="0.00001" />
              <ENumberField v-model="finalRate" label="Ends at" :step="0.000001" />
              <ENumberField v-model="resultWeight" label="Weight on the result" :min="0" :max="1" :step="0.1" />
            </div>
          </EStack>
        </EDisclosure>
      </template>

      <template v-if="kind === 'custom'">
        <ESelect v-model="gamePreset" label="Games" :options="presets" />
        <ESegmented
          v-model="stop"
          label="Stop"
          :options="[
            { value: 'sprt', label: 'When the evidence decides' },
            { value: 'pairs', label: 'After a number of pairs' },
          ]"
        />
        <div v-if="stop === 'sprt'" class="pair">
          <ENumberField v-model="lower" label="Lower bound" suffix="nElo" :step="0.25" />
          <ENumberField v-model="upper" label="Upper bound" suffix="nElo" :step="0.25" />
        </div>
        <ENumberField v-else v-model="fixedPairs" label="Pairs" :min="10" :step="100" />
      </template>

      <ESwitch
        v-if="hasLong"
        v-model="long"
        label="Also play long games"
        description="Runs 40+0.4 after short games pass. Worth it for a gain under about 2 Elo."
      />
      <ETextField
        v-model="tags"
        label="Tags"
        optional
        hint="Words that group ideas, separated by commas."
        autocomplete="off"
      />
    </EStack>
    <template #actions>
      <EButton :loading="busy" :disabled="!!missing" @click="create">Queue idea</EButton>
      <EButton variant="outline" @click="open = false">Cancel</EButton>
      <span v-if="missing" class="sec-note">{{ missing }}</span>
    </template>
  </EDialog>
</template>

<style scoped>
.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--e-gap-snug) var(--e-gap-base);
}
</style>
