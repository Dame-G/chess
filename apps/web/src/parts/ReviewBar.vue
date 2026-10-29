<script setup lang="ts">
import { computed, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { plural, progressText, reviewCost } from "../format";
import { usePlayers } from "../stores/players";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useWork } from "../stores/work";

/**
 * What a profile can do next with its games, as one filled button: import the player's games when none are here,
 * else review every one not reviewed yet. The review runs in the background, newest first, below ideas, so trials keep
 * playing; while it runs the bar shows how far it is and can stop it. Both are analysis, so they are off (with one
 * calm line why) while you play on Lichess. `mine` is You, whose games are already here.
 */
const props = defineProps<{
  playerKey: string;
  name: string;
  games: number;
  /** Not reviewed in the profile's window; stands in until the server says how many of all their games are not. */
  pending: number;
  /** All their games not reviewed yet, whatever the window: what "Review all" reviews. */
  total?: number | null;
  mine?: boolean;
  /** Another action on the screen is the filled one. */
  quiet?: boolean;
}>();

const players = usePlayers();
const gamesStore = useGames();
const policy = usePolicy();
const work = useWork();
const toast = useToast();

const IMPORT = 50;
const busy = ref(false);
const stopping = ref(false);

/** Work of this kind for this player that is queued or running, so the button does not ask twice. */
const underway = computed(() =>
  [...work.running, ...work.queued].find(
    (w) =>
      (w.kind === "import" || w.kind === "review") && w.subject.type === "player" && w.subject.key === props.playerKey,
  ),
);
/** How many games "Review all" reviews: every one not reviewed, not only those in the profile's window. */
const todo = computed(() => props.total ?? props.pending);
const next = computed<"import" | "review" | null>(() =>
  props.games === 0 && !props.mine ? "import" : todo.value > 0 ? "review" : null,
);
const fraction = computed(() => {
  const p = underway.value?.progress;
  return p?.total ? (p.done / p.total) * 100 : 0;
});
/** How far it is: "6 of 23 games", or why it is not moving yet. */
const where = computed(() => {
  const w = underway.value;
  if (!w) return "";
  return progressText(w.progress) ?? (w.status === "queued" ? "Waiting its turn" : "Starting");
});
/** One line under the bar: why it waits, and what a background review leaves to others. */
const note = computed(() => {
  const w = underway.value;
  if (!w) return "";
  const paused = w.status === "paused" && w.paused_for ? `Paused: ${w.paused_for}. ` : "";
  const background =
    w.priority === "background" ? "Trials go first; each game shows here as soon as it is reviewed." : "";
  return `${paused}${background}`;
});

async function go() {
  busy.value = true;
  try {
    if (next.value === "import") await players.importHistory(props.playerKey, { max: IMPORT });
    else await gamesStore.reviewAll(props.playerKey);
  } catch (e) {
    toast.show({ title: "That did not start", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  } finally {
    busy.value = false;
  }
}

async function stop() {
  const w = underway.value;
  if (!w) return;
  stopping.value = true;
  try {
    await work.stop(w.id);
  } catch (e) {
    toast.show({ title: "That did not stop", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  } finally {
    stopping.value = false;
  }
}
</script>

<template>
  <div v-if="next || underway" class="bar">
    <template v-if="underway">
      <div class="line stack">
        <EProgress v-if="underway.progress?.total" :value="fraction" :label="underway.title" :value-text="where" />
        <p v-else class="title" role="status">{{ underway.title }}: {{ where }}.</p>
        <p v-if="note" class="cost">{{ note }}</p>
      </div>
      <EButton variant="outline" :loading="stopping" @click="stop">Stop</EButton>
    </template>
    <template v-else>
      <p class="line">
        <template v-if="next === 'import'">
          None of {{ name }}'s games are here yet. Reading the last {{ IMPORT }} from Lichess takes a moment.
        </template>
        <template v-else>
          {{ plural(todo, "game") }} not reviewed yet
          <span class="cost">— {{ reviewCost(todo) }} in the background, newest first</span>
        </template>
      </p>
      <EButton :variant="quiet ? 'outline' : 'primary'" :loading="busy" :disabled="!policy.allowed" @click="go">
        {{
          next === "import"
            ? `Import their last ${IMPORT} games`
            : todo === 1
              ? "Review it"
              : `Review all ${todo.toLocaleString("en-GB")}`
        }}
      </EButton>
      <p v-if="!policy.allowed" class="sec-note why" role="status">
        {{ policy.reason }}. This works again when your game ends.
      </p>
    </template>
  </div>
</template>

<style scoped>
.bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug) var(--e-gap-base);
  padding: var(--e-gap-snug);
  border: var(--line);
  border-radius: var(--e-radius-sm);
  background: var(--e-surface-tint, transparent);
}

.stack {
  display: grid;
  gap: var(--e-gap-tight);
}

.line {
  flex: 1 1 14rem;
  margin: 0;
  min-width: 0;
}

.title {
  margin: 0;
}

.cost {
  margin: 0;
  color: var(--e-muted);
}

.why {
  flex-basis: 100%;
}
</style>
