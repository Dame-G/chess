<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import Workspace from "../components/Workspace.vue";
import BotSettings from "../components/lichess/BotSettings.vue";
import ChallengeForm from "../components/lichess/ChallengeForm.vue";
import ChallengeLog from "../components/lichess/ChallengeLog.vue";
import GameRows from "../components/lichess/GameRows.vue";
import MeAccount from "../components/lichess/MeAccount.vue";
import MeChallenges from "../components/lichess/MeChallenges.vue";
import MeImport from "../components/lichess/MeImport.vue";
import MeLogin from "../components/lichess/MeLogin.vue";
import MePlay from "../components/lichess/MePlay.vue";
import SeekPanel from "../components/lichess/SeekPanel.vue";
import SetupPanel from "../components/lichess/SetupPanel.vue";
import { api } from "../api/runner";
import { unwrap, useLichess, useLichessGames, useLichessMe, type GameFilter } from "../api/queries";
import { liveGame, type LichessStatus, type MeStatus } from "../lichess";
import { stored } from "../stored";

const route = useRoute();
const router = useRouter();
const cache = useQueryCache();
const toast = useToast();
const { state: lichess } = useLichess();
const { state: meState } = useLichessMe();

// ---- Two identities: you, and your bot. The choice lives in the address (`?as=bot`).
const who = computed<"me" | "bot">(() => (route.query.as === "bot" ? "bot" : "me"));
const identities = [
  { value: "me", label: "You" },
  { value: "bot", label: "Your bot" },
];
const pick = computed({
  get: () => who.value,
  set: (v: string) => {
    const query = { ...route.query, as: v === "bot" ? "bot" : undefined };
    void router.replace({ query });
  },
});

// ---- Games: both accounts, or one.
const filter = stored<GameFilter>("lichess.games", "all");
const filters = [
  { value: "all", label: "All" },
  { value: "me", label: "Yours" },
  { value: "bot", label: "Bot" },
];
const { state: games } = useLichessGames(filter);
const { state: myGames } = useLichessGames("me");
const me = computed(() => (meState.value.data?.configured ? meState.value.data : undefined));

// ---- Coming back from Lichess's login page: say how it went, then clean the address.
const loginProblem = ref("");
function returned() {
  const { lichess: outcome, message, ...rest } = route.query;
  if (outcome === undefined) return;
  if (outcome === "connected") {
    toast.show({ key: "lichess-login", title: "Logged in with Lichess", tone: "success" });
    void cache.invalidateQueries({ key: ["lichess-me"] });
  } else loginProblem.value = typeof message === "string" && message ? message : "Lichess did not confirm the login.";
  void router.replace({ query: rest });
}
onMounted(returned);
watch(() => route.query.lichess, returned);

const loggingOut = ref(false);
const outBusy = ref(false);
const meProblem = ref("");
async function logout() {
  outBusy.value = true;
  meProblem.value = "";
  try {
    cache.setQueryData<MeStatus>(["lichess-me"], await unwrap(api.DELETE("/api/lichess/me")));
    loggingOut.value = false;
  } catch (e) {
    meProblem.value = (e as Error).message;
  } finally {
    outBusy.value = false;
  }
}
const opponent = computed(() => liveGame(me.value, myGames.value.data)?.opponent);
const incomingCount = computed(() => me.value?.challenges.incoming.length ?? 0);

const status = computed(() => lichess.value.data);
/** A bot account is connected: the page shows the bot, otherwise the steps to get one. */
const bot = computed(() => (status.value?.configured && status.value.account?.bot ? status.value : undefined));
const account = computed(() => bot.value?.account);
const botName = computed(() => account.value?.username ?? "");

// ---- The switch: one control for online and offline, answered by the runner's state.
const switching = ref(false);
const problem = ref("");
async function setOnline(on: boolean) {
  switching.value = true;
  problem.value = "";
  try {
    const next = await unwrap<LichessStatus>(api.POST(on ? "/api/lichess/start" : "/api/lichess/stop"));
    cache.setQueryData<LichessStatus>(["lichess"], next);
  } catch (e) {
    problem.value = (e as Error).message;
  } finally {
    switching.value = false;
  }
}
const online = computed({
  get: () => bot.value?.online ?? false,
  set: (on: boolean) => void setOnline(on),
});
const sentence = computed(() => {
  const b = bot.value;
  if (!b?.online) return "Not accepting challenges";
  if (b.state === "connecting") return "Connecting to Lichess…";
  if (b.state === "error") return "Cannot reach Lichess, trying again";
  const n = b.playing.length;
  return n ? `Playing ${n === 1 ? "a game" : `${n} games`}` : "Accepting challenges";
});

async function disconnect() {
  problem.value = "";
  try {
    cache.setQueryData<LichessStatus>(["lichess"], await unwrap(api.DELETE("/api/lichess/token")));
  } catch (e) {
    problem.value = (e as Error).message;
  }
}

const gameList = computed(() => games.value.data ?? []);
const open = (id: number) => void router.push(`/matches/${id}`);
</script>

