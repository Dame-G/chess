<script setup lang="ts">
/**
 * Starting a game from the board: you against a build, or two builds against each other. It sits in the inspector
 * beside the board, so a game is a choice or two away, from the start position or from the board as it stands.
 */
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { PhArrowsLeftRight, PhPlay } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { api } from "../api/runner";
import { unwrap, useJobs, useMatches, useVersions, type Branch } from "../api/queries";
import type { components } from "../api/schema";
import { stored } from "../stored";
import BranchPicker from "../components/BranchPicker.vue";
import { sortVersions } from "../components/versions/versions";

type MatchRequest = components["schemas"]["MatchRequest"];

const props = defineProps<{
  mode: "play" | "engines";
  /** The position on the board, for "From the board". */
  fen: string;
  moves: string[];
}>();

const router = useRouter();
const route = useRoute();
const { state: versions } = useVersions();
const { state: jobs } = useJobs();
const { state: recent } = useMatches();
const running = computed(() => recent.value.data?.find((m) => m.status === "running" && !m.lichess));

// The versions: newest main first; for two engines the second is the newest other version that has a build.
// The Versions page opens this with `white`, `black` or `opponent` naming branches.
const asked = (key: string) => (typeof route.query[key] === "string" ? (route.query[key] as string) : "");
const opponent = ref(asked("opponent") || "main");
const white = ref(asked("white") || "main");
const black = ref(asked("black"));
if (asked("opponent") || asked("white") || asked("black")) {
  const rest = { ...route.query };
  delete rest.opponent;
  delete rest.white;
  delete rest.black;
  void router.replace({ query: rest });
}
watch(
  () => versions.value.data,
  (list) => {
    if (black.value || !list) return;
    const other = sortVersions(list).find((v) => !v.is_main && !v.upstream && !v.hidden && v.latest_build_id != null);
    black.value = other?.name ?? "main";
  },
  { immediate: true },
);

/** The build that plays for a version: its newest. */
const buildOf = (name: string) => versions.value.data?.find((v) => v.name === name)?.latest_build_id ?? undefined;
const note = (branch: Branch) => (buildOf(branch.name) == null ? "not built" : undefined);
const chosen = computed(() => (props.mode === "play" ? [opponent.value] : [white.value, black.value]));
/** The chosen versions that have no build to play with. */
const unbuilt = computed(() =>
  versions.value.status === "success" ? [...new Set(chosen.value.filter((n) => n && buildOf(n) == null))] : [],
);
const building = (name: string) =>
  !!jobs.value.data?.some(
    (j) => j.spec.kind === "build" && j.spec.ref === name && (j.status === "queued" || j.status === "running"),
  );
async function build(name: string) {
  error.value = "";
  try {
    await unwrap(api.POST("/api/builds", { body: { ref: name } }));
  } catch (e) {
    error.value = (e as Error).message;
  }
}

// You against a build.
const colour = stored("play.colour", "random");
const colours = [
  { value: "random", label: "Random" },
  { value: "white", label: "White" },
  { value: "black", label: "Black" },
];
const clocks = [
  { value: "1+0", label: "1+0" },
  { value: "3+2", label: "3+2" },
  { value: "5+3", label: "5+3" },
  { value: "10+5", label: "10+5" },
  { value: "15+10", label: "15+10" },
];
const clock = stored("play.clock", "5+3");
const casual = stored("play.casual", false);
const timings = [
  { value: "clock", label: "Clock" },
  { value: "move", label: "Per move" },
];
const timing = computed({
  get: () => (casual.value ? "move" : "clock"),
  set: (v) => (casual.value = v === "move"),
});
const thinks = [
  { value: "500", label: "0.5 s" },
  { value: "1000", label: "1 s" },
  { value: "3000", label: "3 s" },
  { value: "10000", label: "10 s" },
];
const think = stored("play.think", "1000");

// Two builds.
const pace = stored("match.preset", "blitz");
// Named by the clock itself: "bullet" and "rapid" mean other things on Lichess.
const paces = [
  { value: "bullet", label: "1+0.01" },
  { value: "blitz", label: "10+0.1" },
  { value: "rapid", label: "60+0.6" },
  { value: "custom", label: "Custom" },
];
const paceClocks: Record<string, [number, number]> = { bullet: [1, 0.01], blitz: [10, 0.1], rapid: [60, 0.6] };
const customSeconds = stored<number | null>("match.seconds", 30);
const customIncrement = stored<number | null>("match.increment", 0.3);
const paceSeconds = computed(() => (pace.value === "custom" ? (customSeconds.value ?? 0) : paceClocks[pace.value]![0]));
const paceIncrement = computed(() =>
  pace.value === "custom" ? (customIncrement.value ?? 0) : paceClocks[pace.value]![1],
);
const paceHint = computed(() => `${paceSeconds.value} s each, plus ${paceIncrement.value} s a move.`);

// Where it starts.
const away = computed(() => props.fen !== "startpos" || props.moves.length > 0);
const fromBoard = stored("play.fromBoard", true);
const playStart = computed({
  get: () => (fromBoard.value ? "position" : "start"),
  set: (v) => (fromBoard.value = v === "position"),
});
const playStartOptions = [
  { value: "start", label: "Start" },
  { value: "position", label: "Board" },
];
const chosenStart = stored("match.opening", "start");
const startOptions = computed(() => [
  { value: "start", label: "Start" },
  { value: "position", label: "Board", disabled: !away.value },
  { value: "book", label: "Book" },
]);
const start = computed({
  get: () => (chosenStart.value === "position" && !away.value ? "start" : chosenStart.value),
  set: (v) => (chosenStart.value = v),
});
const startHint = computed(() =>
  start.value === "position"
    ? "From the board as it stands."
    : start.value === "book"
      ? "A random line from the opening book the tests use."
      : "From the starting position.",
);

