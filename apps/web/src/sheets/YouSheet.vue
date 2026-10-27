<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import type { Player, ProfileFilter } from "../api/types";
import ChallengeList from "../parts/ChallengeList.vue";
import DrillsCard from "../parts/DrillsCard.vue";
import ProfileFilters from "../parts/ProfileFilters.vue";
import ProfileHead from "../parts/ProfileHead.vue";
import ProfileView from "../parts/ProfileView.vue";
import ProgressChart from "../parts/ProgressChart.vue";
import RepertoireView from "../parts/RepertoireView.vue";
import ReviewBar from "../parts/ReviewBar.vue";
import { useConnections } from "../stores/connections";
import { ALL_GAMES, usePlayers } from "../stores/players";
import { useProgress } from "../stores/progress";
import SheetFrame from "./SheetFrame.vue";

/**
 * You: your Lichess name and ratings, then your games added up (accuracy and rating over time, phases, results, moves,
 * openings, time, the best moments and worst blunders as links onto the board, and the weak spots to play out), what
 * is left to review, your challenges, and Compare with…. Not linked to Lichess yet: the way to log in comes first.
 */
const connections = useConnections();
const players = usePlayers();
const progress = useProgress();
const router = useRouter();
const toast = useToast();

const filter = ref<ProfileFilter>({ ...ALL_GAMES });
const failed = ref<string | null>(null);
const profile = computed(() => players.profileOf("you", filter.value));
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
  <SheetFrame title="You" description="Your games added up: how you play, what to work on, and how you compare.">
    <ENotice v-if="!linked" tone="info" title="Your Lichess account is not linked" class="gap">
      Log in to see your ratings, play on Lichess from here and bring your games in.
      <template #actions>
        <EButton :loading="logging" @click="login">Log in with Lichess</EButton>
      </template>
    </ENotice>

    <ProfileHead v-if="player" :player="player" />

    <section v-if="challenges.length" class="sec" aria-labelledby="y-ch">
      <h2 id="y-ch" class="part-title">Your challenges</h2>
      <ChallengeList :challenges="challenges" connection="you" />
    </section>

    <ENotice v-if="failed" tone="danger" title="Your profile did not load" class="gap">{{ failed }}</ENotice>
    <div v-else-if="!profile" class="loading" aria-busy="true">
      <ESkeleton :lines="4" />
    </div>
    <template v-else>
      <section class="sec" aria-label="Games and review">
        <ProfileFilters v-model="filter" />
        <ReviewBar
          player-key="you"
          name="You"
          mine
          :quiet="!linked"
          :games="profile.games"
          :pending="profile.pending_review"
        />
        <p v-if="profile.games === 0" class="sec-note">
          No games match. Play one against Dameless from New game{{
            linked ? "" : ", or log in to bring in your Lichess history"
          }}.
        </p>
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
  </SheetFrame>
</template>

<style scoped>
.gap {
  margin-block-end: var(--e-gap-base);
}

.loading {
  padding-block: var(--e-gap-base);
}
</style>