<template>
  <Workspace library-label="Lichess games" inspector-label="Challenges">
    <template #library-head>
      <strong class="head-title">Games</strong>
      <ESegmented v-model="filter" label="Show" hide-label size="sm" :options="filters" class="filter" />
    </template>
    <template #library>
      <ESkeleton v-if="games.status === 'pending'" :lines="5" label="Loading games" class="pad" />
      <EEmptyState v-else-if="!gameList.length" title="No games yet" icon="info" size="sm" bare class="pad">
        Games you and your bot play on Lichess are kept here to replay.
      </EEmptyState>
      <GameRows v-else :games="gameList" :show-account="filter === 'all'" @open="open" />
    </template>

    <template #stage-head>
      <ESegmented v-model="pick" label="Account" hide-label size="sm" :options="identities" />
      <template v-if="who === 'bot' && bot && account">
        <ELink :href="`https://lichess.org/@/${botName}`" external class="identity">
          {{ botName }}
        </ELink>
        <ESwitch v-model="online" label="Online" :description="sentence" :disabled="switching" class="power" />
      </template>
    </template>

    <div class="stage">
      <!-- You -->
      <template v-if="who === 'me'">
        <ESkeleton v-if="meState.status === 'pending' && !meState.data" :lines="6" label="Loading" class="pad" />
        <ENotice
          v-else-if="meState.status === 'error'"
          tone="warning"
          title="The runner does not know your Lichess account yet"
          :role="false"
        >
          {{ meState.error?.message }}
        </ENotice>
        <MeLogin v-else-if="!me" :problem="loginProblem" />
        <template v-else>
          <MeAccount :me="me" :opponent="opponent" />
          <ENotice v-if="meProblem" tone="danger" title="That did not work" :role="false">{{ meProblem }}</ENotice>
          <MePlay :me="me" />
        </template>
      </template>

      <!-- Your bot -->
      <template v-else>
        <ESkeleton v-if="lichess.status === 'pending' && !status" :lines="6" label="Loading" class="pad" />
        <ENotice
          v-else-if="lichess.status === 'error'"
          tone="warning"
          title="The runner does not know Lichess yet"
          :role="false"
        >
          {{ lichess.error?.message }}
        </ENotice>
        <SetupPanel v-else-if="status && !bot" :status="status" />
        <template v-else-if="bot">
          <ENotice
            v-if="bot.state === 'error' && bot.error"
            tone="danger"
            title="Lichess refused the connection"
            :role="false"
          >
            {{ bot.error }}
          </ENotice>
          <ENotice v-if="problem" tone="danger" title="That did not work" :role="false">{{ problem }}</ENotice>
          <SeekPanel :status="bot" />
          <BotSettings :settings="bot.settings" />
        </template>
      </template>
    </div>

    <template v-if="who === 'me' ? me : status?.configured" #stage-foot>
      <EButton v-if="who === 'me'" variant="quiet" size="sm" @click="loggingOut = true">Log out of Lichess</EButton>
      <EButton v-else variant="quiet" size="sm" @click="disconnect">Disconnect from Lichess</EButton>
    </template>

    <template v-if="who === 'me' ? me : bot" #inspector-head>
      <strong class="head-title">{{ who === "me" ? "Challenges" : "Challenge someone" }}</strong>
      <span v-if="who === 'me' && incomingCount" class="count num">{{ incomingCount }}</span>
    </template>
    <template v-if="who === 'me' ? me : bot" #inspector>
      <div v-if="who === 'me' && me" class="side">
        <MeChallenges :me="me" />
        <section class="log" aria-labelledby="lv-import">
          <h3 id="lv-import" class="part-title">Your games</h3>
          <MeImport :has-games="(myGames.data?.length ?? 0) > 0" />
        </section>
      </div>
      <div v-else-if="bot" class="side">
        <ChallengeForm :online="bot.online" />
        <section class="log" aria-labelledby="lv-log">
          <h3 id="lv-log" class="part-title">Recent challenges</h3>
          <ChallengeLog />
        </section>
      </div>
    </template>

    <EDialog
      v-model:open="loggingOut"
      title="Log out of Lichess?"
      description="Dameless forgets your login and revokes it at Lichess. Games you are playing there carry on without it."
      size="sm"
    >
      <template #actions>
        <EButton variant="danger" :loading="outBusy" @click="logout">Log out</EButton>
        <EButton variant="outline" @click="loggingOut = false">Cancel</EButton>
      </template>
    </EDialog>
  </Workspace>
</template>

<style scoped>
.head-title {
  font: var(--e-type-h5);
}

.filter {
  margin-inline-start: auto;
}

.count {
  margin-inline-start: auto;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.pad {
  padding: var(--e-gap-base) var(--ws-pad);
}

.identity {
  font: var(--e-type-h5);
}

.power {
  margin-inline-start: auto;
}

.stage {
  display: grid;
  align-content: start;
  gap: var(--e-gap-loose);
  padding: var(--e-gap-loose) var(--ws-pad);
}

.stage > :deep(*) {
  max-inline-size: 36rem;
}

.side {
  display: grid;
  align-content: start;
  gap: var(--e-gap-section);
  padding: var(--e-gap-loose) var(--ws-pad);
}

.log {
  display: grid;
  gap: var(--e-gap-base);
}
</style>
