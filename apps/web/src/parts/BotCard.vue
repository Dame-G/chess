<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import type { SeekMode, Speed } from "../api/types";
import { formatNumber, formatRelative, speedName, timeControl } from "../format";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import UiBadge from "../ui/UiBadge.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiField from "../ui/UiField.vue";
import UiKeyValue from "../ui/UiKeyValue.vue";
import UiKv from "../ui/UiKv.vue";
import UiLive from "../ui/UiLive.vue";
import UiMeter from "../ui/UiMeter.vue";
import UiSection from "../ui/UiSection.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";
import UiTable from "../ui/UiTable.vue";
import { botNow, botResults, botTally, botTrend, declineWords, rulesItems } from "./bot";
import BotSettingsDialog from "./BotSettingsDialog.vue";
import LineChart from "./LineChart.vue";

/**
 * The bot: Dameless on Lichess as its BOT account. The online switch, whether it is looking for games, how many of the
 * day's games against bots are used, the rules it takes challenges by (with an edit dialog) and the challenges it
 * answered lately. Every control just calls its route; the person clicking is the one deciding.
 */
const connections = useConnections();
const games = useGames();

const conn = computed(() => connections.bot);

// ---- Now, and how it has done
/** The other side of one of the bot's games, as "Alpha-Bot (2251)": what Now says it is playing. */
function opponentOf(id: number): string | null {
  const g = games.list.find((x) => x.id === id);
  if (!g) return null;
  const them = g.white.player === "dameless" ? g.black : g.white;
  return them.rating ? `${them.name} (${them.rating})` : them.name;
}
const now = computed(() => botNow(conn.value, opponentOf));

const failedGames = ref(false);
const loadingGames = ref(true);
/** Brings the bot's games (they are Dameless's games on Lichess) so the results and the trend have something to say. */
async function readGames() {
  failedGames.value = false;
  try {
    await games.page({ player: "dameless", source: "lichess", limit: 60 });
  } catch {
    failedGames.value = true;
  } finally {
    loadingGames.value = false;
  }
}
onMounted(() => void readGames());
// A game ending (or starting) changes what there is to read.
watch(
  () => conn.value.playing.length,
  () => void readGames(),
);

const finished = computed(() => botResults(games.list, 60));
const recent = computed(() => finished.value.slice(0, 20));
const tally = computed(() => botTally(recent.value));
/** The speed it plays most, which is the rating worth following. */
const mainSpeed = computed<Speed | null>(() => {
  const counts = new Map<Speed, number>();
  for (const g of finished.value) counts.set(g.speed, (counts.get(g.speed) ?? 0) + 1);
  return [...counts.entries()].toSorted((a, b) => b[1] - a[1])[0]?.[0] ?? null;
});
const trend = computed(() => {
  const speed = mainSpeed.value;
  if (!speed) return null;
  return botTrend(
    finished.value,
    speed,
    (conn.value.account?.ratings as Partial<Record<Speed, { rating: number }>> | undefined)?.[speed]?.rating ?? null,
  );
});
const recentColumns = [
  { key: "result", label: "Result" },
  { key: "opponent", label: "Opponent" },
  { key: "game", label: "Game", hide: "sm" as const },
  { key: "when", label: "When", align: "end" as const },
];
const gameText = (g: { rated: boolean; speed: Speed; clock: Parameters<typeof timeControl>[0] }) =>
  `${g.rated ? "Rated" : "Casual"} ${speedName[g.speed].toLowerCase()} ${timeControl(g.clock)}`;
const bot = computed(() => conn.value.bot);
const online = computed(() => bot.value?.online === true);
const username = computed(() => conn.value.account?.username ?? null);
const rules = computed(() => (bot.value ? rulesItems(bot.value.settings) : []));
const editing = ref(false);
const busy = ref(false);
/** Why the last control did not work, said under the controls until the next try. */
const failure = ref<string | null>(null);
/** No version is saved: the server refuses to take the bot online until one is chosen. */
const noVersion = computed(() => !bot.value?.settings.version);