const busy = ref(false);
const error = ref("");
const valid = computed(() =>
  props.mode === "play"
    ? buildOf(opponent.value) != null
    : buildOf(white.value) != null && buildOf(black.value) != null && paceSeconds.value > 0 && paceIncrement.value >= 0,
);

function opening(here: boolean): MatchRequest["opening"] {
  return here && away.value ? { type: "position", fen: props.fen, moves: props.moves } : { type: "start" };
}

async function begin() {
  if (!valid.value || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    let body: MatchRequest;
    if (props.mode === "play") {
      const mine = colour.value === "random" ? (Math.random() < 0.5 ? "white" : "black") : colour.value;
      const opposing = buildOf(opponent.value)!;
      const [w, b] = mine === "white" ? (["human", opposing] as const) : ([opposing, "human"] as const);
      const [seconds, increment] = clock.value.split("+").map(Number) as [number, number];
      body = {
        white: w,
        black: b,
        time: casual.value
          ? { type: "movetime", ms: Number(think.value) }
          : { type: "clock", seconds: seconds * 60, increment },
        opening: opening(fromBoard.value),
      };
    } else {
      body = {
        white: buildOf(white.value)!,
        black: buildOf(black.value)!,
        time: { type: "clock", seconds: paceSeconds.value, increment: paceIncrement.value },
        opening: start.value === "book" ? { type: "book" } : opening(start.value === "position"),
      };
    }
    const m = await unwrap(api.POST("/api/matches", { body }));
    void router.push(`/matches/${m.id}`);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}

function swap() {
  [white.value, black.value] = [black.value, white.value];
}
</script>

<template>
  <form class="setup" @submit.prevent="begin">
    <ENotice
      v-if="running"
      tone="info"
      :title="running.white.human || running.black.human ? 'Your game is still on' : 'A match is running'"
      :role="false"
    >
      Only one game runs at a time.
      <template #actions>
        <EButton size="sm" variant="outline" @click="router.push(`/matches/${running.id}`)">Go to it</EButton>
      </template>
    </ENotice>

    <template v-if="mode === 'play'">
      <BranchPicker v-model="opponent" label="Play against" size="sm" :note="note" />
      <ESegmented v-model="colour" label="You play" size="sm" :options="colours" />
      <ESegmented v-model="timing" label="Time" size="sm" :options="timings" />
      <ESegmented v-if="!casual" v-model="clock" label="Clock (minutes + seconds)" size="sm" :options="clocks" />
      <ESegmented
        v-else
        v-model="think"
        label="Engine thinks per move"
        size="sm"
        :options="thinks"
        hint="You can take moves back in a game without a clock."
      />
      <ESegmented
        v-if="away"
        v-model="playStart"
        label="Opening"
        size="sm"
        :options="playStartOptions"
        :hint="playStart === 'position' ? 'From the board as it stands.' : 'From the starting position.'"
      />
    </template>

    <template v-else>
      <div class="engines">
        <BranchPicker v-model="white" label="White" size="sm" :note="note" />
        <EIconButton :icon="PhArrowsLeftRight" label="Swap colours" tooltip size="sm" class="swap" @click="swap" />
        <BranchPicker v-model="black" label="Black" size="sm" :note="note" />
      </div>
      <ESegmented
        v-model="pace"
        label="Time control"
        size="sm"
        :options="paces"
        :hint="pace === 'custom' ? undefined : paceHint"
      />
      <div v-if="pace === 'custom'" class="pair">
        <ENumberField v-model="customSeconds" label="Seconds each" size="sm" :min="1" :step="1" />
        <ENumberField v-model="customIncrement" label="Increment" size="sm" :min="0" :step="0.01" suffix="s" />
      </div>
      <ESegmented v-model="start" label="Opening" size="sm" :options="startOptions" :hint="startHint" />
    </template>

    <ENotice v-for="name in unbuilt" :key="name" tone="warning" :title="`${name} has no build yet`" :role="false">
      A version has to be built before it can play.
      <template #actions>
        <EButton size="sm" variant="outline" :loading="building(name)" @click="build(name)">Build it</EButton>
      </template>
    </ENotice>
    <ENotice v-if="error" tone="danger" title="The game did not start" :role="false">{{ error }}</ENotice>
    <EButton type="submit" block :icon="PhPlay" :disabled="!valid" :loading="busy">
      {{ mode === "play" ? "Play" : "Start match" }}
    </EButton>
  </form>
</template>

<style scoped>
.setup {
  display: grid;
  gap: var(--e-gap-base);
  padding: var(--e-gap-base) var(--ws-pad);
}

.engines {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: end;
  gap: var(--e-gap-snug);
}

.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--e-gap-base);
}

/* Build names are long ("test/no-nmp · 85865a3"): on a phone each gets the full width. */
@media (width < 30rem) {
  .engines {
    grid-template-columns: minmax(0, 1fr);
  }

  .swap {
    justify-self: center;
  }
}
</style>
