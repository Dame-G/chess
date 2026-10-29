<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { ESheet } from "@eos-ui/vue/components/ESheet";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import type { Player, ProfileFilter } from "../api/types";
import ChallengeList from "../parts/ChallengeList.vue";
import DrillsCard from "../parts/DrillsCard.vue";
import ProfileFilters from "../parts/ProfileFilters.vue";
import ProfileHead from "../parts/ProfileHead.vue";
import ProfileSkeleton from "../parts/ProfileSkeleton.vue";
import ProfileView from "../parts/ProfileView.vue";
import ProgressChart from "../parts/ProgressChart.vue";
import RepertoireView from "../parts/RepertoireView.vue";
import ReviewBar from "../parts/ReviewBar.vue";
import { useUi } from "../shell/ui";
import { useConnections } from "../stores/connections";
import { progressText } from "../format";
import { usePolicy } from "../stores/policy";
import { ALL_GAMES, usePlayers } from "../stores/players";
import { useProgress } from "../stores/progress";
import { useWork } from "../stores/work";
import { useSheet } from "./useSheet";

/**
 * You: your Lichess name and ratings, then your games added up (accuracy and rating over time, phases, results, moves,
 * openings, time, the best moments and worst blunders as links onto the board, and the weak spots to play out), what
 * is left to review, your challenges, and Compare with…. Not linked to Lichess yet: the way to log in comes first.
 */
const sheet = useSheet();
const connections = useConnections();
const players = usePlayers();
const progress = useProgress();
const work = useWork();
const router = useRouter();
const toast = useToast();

const filter = ref<ProfileFilter>({ ...ALL_GAMES });
const failed = ref<string | null>(null);
const profile = computed(() => players.profileOf("you", filter.value));
const ui = useUi();
const policy = usePolicy();
/** No game at all with nothing filtered out: the first run, not an empty filter. */
const firstRun = computed(
  () => profile.value?.games === 0 && !filter.value.speeds?.length && filter.value.rated == null && !filter.value.since,
);
const importing = ref(false);
/** Your import, queued or running: the first brings in your whole history, later ones only what is new. */
const importWork = computed(() =>
  [...work.running, ...work.queued].find(
    (w) => w.kind === "import" && w.subject.type === "player" && w.subject.key === "you",
  ),
);
const importLine = computed(() => {
  const w = importWork.value;
  return w ? `${w.title}: ${progressText(w.progress) ?? "starting"}.` : "";
});
async function importMine() {
  importing.value = true;
  try {
    await players.importHistory("you");
  } catch (e) {
    toast.show({ title: "That did not start", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  } finally {
    importing.value = false;
  }
}
const linked = computed(() => connections.you.configured && !!connections.you.account);

async function load() {
  failed.value = null;
  try {
    await players.loadProfile("you", filter.value);
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  }
}
watch(filter, load, { immediate: true, deep: true });
onBeforeUnmount(() => players.unwatch("you"));
onMounted(() => void progress.load().catch(() => {}));

/** Without a Lichess login the head still names You, from the account if there is one. */
const player = computed<Player | null>(() => {
  if (profile.value) return profile.value.player;
  const a = connections.you.account;
  return a ? { key: "you", name: a.username, title: a.title, ratings: a.ratings, lichess: a.username } : null;
});

const logging = ref(false);
async function login() {
  logging.value = true;
  try {
    const { url } = await connections.login("/you");
    window.location.assign(url);
  } catch (e) {
    logging.value = false;
    toast.show({ title: "Login did not start", message: e instanceof Error ? e.message : String(e), tone: "warning" });
  }
}

const challenges = computed(() => connections.you.challenges);
const compare = () => router.push({ path: "/compare", query: { players: "you,dameless" } });
</script>

<template>
  <ESheet
    v-bind="sheet.bind"
    title="You"
    description="Your games added up: how you play, what to work on, and how you compare."
  >
    <ENotice v-if="!linked" tone="info" title="Your Lichess account is not linked" class="gap">
      Log in to see your ratings, play on Lichess from here and bring your games in.
      <template #actions>
        <EButton :loading="logging" @click="login">Log in with Lichess</EButton>
      </template>
    </ENotice>

    <ProfileHead v-if="player && linked" :player="player" />

    <section v-if="linked" class="sec" aria-labelledby="y-ch">
      <h2 id="y-ch" class="part-title">Your challenges</h2>
      <ChallengeList v-if="challenges.length" :challenges="challenges" connection="you" />
      <p v-else class="sec-note">None waiting. Challenges to you on Lichess show here.</p>
    </section>

    <ENotice v-if="failed" tone="danger" title="Your profile did not load" class="gap" retry @retry="load">
      {{ failed }}
    </ENotice>
    <ProfileSkeleton v-else-if="!profile" label="Loading your profile" />
    <EEmptyState v-else-if="firstRun" title="No games yet">
      Play one against Dameless{{ linked ? ", or bring in your Lichess games" : "" }}. Your profile, drills and openings
      are built from your games.
      <template #action>
        <div class="sheet-actions">
          <EButton @click="ui.startNewGame()">New game</EButton>
          <EButton
            v-if="linked"
            variant="outline"
            :loading="importing || !!importWork"
            :disabled="!policy.allowed"
            @click="importMine"
          >
            Import my Lichess games
          </EButton>
          <EButton v-else variant="outline" :loading="logging" @click="login">Log in with Lichess</EButton>
        </div>
        <p v-if="importWork" class="sec-note" role="status">{{ importLine }}</p>
      </template>
    </EEmptyState>
    <template v-else>
      <section class="sec" aria-label="Games and review">
        <ProfileFilters v-model="filter" :games="profile.games" whose="your" />
        <ReviewBar
          player-key="you"
          name="You"
          mine
          :quiet="!linked"
          :games="profile.games"
          :pending="profile.pending_review"
          :total="profile.unreviewed_total"
        />
        <p v-if="profile.games === 0" class="sec-note">No games match these filters.</p>
        <div v-if="linked && !importWork" class="sheet-actions">
          <EButton variant="outline" :loading="importing" :disabled="!policy.allowed" @click="importMine">
            Import my Lichess games
          </EButton>
        </div>
      </section>

      <ProfileView :profile="profile" is-you />

      <section v-if="progress.chart" class="sec" aria-labelledby="y-progress">
        <div class="sec-head">
          <h2 id="y-progress" class="part-title">Progress</h2>
          <p class="sec-note">You and Dameless, side by side.</p>
        </div>
        <ProgressChart :chart="progress.chart" />
      </section>

      <DrillsCard />
      <RepertoireView />

      <section class="sec" aria-label="Compare">
        <div class="sheet-actions">
          <EButton variant="outline" @click="compare">Compare with…</EButton>
        </div>
      </section>
    </template>
  </ESheet>
</template>

<style scoped>
.gap {
  margin-block-end: var(--e-gap-base);
}
</style>
