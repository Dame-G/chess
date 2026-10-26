<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useQueryCache } from "@pinia/colada";
import { ECheckboxGroup } from "@eos-ui/vue/components/ECheckboxGroup";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { api } from "../../api/runner";
import { unwrap } from "../../api/queries";
import BranchPicker from "../BranchPicker.vue";
import type { LichessSettings, LichessStatus, Speed } from "../../lichess";

const props = defineProps<{ settings: LichessSettings }>();

const cache = useQueryCache();

const clone = (s: LichessSettings): LichessSettings => JSON.parse(JSON.stringify(s)) as LichessSettings;
const draft = ref(clone(props.settings));
let saved = JSON.stringify(props.settings);
let timer: ReturnType<typeof setTimeout> | undefined;
const note = ref<"" | "saving" | "saved">("");
const error = ref("");

// ---- Which version plays: a branch, always its newest build. A build number or tag saved earlier stays as typed.
const build = computed({
  get: () => draft.value.build ?? "",
  set: (v: string) => (draft.value.build = v || null),
});

// ---- The rules, in the words people use.
const speedOptions = [
  { value: "bullet", label: "Bullet" },
  { value: "blitz", label: "Blitz" },
  { value: "rapid", label: "Rapid" },
  { value: "classical", label: "Classical" },
];
const modeOptions = [
  { value: "rated", label: "Rated" },
  { value: "casual", label: "Casual" },
];
const whoOptions = [
  { value: "humans", label: "People" },
  { value: "bots", label: "Bots" },
];
const speeds = computed({
  get: () => draft.value.speeds as string[],
  set: (v) => {
    draft.value.speeds = v as Speed[];
    // The speeds are the clock filter: any initial time up to the longest speed picked.
    draft.value.min_initial = 0;
    draft.value.max_initial = v.includes("classical") ? 10_800 : 1800;
  },
});
const modes = computed({
  get: () => [...(draft.value.accept_rated ? ["rated"] : []), ...(draft.value.accept_casual ? ["casual"] : [])],
  set: (v) => {
    draft.value.accept_rated = v.includes("rated");
    draft.value.accept_casual = v.includes("casual");
  },
});
const who = computed({
  get: () => [...(draft.value.accept_humans ? ["humans"] : []), ...(draft.value.accept_bots ? ["bots"] : [])],
  set: (v) => {
    draft.value.accept_humans = v.includes("humans");
    draft.value.accept_bots = v.includes("bots");
  },
});

// ---- Mistakes the rules cannot carry out are said, not saved.
const problem = computed(() => {
  const d = draft.value;
  const numbers = [d.hash, d.threads, d.move_overhead, d.max_games, d.min_initial, d.max_initial];
  if ([...numbers, d.min_increment, d.max_increment].some((n) => typeof n !== "number" || Number.isNaN(n)))
    return "Fill in every number.";
  if (d.min_initial > d.max_initial) return "The shortest clock is longer than the longest.";
  if (d.min_increment > d.max_increment) return "The smallest increment is bigger than the largest.";
  if (!d.speeds.length) return "Pick at least one speed, or every challenge is declined.";
  return "";
});

async function save() {
  timer = undefined;
  if (problem.value) return;
  const body = clone(draft.value);
  // What it looks for is edited beside the Find a game button: keep the newest of it.
  const latest = cache.getQueryData<LichessStatus>(["lichess"])?.settings;
  if (latest) {
    body.seek_clocks = latest.seek_clocks;
    body.seek_rated = latest.seek_rated;
    body.seek_range = latest.seek_range;
  }
  note.value = "saving";
  error.value = "";
  try {
    const out = await unwrap(api.PUT("/api/lichess/settings", { body }));
    saved = JSON.stringify(out);
    cache.setQueryData<LichessStatus>(["lichess"], (old) => (old ? { ...old, settings: out } : old!));
    note.value = "saved";
  } catch (e) {
    error.value = (e as Error).message;
    note.value = "";
  }
}

// Every change is kept a moment after the last one, so typing a number is one save.
watch(
  draft,
  (d) => {
    clearTimeout(timer);
    timer = undefined;
    if (JSON.stringify(d) === saved) return;
    note.value = "";
    timer = setTimeout(save, 600);
  },
  { deep: true },
);
// The runner's settings changed elsewhere (the command line): follow them unless this page is mid-edit.
watch(
  () => props.settings,
  (s) => {
    const next = JSON.stringify(s);
    if (next !== saved && timer === undefined) draft.value = clone(s);
    saved = next;
  },
);
onBeforeUnmount(() => {
  if (timer) void save();
});
</script>

<template>
  <div class="rules">
    <section class="part" aria-labelledby="ls-plays">
      <h3 id="ls-plays" class="part-title">
        Plays
        <span v-if="note" class="saved" role="status">{{ note === "saving" ? "Saving…" : "Saved" }}</span>
      </h3>
      <BranchPicker
        v-model="build"
        label="Engine"
        size="sm"
        placeholder="Choose a version"
        allow-ref
        :hint="
          draft.build ? 'Always the newest build of the version.' : 'Until one is chosen, every challenge is declined.'
        "
      />
    </section>

    <section class="part" aria-labelledby="ls-accepts">
      <h3 id="ls-accepts" class="part-title">Accepts</h3>
      <ECheckboxGroup v-model="speeds" label="Speeds" orientation="horizontal" :options="speedOptions" />
      <div class="pair">
        <ECheckboxGroup v-model="modes" label="Games" orientation="horizontal" :options="modeOptions" />
        <ECheckboxGroup v-model="who" label="Opponents" orientation="horizontal" :options="whoOptions" />
      </div>
      <div class="pair">
        <ENumberField v-model="draft.min_increment" label="Increment from" size="sm" :min="0" suffix="s" />
        <ENumberField v-model="draft.max_increment" label="Increment up to" size="sm" :min="0" suffix="s" />
      </div>
      <ENumberField v-model="draft.max_games" label="Games at once" size="sm" :min="1" :max="8" class="narrow" />
      <ESwitch
        v-model="draft.from_position"
        label="Games from a set position"
        description="Standard chess only. Other variants are always declined."
      />
      <ENotice v-if="problem" tone="warning" title="Not saved yet" :role="false">{{ problem }}</ENotice>
      <ENotice v-if="error" tone="danger" title="Could not save" :role="false">{{ error }}</ENotice>
    </section>

    <EDisclosure title="Engine options">
      <div class="pair pair--engine">
        <ENumberField v-model="draft.hash" label="Hash" size="sm" :min="1" suffix="MB" />
        <ENumberField v-model="draft.threads" label="Threads" size="sm" :min="1" />
        <ENumberField
          v-model="draft.move_overhead"
          label="Move overhead"
          size="sm"
          :min="0"
          :max="2000"
          :step="50"
          suffix="ms"
        />
      </div>
    </EDisclosure>
  </div>
</template>

<style scoped>
.rules {
  display: grid;
  gap: var(--e-gap-loose);
}

.part {
  display: grid;
  gap: var(--e-gap-base);
}

.part-title {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}

.saved {
  font-weight: 400;
  letter-spacing: 0;
  text-transform: none;
}

.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--e-gap-base) var(--e-gap-loose);
}

.pair--engine {
  grid-template-columns: repeat(3, minmax(0, 1fr));
  padding-block-start: var(--e-gap-base);
}

.narrow {
  max-inline-size: 12rem;
}

@media (width < 30rem) {
  .pair--engine {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