async function run(action: () => Promise<unknown>) {
  busy.value = true;
  failure.value = null;
  try {
    await action();
  } catch (e) {
    failure.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

const seekMode = computed({
  get: () => (bot.value?.seek.mode ?? "off") as SeekMode,
  set: (v: SeekMode) => void run(() => connections.setBotSeek(v)),
});
const seekOptions = computed(() =>
  [
    { value: "off", label: "Off" },
    { value: "once", label: "Find one game" },
    { value: "always", label: "Always" },
  ].map((o) => ({ ...o, value: o.value as SeekMode, disabled: !online.value || busy.value })),
);

const state = computed(() =>
  noVersion.value && !online.value ? "Choose the version it plays first" : online.value ? "Online" : "Offline",
);

const limit = computed(() => {
  const b = bot.value;
  if (!b) return null;
  const used = b.bot_games_today;
  return {
    used,
    reached: !!b.bot_limit_until,
    text: `${used} of 100`,
  };
});

interface Entry {
  id: string;
  verdict: string;
  tone: "win" | "neutral" | "info";
  who: string;
  what: string;
  why: string;
}
const entries = computed<Entry[]>(() => {
  const who = (o: { name: string; rating?: number | null }) => `${o.name}${o.rating ? ` (${o.rating})` : ""}`;
  const waiting: Entry[] = conn.value.challenges.map((c) => ({
    id: c.id,
    verdict: "Waiting",
    tone: "info",
    who: who(c.opponent),
    what: `${c.rated ? "Rated" : "Casual"} ${speedName[c.speed].toLowerCase()}${c.time ? ` ${timeControl(c.time)}` : ""}`,
    why: "Waiting for an answer",
  }));
  const answered: Entry[] = connections.recent
    .filter((e) => e.challenge.connection === "bot")
    .slice(0, 5)
    .map((e) => {
      const c = e.challenge;
      const accepted = e.event === "accepted";
      return {
        id: `${c.id}-${e.event}`,
        verdict: accepted
          ? "Accepted"
          : e.event === "canceled"
            ? "Withdrawn"
            : c.direction === "outgoing"
              ? "Declined by"
              : "Declined",
        tone: accepted ? "win" : "neutral",
        who: who(c.opponent),
        what: `${c.rated ? "Rated" : "Casual"} ${speedName[c.speed].toLowerCase()}${c.time ? ` ${timeControl(c.time)}` : ""}`,
        why: accepted ? "" : declineWords(e.reason),
      };
    });
  return [...waiting, ...answered];
});
const columns = [
  { key: "verdict", label: "Answer" },
  { key: "who", label: "Challenger" },
  { key: "what", label: "Game", hide: "sm" as const },
  { key: "why", label: "Why", hide: "sm" as const },
];
</script>

<template>
  <div id="d-bot" class="bot">
    <UiEmpty v-if="connections.loaded && !conn.configured" dense title="Dameless has no BOT account token yet.">
      <template #actions>
        <ELink :link-component="RouterLink" to="/settings?tab=accounts">Add one in Settings</ELink>
      </template>
    </UiEmpty>
    <template v-else-if="bot">
      <UiSection title="Now">
        <p class="now" :class="`now--${now.tone}`" role="status">
          <UiLive v-if="now.tone === 'live'" label="Live" :pulse="now.game ?? 0" />
          <RouterLink v-if="now.game !== null" :to="{ path: '/', query: { game: String(now.game) } }" class="watch">
            {{ now.text }}<template v-if="now.more"> and {{ now.more }} more</template> · Watch
          </RouterLink>
          <template v-else>{{ now.text }}</template>
        </p>
      </UiSection>

      <UiSection title="On Lichess">
        <template #aside>
          <span v-if="username" class="who">
            <ELink :href="`https://lichess.org/@/${username}`" external>{{ username }}</ELink>
            <span v-if="connections.botRating" class="rating"> · {{ connections.botRating }} blitz</span>
          </span>
        </template>
        <div class="controls">
          <ESwitch
            :model-value="online"
            label="Play on Lichess"
            :description="state"
            :disabled="busy || (noVersion && !online)"
            @update:model-value="run(() => connections.setBotOnline($event))"
          />

          <ENotice v-if="failure" tone="warning" title="That did not work" role="alert">{{ failure }}</ENotice>

          <UiField group label="Looking for games">
            <UiSeg v-model="seekMode" label="Looking for games" :options="seekOptions" />
          </UiField>

          <UiMeter
            v-if="limit"
            label="Games against bots in the last day"
            :value="limit.used"
            :max="100"
            :text="limit.text"
            :tone="limit.reached ? 'warn' : undefined"
          />
        </div>
      </UiSection>

      <UiSection title="Recent games">
        <UiEmpty v-if="failedGames" dense tone="error" title="Its games did not load">
          <template #actions><EButton variant="outline" size="sm" @click="readGames">Try again</EButton></template>
        </UiEmpty>
        <UiEmpty
          v-else-if="!loadingGames && !recent.length"
          dense
          title="Its games on Lichess show here once it has played some."
        />
        <template v-else>
          <UiStats :columns="2">
            <UiStat
              :label="`Last ${recent.length} games`"
              :value="`${tally.w}–${tally.d}–${tally.l}`"
              sub="Won, drawn, lost"
            />
            <UiStat
              v-if="mainSpeed"
              :label="`${speedName[mainSpeed]} rating`"
              :value="trend?.rating.at(-1) ?? null"
              :delta="trend?.change ?? null"
              :sub="trend?.change != null ? `Since ${formatRelative(new Date(trend.x[0]! * 1000))}` : undefined"
            />
          </UiStats>
          <LineChart
            v-if="trend && trend.rating.length >= 3"
            :label="`Dameless on Lichess: its ${mainSpeed ? speedName[mainSpeed].toLowerCase() : ''} rating through its games`"
            :x="trend.x"
            :series="[{ label: 'Rating', values: trend.rating, color: '--c-ink', width: 2 }]"
            time
            :height="110"
            :format="(v) => formatNumber(Math.round(v))"
          />
          <UiTable
            label="Its recent games"
            :columns="recentColumns"
            :rows="recent.slice(0, 8)"
            row-key="id"
            :loading="loadingGames"
          >
            <template #cell-result="{ row }">
              <UiBadge :tone="row.result === 'W' ? 'win' : row.result === 'L' ? 'loss' : 'neutral'">
                {{ { W: "Won", L: "Lost", D: "Drew" }[row.result] }}
              </UiBadge>
            </template>
            <template #cell-opponent="{ row }">
              <RouterLink :to="{ path: '/', query: { game: String(row.id) } }" class="watch">
                {{ row.opponent }}<template v-if="row.rating"> ({{ row.rating }})</template>
              </RouterLink>
            </template>
            <template #cell-game="{ row }">{{ gameText(row) }}</template>
            <template #cell-when="{ row }">{{ formatRelative(row.at) }}</template>
          </UiTable>
        </template>
      </UiSection>

      <UiSection title="Rules">
        <template #aside>
          <EButton :variant="noVersion ? 'primary' : 'outline'" size="sm" @click="editing = true">
            <template v-if="noVersion">Choose a version</template>
            <template v-else>Edit rules</template>
          </EButton>
        </template>
        <p v-if="noVersion" class="line" role="status">
          No version chosen: the bot won't play. Choose the version that plays in its rules.
        </p>
        <UiKeyValue dense>
          <UiKv v-for="r in rules" :key="r.label" :label="r.label" :tone="r.tone">
            {{ r.value }}
          </UiKv>
        </UiKeyValue>
      </UiSection>

      <UiSection title="Recent challenges">
        <UiEmpty v-if="!entries.length" dense title="Challenges it answers show here, with why it said no." />
        <UiTable v-else label="Recent challenges" :columns="columns" :rows="entries" row-key="id">
          <template #cell-verdict="{ row }">
            <UiBadge :tone="row.tone">{{ row.verdict }}</UiBadge>
          </template>
          <template #cell-why="{ row }">{{ row.why || "–" }}</template>
        </UiTable>
      </UiSection>

      <BotSettingsDialog v-if="bot" v-model:open="editing" :settings="bot.settings" />
    </template>
  </div>
</template>

<style scoped>
.bot {
  min-width: 0;
}

.controls {
  display: grid;
  gap: var(--sp-3);
}

.who {
  font: var(--t-meta);
}

.rating {
  color: var(--c-ink-2);
}

.now {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  margin: 0;
  font: var(--t-body-strong);
}

.now--off,
.now--warn {
  color: var(--c-ink-2);
}

.watch {
  color: inherit;
  text-decoration: underline;
  text-decoration-color: var(--c-line-strong);
  text-underline-offset: 0.2em;
}

.watch:hover {
  text-decoration-color: currentcolor;
}

.line {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
