<script setup lang="ts">
import { computed, ref } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { plural, progressText, reviewCost } from "../format";
import { useImport } from "../sheets/useImport";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import { useWork } from "../stores/work";
import ProfileNotice from "./ProfileNotice.vue";

/**
 * What a profile can do next with its games, as one banner row with one filled button: import the player's games when
 * none are here, else review every one not reviewed yet. The review runs in the background, newest first, below
 * other work; while it runs the bar shows how far it is and can stop it. Both are analysis, so they are off
 * (with one calm line why) while you play on Lichess. `mine` is You, whose games are already here; with `importable`
 * it also offers to bring in what is new on Lichess, as a quiet button beside the review.
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
  /** The first thing to do on this screen (nothing is reviewed yet): bigger, with what a review gives. */
  lead?: boolean;
  /** Just the action, with its progress: no frame and no sentence, for a screen that says the rest itself. */
  bare?: boolean;
  /** Offer to bring in new games from Lichess too. */
  importable?: boolean;
  /** What that button says; "Import my Lichess games" for You. */
  importLabel?: string;
  /** Your whole history is already here, so the button only looks for new games: "Check for new games", an outline. */
  historyComplete?: boolean;
}>();

const gamesStore = useGames();
const policy = usePolicy();
const work = useWork();
const toast = useToast();
const importing = useImport(() => props.playerKey);

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
    w.priority === "background" ? "Other work goes first; each game shows here as soon as it is reviewed." : "";
  return `${paused}${background}`;
});

async function go() {
  busy.value = true;
  try {
    if (next.value === "import") await importing.run(IMPORT);
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
  <ProfileNotice v-if="next || underway || (importable && !bare)" :lead="lead && !!next" :bare="bare && !underway">
    <template v-if="underway">
      <EProgress v-if="underway.progress?.total" :value="fraction" :label="underway.title" :value-text="where" />
      <p v-else class="title" role="status">{{ underway.title }}: {{ where }}.</p>
      <p v-if="note" class="cost">{{ note }}</p>
    </template>
    <template v-else-if="next === 'import'">
      <p class="title">None of {{ name }}'s games are here yet.</p>
      <p class="cost">Reading the last {{ IMPORT }} from Lichess takes a moment.</p>
    </template>
    <template v-else-if="next === 'review'">
      <p class="title">{{ plural(todo, "game") }} not reviewed yet</p>
      <p v-if="lead && mine" class="cost">
        A review scores your accuracy, finds your mistakes and turns them into drills. It runs in the background, newest
        first (takes {{ reviewCost(todo) }}), so this page fills in as it goes.
      </p>
      <p v-else class="cost">Takes {{ reviewCost(todo) }} in the background, newest first.</p>
    </template>
    <template v-else>
      <p class="title">Every game is reviewed.</p>
    </template>
    <p v-if="!policy.allowed && !underway" class="cost" role="status">
      {{ policy.reason }}. This works again when your game ends.
    </p>

    <template #actions>
      <template v-if="underway">
        <EButton variant="outline" size="sm" :loading="stopping" @click="stop">Stop</EButton>
      </template>
      <template v-else>
        <EButton
          v-if="next"
          :variant="quiet ? 'outline' : 'primary'"
          :loading="busy"
          :disabled="!policy.allowed"
          @click="go"
        >
          {{
            next === "import"
              ? bare
                ? `Import ${IMPORT} games`
                : `Import their last ${IMPORT} games`
              : todo === 1
                ? "Review it"
                : `Review all ${todo.toLocaleString("en-GB")}`
          }}
        </EButton>
        <EButton
          v-if="importable && next !== 'import'"
          :variant="next || historyComplete ? 'outline' : 'primary'"
          :loading="importing.busy.value"
          :disabled="!policy.allowed"
          @click="importing.run(mine ? undefined : IMPORT)"
        >
          {{ importLabel ?? (historyComplete ? "Check for new games" : "Import my Lichess games") }}
        </EButton>
      </template>
    </template>
  </ProfileNotice>
</template>
