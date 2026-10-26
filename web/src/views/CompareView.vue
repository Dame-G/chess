<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import { PhList } from "@phosphor-icons/vue";
import { useToast } from "@eos-ui/vue";
import { ECheckbox } from "@eos-ui/vue/components/ECheckbox";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import "../components/compare/compare.css";
import Workspace from "../components/Workspace.vue";
import { toLibrary } from "../components/improve/libraryNav";
import AddPlayer from "../components/compare/AddPlayer.vue";
import CompareFilters from "../components/compare/CompareFilters.vue";
import CompareSheet from "../components/compare/CompareSheet.vue";
import IdentitySwitch from "../components/compare/IdentitySwitch.vue";
import MomentsPane from "../components/compare/MomentsPane.vue";
import PlayerEntry from "../components/compare/PlayerEntry.vue";
import TopPlayers from "../components/compare/TopPlayers.vue";
import { readState, reviewCost, stateQuery } from "../components/compare/compare";
import { useReviewProgress } from "../components/compare/progress";
import {
  importPlayerGames,
  reviewPlayerGames,
  useCompare,
  useJobs,
  useLichess,
  useLichessMe,
  type CompareQuery,
} from "../api/queries";

const route = useRoute();
const router = useRouter();
const cache = useQueryCache();
const toast = useToast();

// ---- What is compared lives in the address, so a comparison can be bookmarked.
const state = computed(() => readState(route.query));
function change(patch: Partial<CompareQuery>) {
  void router.replace({ query: { ...route.query, ...stateQuery({ ...state.value, ...patch }) } });
}
const speeds = computed({ get: () => [...state.value.speeds], set: (v: string[]) => change({ speeds: v }) });
const rated = computed({ get: () => state.value.rated, set: (v: boolean) => change({ rated: v }) });
const limit = computed({ get: () => String(state.value.limit), set: (v: string) => change({ limit: Number(v) }) });
const withBot = computed({
  get: () => state.value.players.includes("bot"),
  set: (on: boolean) =>
    change({ players: on ? [...state.value.players, "bot"] : state.value.players.filter((p) => p !== "bot") }),
});
const named = computed(() => state.value.players.filter((p) => p !== "me" && p !== "bot"));
const add = (username: string) => {
  if (!state.value.players.some((p) => p.toLowerCase() === username.toLowerCase()))
    change({ players: [...state.value.players, username] });
};
const remove = (username: string) => change({ players: state.value.players.filter((p) => p !== username) });

// ---- Who can be asked for.
const { state: meState } = useLichessMe();
const { state: botState } = useLichess();
const me = computed(() => (meState.value.data?.configured ? meState.value.data : undefined));
const hasBot = computed(() => !!(botState.value.data?.configured && botState.value.data.account?.bot));
const locked = computed(() => meState.value.data?.fair_play_lock === true);
const asked = computed<CompareQuery>(() => ({
  ...state.value,
  players: state.value.players.filter((p) => (p === "me" ? !!me.value : p === "bot" ? hasBot.value : true)),
}));

const { state: compare } = useCompare(asked);
const players = computed(() => compare.value.data?.players ?? []);
const byId = (id: string) => players.value.find((p) => p.player.toLowerCase() === id.toLowerCase());
const mine = computed(() => byId("me"));
const reviewedAny = computed(() => players.value.some((p) => p.reviewed > 0));

// ---- Imports and reviews running now.
const { state: jobs } = useJobs();
const progress = useReviewProgress();
const live = (status: string) => status === "queued" || status === "running";
const importing = computed(
  () =>
    new Set(
      (jobs.value.data ?? []).flatMap((j) =>
        j.spec.kind === "lichess_player_import" && live(j.status) ? [j.spec.username.toLowerCase()] : [],
      ),
    ),
);
/** The review running now, else the next one waiting. */
const review = computed(() => {
  const reviews = (jobs.value.data ?? []).filter((j) => j.spec.kind === "review" && live(j.status));
  return reviews.find((j) => j.status === "running") ?? reviews.at(-1);
});
const reviewing = computed(() => !!review.value);
const reviewTotal = computed(() => {
  const spec = review.value?.spec;
  const total = progress.value?.job === review.value?.id ? progress.value?.total : undefined;
  return total ?? (spec?.kind === "review" ? spec.matches.length : 0);
});
const reviewDone = computed(() => (progress.value?.job === review.value?.id ? progress.value!.done : 0));
const paused = computed(() => progress.value?.job === review.value?.id && progress.value?.paused === true);

const problem = ref("");
const busy = ref(false);
async function start(what: () => Promise<unknown>) {
  busy.value = true;
  problem.value = "";
  try {
    await what();
    await cache.invalidateQueries({ key: ["jobs"] });
  } catch (e) {
    problem.value = (e as Error).message;
    toast.show({ key: "compare-job", title: problem.value || "That did not start", tone: "danger" });
  } finally {
    busy.value = false;
  }
}
const importGames = (username: string) => start(() => importPlayerGames(username, state.value.limit, state.value));
const reviewGames = (who: string, n?: number) =>
  start(() => reviewPlayerGames(who, n ?? byId(who)?.pending_review ?? state.value.limit, state.value));

