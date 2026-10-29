<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { PhArrowUpRight } from "@phosphor-icons/vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { Player, ProfileFilter } from "../api/types";
import ChallengeList from "../parts/ChallengeList.vue";
import DrillsCard from "../parts/DrillsCard.vue";
import ProfileFilters from "../parts/ProfileFilters.vue";
import ProfileHead from "../parts/ProfileHead.vue";
import ProfileNotice from "../parts/ProfileNotice.vue";
import ProfileOpenings from "../parts/ProfileOpenings.vue";
import ProfileStats from "../parts/ProfileStats.vue";
import ProfileView from "../parts/ProfileView.vue";
import ProfileWeakSpots from "../parts/ProfileWeakSpots.vue";
import RepertoireView from "../parts/RepertoireView.vue";
import ReviewBar from "../parts/ReviewBar.vue";
import { useUi } from "../shell/ui";
import { useConnections } from "../stores/connections";
import { useDrills } from "../stores/drills";
import { usePolicy } from "../stores/policy";
import { ALL_GAMES, usePlayers } from "../stores/players";
import { useProgress } from "../stores/progress";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSheetPage, { type SheetTab } from "../ui/UiSheetPage.vue";
import { useImport } from "./useImport";

/**
 * You: your Lichess ratings across the top, then your games added up in tabs. Overview is the figures, the review of
 * your games and how you do against Dameless over time; Openings the openings you have met and your own lines; Drills
 * your mistakes as practice and the positions to play out again; Challenges what waits for you on Lichess. Not linked to
 * Lichess yet: the way to log in comes first. No games at all: one page that says how to get some.
 */
const connections = useConnections();
const players = usePlayers();
const progress = useProgress();
const drills = useDrills();
const router = useRouter();
const toast = useToast();
const ui = useUi();
const policy = usePolicy();
const importing = useImport("you");

const tab = ref("overview");
const filter = ref<ProfileFilter>({ ...ALL_GAMES });
const failed = ref<string | null>(null);
const profile = computed(() => players.profileOf("you", filter.value));
/** No game at all with nothing filtered out: the first run, not an empty filter. */
const firstRun = computed(
  () => profile.value?.games === 0 && !filter.value.speeds?.length && filter.value.rated == null && !filter.value.since,
);
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

/** Without a Lichess login the sheet still names You, from the account if there is one. */
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
const lichessUrl = computed(() =>
  linked.value && player.value?.lichess ? `https://lichess.org/@/${player.value.lichess}` : null,
);
const subtitle = computed(() =>
  linked.value && player.value ? `${player.value.name} · Lichess` : "Not linked to Lichess",
);

const firstRunBody = computed(
  () =>
    importing.line.value ||
    `Play one against Dameless${linked.value ? ", or bring in your Lichess history" : ""}. Your profile, drills and openings are built from your games.`,
);
const loading = computed(() => !profile.value && !failed.value);
const tabs = computed<SheetTab[] | undefined>(() =>
  !profile.value || firstRun.value
    ? undefined
    : [
        { id: "overview", label: "Overview" },
        { id: "openings", label: "Openings" },
        { id: "drills", label: "Drills" },
        ...(challenges.value.length ? [{ id: "challenges", label: "Challenges", count: challenges.value.length }] : []),
      ],
);
/** Nothing reviewed yet, so the review is the first thing to do and leads the page. */
const reviewFirst = computed(() => profile.value?.reviewed === 0);
</script>

<template>
  <UiSheetPage v-model:tab="tab" title="You" :subtitle="subtitle" :tabs="tabs" wide :loading="loading">
    <template #actions>
      <EButton variant="quiet" size="sm" @click="compare">Compare</EButton>
      <EButton
        v-if="lichessUrl"
        variant="quiet"
        size="sm"
        :href="lichessUrl"
        external
        aria-label="On Lichess"
        title="On Lichess"
      >
        <PhArrowUpRight aria-hidden="true" />
      </EButton>
    </template>
    <template v-if="player && linked && !failed" #lead>
      <ProfileHead :player="player" />
    </template>

    <UiEmpty v-if="failed" tone="error" title="Your profile didn't load" :body="failed">
      <template #actions>
        <EButton variant="outline" @click="load">Try again</EButton>
      </template>
    </UiEmpty>
    <UiEmpty v-else-if="firstRun" title="No games yet" :body="firstRunBody">
      <template #actions>
        <EButton @click="ui.startNewGame()">New game</EButton>
        <EButton
          v-if="linked"
          variant="outline"
          :loading="importing.busy.value || !!importing.underway.value"
          :disabled="!policy.allowed"
          @click="importing.run()"
        >
          Import my Lichess games
        </EButton>
        <EButton v-else variant="outline" :loading="logging" @click="login">Log in with Lichess</EButton>
      </template>
    </UiEmpty>

    <template v-if="profile" #tab-overview>
      <ProfileFilters v-model="filter" :games="profile.games" whose="your" tabs />
      <ProfileNotice v-if="!linked">
        <p class="title">Your Lichess account is not linked</p>
        <p class="cost">Log in to see your ratings, play on Lichess from here and bring your games in.</p>
        <template #actions>
          <EButton :loading="logging" @click="login">Log in with Lichess</EButton>
        </template>
      </ProfileNotice>
      <UiEmpty v-if="profile.games === 0" dense title="No games match these filters." />
      <template v-else>
        <ReviewBar
          v-if="reviewFirst"
          player-key="you"
          name="You"
          mine
          lead
          :importable="linked"
          :quiet="!linked"
          :games="profile.games"
          :pending="profile.pending_review"
          :total="profile.unreviewed_total"
        />
        <ProfileStats :profile="profile" />
        <ReviewBar
          v-if="!reviewFirst"
          player-key="you"
          name="You"
          mine
          :importable="linked"
          :quiet="!linked"
          :games="profile.games"
          :pending="profile.pending_review"
          :total="profile.unreviewed_total"
        />
        <ProfileView :profile="profile" :progress="progress.chart" />
      </template>
    </template>

    <template v-if="profile" #tab-openings>
      <ProfileFilters v-model="filter" :games="profile.games" whose="your" tabs />
      <ProfileOpenings :profile="profile" />
      <RepertoireView />
    </template>

    <template v-if="profile" #tab-drills>
      <DrillsCard @review="tab = 'overview'" />
      <UiEmpty v-if="!drills.available" dense title="Drills are off while you play on Lichess." />
      <ProfileWeakSpots :profile="profile" />
    </template>

    <template #tab-challenges>
      <ChallengeList :challenges="challenges" connection="you" />
    </template>
  </UiSheetPage>
</template>
