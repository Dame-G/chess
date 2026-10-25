<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { api } from "../api/runner";
import { defaultBuild, unwrap, useBuilds, useMatches } from "../api/queries";
import { stored } from "../stored";

const props = defineProps<{
  /** The position on the board, for "This position". */
  fen: string;
  moves: string[];
}>();
const open = defineModel<boolean>("open", { default: false });

const router = useRouter();
const { state: builds } = useBuilds();
const { state: recent } = useMatches();
const running = computed(() => recent.value.data?.find((m) => m.status === "running"));

const white = ref("");
const black = ref("");
const buildOptions = computed(() =>
  (builds.value.data ?? []).map((b) => ({
    value: String(b.id),
    label: `${b.ref} · ${b.commit.slice(0, 7)}`,
    description: b.subject,
  })),
);

const presets = [
  { value: "bullet", label: "Bullet 1+0.01" },
  { value: "blitz", label: "Blitz 10+0.1" },
  { value: "rapid", label: "Rapid 60+0.6" },
  { value: "custom", label: "Custom" },
];
const clocks: Record<string, [number, number]> = { bullet: [1, 0.01], blitz: [10, 0.1], rapid: [60, 0.6] };
const preset = stored("match.preset", "blitz");
const customSeconds = stored<number | null>("match.seconds", 30);
const customIncrement = stored<number | null>("match.increment", 0.3);

const here = computed(() => props.fen === "startpos" && !props.moves.length);
const openings = computed(() => [
  { value: "start", label: "Start position" },
  { value: "position", label: "This position", disabled: here.value },
  { value: "book", label: "Random book opening" },
]);
const opening = stored("match.opening", "start");
const openingHint = computed(() => {
  if (opening.value === "position")
    return props.moves.length
      ? `The board as it stands, after ${props.moves.length} move${props.moves.length === 1 ? "" : "s"}.`
      : "The board as it stands.";
  if (opening.value === "book") return "A line from the opening book the tests use, different every time.";
  return undefined;
});

// Each time it opens: newest main against the newest other build, and a stored opening that still applies.
function pick() {
  const list = builds.value.data ?? [];
  const first = defaultBuild(list);
  white.value = first ? String(first.id) : "";
  black.value = String((list.find((b) => b.id !== first?.id && b.ref !== first?.ref) ?? first)?.id ?? "");
}
watch(open, (o) => {
  if (!o) return;
  error.value = "";
  pick();
  if (opening.value === "position" && here.value) opening.value = "start";
});
// Opened before the builds arrived: fill the choice in when they do.
watch(
  () => builds.value.data,
  () => {
    if (open.value && !white.value) pick();
  },
);

const seconds = computed(() => (preset.value === "custom" ? (customSeconds.value ?? 0) : clocks[preset.value]![0]));
const increment = computed(() => (preset.value === "custom" ? (customIncrement.value ?? 0) : clocks[preset.value]![1]));
const valid = computed(() => !!white.value && !!black.value && seconds.value > 0 && increment.value >= 0);

const busy = ref(false);
const error = ref("");

async function start() {
  if (!valid.value || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const m = await unwrap(
      api.POST("/api/matches", {
        body: {
          white: Number(white.value),
          black: Number(black.value),
          time: { type: "clock", seconds: seconds.value, increment: increment.value },
          opening:
            opening.value === "book"
              ? { type: "book" }
              : opening.value === "position" && !here.value
                ? { type: "position", fen: props.fen, moves: props.moves }
                : { type: "start" },
        },
      }),
    );
    open.value = false;
    void router.push(`/matches/${m.id}`);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}

function watchRunning() {
  const id = running.value?.id;
  if (!id) return;
  open.value = false;
  void router.push(`/matches/${id}`);
}
</script>

<template>
  <EDialog
    v-model:open="open"
    title="Watch a match"
    description="Two builds play each other, move by move, with real clocks."
    size="md"
  >
    <form class="form" @submit.prevent="start">
      <div class="pair">
        <ESelect v-model="white" label="White" size="sm" :options="buildOptions" />
        <ESelect v-model="black" label="Black" size="sm" :options="buildOptions" />
      </div>
      <ESegmented v-model="preset" label="Time control" size="sm" :options="presets" />
      <div v-if="preset === 'custom'" class="pair">
        <ENumberField v-model="customSeconds" label="Seconds each" size="sm" :min="1" :step="1" />
        <ENumberField v-model="customIncrement" label="Increment" size="sm" :min="0" :step="0.01" suffix="s" />
      </div>
      <ESegmented v-model="opening" label="Opening" size="sm" :options="openings" :hint="openingHint" />
      <ENotice v-if="error" tone="danger" title="The match did not start" :role="false">
        {{ error }}
        <template v-if="running" #action>
          <EButton size="sm" variant="outline" @click="watchRunning">Watch it</EButton>
        </template>
      </ENotice>
    </form>
    <template #actions>
      <EButton :disabled="!valid" :loading="busy" @click="start">Start match</EButton>
      <EButton variant="quiet" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.form {
  display: grid;
  gap: var(--e-gap-base);
}

.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--e-gap-base);
}
</style>