// ---- What to show.
const first = computed(() => compare.value.status === "pending" && !compare.value.data);
const onlyMe = computed(() => players.value.length === 1 && players.value[0]!.player === "me");
const empty = computed(() => !!me.value && onlyMe.value && !reviewedAny.value);
const myCost = computed(() => reviewCost(Math.min(state.value.limit, mine.value?.games ?? state.value.limit)));
const myCount = computed(() => Math.min(state.value.limit, mine.value?.games ?? state.value.limit));
</script>

<template>
  <Workspace class="cmp" library-label="Players" inspector-label="Moments">
    <template #library-head>
      <strong class="head-title">Players</strong>
    </template>
    <template #library>
      <div class="library">
        <ul class="entries" aria-label="Compared players">
          <PlayerEntry v-if="me" id="me" :stats="byId('me')" />
          <li v-else class="entry-note">
            <ELink :link-component="RouterLink" to="/lichess">Log in with Lichess</ELink> to put yourself in the
            comparison.
          </li>
          <PlayerEntry v-if="hasBot && withBot" id="bot" :stats="byId('bot')" @remove="withBot = false" />
          <PlayerEntry v-for="p in named" :id="p" :key="p.toLowerCase()" :stats="byId(p)" @remove="remove(p)" />
        </ul>
        <div class="part">
          <ECheckbox v-if="hasBot && !withBot" v-model="withBot" label="Compare with your bot" />
          <AddPlayer @add="add" />
        </div>
        <section class="part" aria-labelledby="cf-title">
          <h3 id="cf-title" class="part-title">Games counted</h3>
          <CompareFilters v-model:speeds="speeds" v-model:rated="rated" v-model:limit="limit" />
        </section>
        <TopPlayers :chosen="state.players" @add="add" />
      </div>
    </template>

    <template #stage-head>
      <IdentitySwitch />
      <span class="grow"></span>
      <EButton class="to-library" variant="quiet" size="sm" :icon="PhList" @click="toLibrary('Players')">
        Players
      </EButton>
    </template>

    <div class="stage">
      <ENotice v-if="locked" tone="info" title="Reviews wait while you play" :role="false">
        Analysis is off while you are playing on Lichess. Reviews and imports pick up again when your game ends.
      </ENotice>
      <div v-if="reviewing" class="running" role="status">
        <EProgress
          :label="
            paused ? 'Review paused while you play' : `Reviewing ${reviewTotal} ${reviewTotal === 1 ? 'game' : 'games'}`
          "
          :value="reviewDone"
          :max="reviewTotal || 1"
          :value-text="`${reviewDone} of ${reviewTotal}`"
        />
      </div>

      <ESkeleton v-if="first" :lines="8" label="Comparing" />
      <ENotice v-else-if="compare.status === 'error'" tone="warning" title="Could not compare" :role="false">
        {{ compare.error?.message }}
      </ENotice>
      <EEmptyState v-else-if="!me && !named.length && !hasBot" title="Nobody to compare yet" icon="info" size="sm" bare>
        Add a Lichess player from the list, or log in to compare your own games.
      </EEmptyState>
      <EEmptyState v-else-if="empty && reviewing" title="Reviewing games" icon="info" bare>
        The first numbers appear as soon as one game is done. Meanwhile, add someone from the list to compare with.
      </EEmptyState>
      <EEmptyState v-else-if="empty" title="Nothing reviewed yet" icon="info" bare>
        A review has the engine search every position of a game and judge each move, the way Lichess's analysis does.
        {{ myCost }} Then add someone from the list to compare with.
        <template #action>
          <EButton v-if="mine?.games" :loading="busy" :disabled="locked" @click="reviewGames('me', myCount)">
            Review my last {{ myCount }} games
          </EButton>
          <ELink v-else :link-component="RouterLink" to="/lichess">Import your games first</ELink>
        </template>
      </EEmptyState>
      <CompareSheet
        v-else-if="players.length"
        :players="players"
        :importing="importing"
        :reviewing="reviewing"
        :locked="locked"
        :limit="state.limit"
        @import="importGames"
        @review="reviewGames"
      />
    </div>

    <template #inspector-head>
      <strong class="head-title">Moments</strong>
    </template>
    <template #inspector>
      <MomentsPane :players="players" />
    </template>
  </Workspace>
</template>

<style scoped>
.head-title {
  font: var(--e-type-h5);
}

.grow {
  flex: 1;
}

.to-library {
  display: none;
}

@media (width < 75rem) {
  .to-library {
    display: inline-flex;
  }
}

.stage {
  display: grid;
  align-content: start;
  gap: var(--e-gap-loose);
  padding: var(--e-gap-loose) var(--ws-pad);
}

.running {
  max-inline-size: 28rem;
}

.library {
  display: grid;
  align-content: start;
  gap: var(--e-gap-section);
  padding-block: var(--e-gap-base) var(--e-gap-loose);
}

.library > :not(.entries) {
  padding-inline: var(--ws-pad);
}

.entries {
  margin: 0;
  padding: 0;
  list-style: none;
}

.entry-note {
  padding: var(--e-gap-snug) var(--ws-pad);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.part {
  display: grid;
  gap: var(--e-gap-base);
}
</style>
