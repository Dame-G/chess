<script setup lang="ts">
/**
 * Starting a game from the board: you against a build, or two builds against each other. It sits in the inspector
 * beside the board, so a game is a choice or two away, from the start position or from the board as it stands.
 */
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { PhArrowsLeftRight, PhPlay } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { api } from "../api/runner";
import { defaultBuild, unwrap, useBuilds, useMatches } from "../api/queries";
import type { components } from "../api/schema";
import { stored } from "../stored";

type MatchRequest = components["schemas"]["MatchRequest"];

const props = defineProps<{
  mode: "play" | "engines";
  /** The position on the board, for "From the board". */
  fen: string;
  moves: string[];
}>();

const router = useRouter();
const { state: builds } = useBuilds();
const { state: recent } = useMatches();
const running = computed(() => recent.value.data?.find((m) => m.status === "running" && !m.lichess));

const options = computed(() =>
  (builds.value.data ?? []).map((b) => ({
    value: String(b.id),
    label: `${b.ref} · ${b.commit.slice(0, 7)}`,
    description: b.subject,
  })),
);

// The builds: newest main first; for two engines the second is the newest build of another ref.
const opponent = ref("");
const white = ref("");
const black = ref("");
function pick() {
  const list = builds.value.data ?? [];
  const first = defaultBuild(list);
  if (!opponent.value || !list.some((b) => String(b.id) === opponent.value)) opponent.value = String(first?.id ?? "");
  if (!white.value || !list.some((b) => String(b.id) === white.value)) white.value = String(first?.id ?? "");
  if (!black.value || !list.some((b) => String(b.id) === black.value))
    black.value = String((list.find((b) => b.id !== first?.id && b.ref !== first?.ref) ?? first)?.id ?? "");
}
watch(() => builds.value.data, pick, { immediate: true });

// You against a build.
const colour = stored("play.colour", "random");
const colours = [
  { value: "white", label: "White" },
  { value: "random", label: "Random" },
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
const thinks = [
  { value: "500", label: "0.5 s" },
  { value: "1000", label: "1 s" },
  { value: "3000", label: "3 s" },
  { value: "10000", label: "10 s" },
];
const think = stored("play.think", "1000");

// Two builds.
const pace = stored("match.preset", "blitz");
const paces = [
  { value: "bullet", label: "Bullet" },
  { value: "blitz", label: "Blitz" },
  { value: "rapid", label: "Rapid" },
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
    ? !!opponent.value
    : !!white.value && !!black.value && paceSeconds.value > 0 && paceIncrement.value >= 0,
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
      const build = Number(opponent.value);
      const [w, b] = mine === "white" ? (["human", build] as const) : ([build, "human"] as const);
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
        white: Number(white.value),
        black: Number(black.value),
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
      <template #action>
        <EButton size="sm" variant="outline" @click="router.push(`/matches/${running.id}`)">Go to it</EButton>
      </template>
    </ENotice>

    <template v-if="mode === 'play'">
      <ESelect v-model="opponent" label="Play against" size="sm" :options="options" />
      <ESegmented v-model="colour" label="You play" size="sm" :options="colours" />
      <ESegmented v-model="clock" label="Clock (minutes + seconds)" size="sm" :options="clocks" :disabled="casual" />
      <ESwitch v-model="casual" label="No clock" />
      <ESegmented
        v-if="casual"
        v-model="think"
        label="Engine thinks per move"
        size="sm"
        :options="thinks"
        hint="You can take moves back in a game without a clock."
      />
      <ESwitch v-if="away" v-model="fromBoard" label="Start from the board as it stands" />
    </template>

    <template v-else>
      <div class="engines">
        <ESelect v-model="white" label="White" size="sm" :options="options" />
        <EIconButton :icon="PhArrowsLeftRight" label="Swap colours" tooltip size="sm" class="swap" @click="swap" />
        <ESelect v-model="black" label="Black" size="sm" :options="options" />
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
