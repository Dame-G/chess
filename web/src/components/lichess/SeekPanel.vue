<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useQueryCache } from "@pinia/colada";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECheckboxGroup } from "@eos-ui/vue/components/ECheckboxGroup";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESpinner } from "@eos-ui/vue/components/ESpinner";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { api } from "../../api/runner";
import { unwrap } from "../../api/queries";
import { clockLabel, clockOptions, type LichessStatus, type SeekMode, type SeekState } from "../../lichess";

const props = defineProps<{ status: LichessStatus }>();

const cache = useQueryCache();
const seek = computed<SeekState>(() => props.status.seek ?? { mode: "off" });
const seeking = computed(() => seek.value.mode !== "off");
const playing = computed(() => props.status.playing.length > 0);
const online = computed(() => props.status.online);

// ---- The search itself: Find a game, Keep playing, Stop.
const busy = ref(false);
const error = ref("");
async function run(call: () => Promise<LichessStatus>) {
  busy.value = true;
  error.value = "";
  try {
    cache.setQueryData<LichessStatus>(["lichess"], await call());
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
const setMode = (mode: SeekMode) => run(() => unwrap(api.POST("/api/lichess/seek", { body: { mode } })));
const stop = () => run(() => unwrap(api.DELETE("/api/lichess/seek")));
const keep = computed({
  get: () => seek.value.mode === "always",
  set: (on: boolean) => void (on ? setMode("always") : stop()),
});

/** One sentence: what the search is doing, or what pressing the button will do. */
const sentence = computed(() => {
  if (!online.value && !seeking.value) return "Switch the bot online to look for a game.";
  if (seek.value.note) return seek.value.note;
  if (seeking.value) return playing.value ? "Playing. Will look again afterwards." : "Looking for a bot to play…";
  return "Challenges a random online bot near your rating.";
});
const pending = computed(() => seek.value.pending ?? undefined);

// ---- What it looks for: kept a moment after the last change, like the rest of the settings.
/** The search settings, with the defaults for a runner too old to send them. */
const seekOf = (s: LichessStatus["settings"]) => ({
  clocks: s.seek_clocks ?? ["3+0", "3+2", "5+3"],
  rated: s.seek_rated ?? true,
  range: s.seek_range ?? 300,
});
const first = seekOf(props.status.settings);
const clocks = ref([...first.clocks]);
const rated = ref(first.rated);
const range = ref<number | null>(first.range);
const options = computed(() => clockOptions(clocks.value));
const problem = computed(() =>
  !clocks.value.length
    ? "Pick at least one clock."
    : typeof range.value !== "number" || range.value < 25 || range.value > 2000
      ? "The rating range is between 25 and 2000."
      : "",
);
const saveError = ref("");
let timer: ReturnType<typeof setTimeout> | undefined;

async function save() {
  timer = undefined;
  if (problem.value) return;
  // Settings are replaced whole: start from the newest ones, so other edits are not undone.
  const latest = cache.getQueryData<LichessStatus>(["lichess"])?.settings ?? props.status.settings;
  const body = { ...latest, seek_clocks: clocks.value, seek_rated: rated.value, seek_range: range.value! };
  saveError.value = "";
  try {
    const out = await unwrap(api.PUT("/api/lichess/settings", { body }));
    cache.setQueryData<LichessStatus>(["lichess"], (old) => (old ? { ...old, settings: out } : old!));
  } catch (e) {
    saveError.value = (e as Error).message;
  }
}
const same = () => {
  const s = seekOf(props.status.settings);
  return (
    rated.value === s.rated &&
    range.value === s.range &&
    clocks.value.length === s.clocks.length &&
    clocks.value.every((c) => s.clocks.includes(c))
  );
};
watch([clocks, rated, range], () => {
  clearTimeout(timer);
  timer = undefined;
  if (same()) return;
  timer = setTimeout(save, 500);
});
// Changed elsewhere (the command line): follow, unless this panel is mid-edit.
watch(
  () => props.status.settings,
  (s) => {
    if (timer !== undefined) return;
    const next = seekOf(s);
    clocks.value = [...next.clocks];
    rated.value = next.rated;
    range.value = next.range;
  },
);
onBeforeUnmount(() => {
  if (timer) void save();
});
</script>

<template>
  <section class="seek" aria-labelledby="sk-title">
    <header class="head">
      <h3 id="sk-title" class="part-title">Find a game</h3>
      <ESwitch
        v-model="keep"
        label="Keep playing"
        description="Start a new game as soon as one ends"
        :disabled="busy || (!online && !keep)"
        class="keep"
      />
    </header>

    <div class="act">
      <EButton v-if="!seeking" :disabled="!online" :loading="busy" @click="setMode('once')">Find a game</EButton>
      <EButton v-else variant="outline" :loading="busy" @click="stop">Stop looking</EButton>
      <p class="say" role="status">
        <ESpinner v-if="seeking && !playing" size="sm" silent class="spin" />
        <span>{{ sentence }}</span>
      </p>
    </div>

    <p v-if="pending" class="pending">
      <span class="opp">
        <span v-if="pending.opponent.title" class="title">{{ pending.opponent.title }}</span>
        <span class="name">{{ pending.opponent.name }}</span>
        <span class="rating num">{{ pending.opponent.rating }}</span>
      </span>
      <span class="meta num">
        {{ clockLabel(pending.clock.initial, pending.clock.increment) }} · {{ pending.rated ? "rated" : "casual" }}
      </span>
      <ELink :href="pending.url" external>View on Lichess</ELink>
    </p>

    <ENotice v-if="error" tone="danger" title="That did not work" :role="false">{{ error }}</ENotice>

    <ECheckboxGroup
      v-model="clocks"
      label="Clocks to play"
      orientation="horizontal"
      :options="options"
      hint="One is picked at random for each challenge. Minutes plus seconds a move."
    />
    <div class="pair">
      <ESwitch v-model="rated" label="Rated" description="Casual if off" />
      <ENumberField v-model="range" label="Rating range" size="sm" prefix="±" :min="25" :max="2000" :step="25" />
    </div>
    <ENotice v-if="problem" tone="warning" title="Not saved yet" :role="false">{{ problem }}</ENotice>
    <ENotice v-if="saveError" tone="danger" title="Could not save" :role="false">{{ saveError }}</ENotice>
  </section>
</template>

<style scoped>
.seek {
  display: grid;
  gap: var(--e-gap-base);
  padding-block-end: var(--e-gap-loose);
  border-block-end: var(--e-stroke-hair) solid var(--e-border);
}

.head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-base);
}

.part-title {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.act {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-base) var(--e-gap-loose);
}

.say {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  min-inline-size: 0;
  margin: 0;
  color: var(--e-muted);
}

.pending {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--e-gap-snug) var(--e-gap-base);
  margin: 0;
  padding: var(--e-gap-snug) var(--e-gap-base);
  border-inline-start: 2px solid var(--e-primary);
  background: color-mix(in oklab, var(--e-border) 25%, transparent);
}

.opp {
  display: inline-flex;
  align-items: baseline;
  gap: var(--e-gap-tight);
}

.title {
  color: var(--e-primary);
  font: var(--e-type-small);
  font-weight: 700;
}

.name {
  font-weight: 600;
}

.rating,
.meta {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: start;
  gap: var(--e-gap-base) var(--e-gap-loose);
}

@media (width < 30rem) {
  .pair {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
