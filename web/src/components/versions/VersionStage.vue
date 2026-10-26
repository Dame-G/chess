<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import { PhCrown, PhFlask, PhHammer, PhSlidersHorizontal, PhSword, PhUser } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { api } from "../../api/runner";
import { unwrap, useJobs, useLichess, useVersions, type Version } from "../../api/queries";
import type { LichessStatus } from "../../lichess";
import { useRunnerEvent } from "../../api/events";
import { ago, failure } from "../../format";
import BuildTable from "./BuildTable.vue";
import { liveBuilds, tuneBadge, verdictStatus } from "./versions";

const props = defineProps<{ version: Version }>();

const router = useRouter();
const cache = useQueryCache();
const { state: jobs } = useJobs();
const { state: lichess } = useLichess();
const { state: versions } = useVersions();

const plain = computed(() => !props.version.is_main && !props.version.upstream);
const built = computed(() => props.version.latest_build_id != null);
const verdict = computed(() => verdictStatus(props.version));
const tested = computed(() => props.version.verdict);

const outcome: Record<string, string> = {
  passed: "It passed every stage of the test, so it can be merged into main.",
  queued: "Its test is waiting for its turn.",
  running: "Its test is running.",
  cancelled: "Its test was stopped before it finished.",
  failed: "It failed a stage of the test. The test shows which one, and why.",
};

/** The headline: what the version's latest test found. */
const headline = computed(() => {
  const v = props.version;
  if (v.is_main)
    return {
      tone: "neutral",
      title: "The baseline every version is tested against",
      text: "When a change passes, it is merged into main.",
    } as const;
  if (v.upstream)
    return {
      tone: "neutral",
      title: "Upstream's branch",
      text: "It belongs to the Reckless project, not to this fork. You can still test it against main.",
    } as const;
  const t = tested.value;
  if (!t)
    return {
      tone: "neutral",
      title: "Not tested yet",
      text: "Test it against main to find out whether it plays stronger.",
    } as const;
  const s = verdict.value;
  const tone =
    s.tone === "success" ? "success" : s.tone === "danger" ? "danger" : s.tone === "info" ? "info" : "neutral";
  const title = `${s.label}${s.figure ? ` · ${s.figure}` : ""} against ${v.tested_against ?? "main"}`;
  return { tone, title, text: outcome[t.status] ?? "" } as const;
});

// ---- Builds in flight, and the last one that failed
const buildJobs = computed(() =>
  (jobs.value.data ?? []).filter((j) => j.spec.kind === "build" && j.spec.ref === props.version.name),
);
const building = computed(() => buildJobs.value.find((j) => j.status === "running" || j.status === "queued"));
const dismissed = ref(0);
const failed = computed(() => {
  const latest = buildJobs.value[0];
  if (!latest || latest.status !== "failed" || latest.id === dismissed.value) return undefined;
  const newest = props.version.builds[0];
  if (newest && newest.created_at > latest.created_at) return undefined;
  return latest;
});
const lastLine = ref("");
useRunnerEvent("job.log", (data) => {
  const { id, line } = data as { id: number; line: string };
  if (id === building.value?.id) lastLine.value = line.trim();
});

const error = ref("");
const busy = ref("");
async function build() {
  busy.value = "build";
  error.value = "";
  try {
    await unwrap(api.POST("/api/builds", { body: { ref: props.version.name } }));
    void cache.invalidateQueries({ key: ["jobs"] });
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = "";
  }
}

// ---- Quick actions: each opens the place that does it
const test = () => router.push({ path: "/improve", query: { new: "test", dev: props.version.name } });
const tune = () => router.push({ path: "/improve", query: { new: "tune", branch: props.version.name } });
const watchIt = () =>
  router.push({ path: "/board", query: { mode: "engines", white: props.version.name, black: "main" } });
const playIt = () => router.push({ path: "/board", query: { mode: "play", opponent: props.version.name } });

// ---- Lichess: the bot's engine is a setting, so it is asked before it changes
const asking = ref(false);
const bot = computed(() => lichess.value.data);
async function putOnLichess() {
  const settings = bot.value?.settings;
  if (!settings) return;
  busy.value = "lichess";
  error.value = "";
  try {
    const out = await unwrap(api.PUT("/api/lichess/settings", { body: { ...settings, build: props.version.name } }));
    cache.setQueryData<LichessStatus>(["lichess"], (old) => (old ? { ...old, settings: out } : old!));
    void cache.invalidateQueries({ key: ["versions"] });
    asking.value = false;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = "";
  }
}

/** Whether its fingerprint equals main's: the same search, so a test against main would play the same games. */
const sameAsMain = computed(() => {
  const main = versions.value.data?.find((v) => v.is_main);
  if (props.version.is_main || !main) return false;
  const mine = props.version.builds.find((b) => !b.spsa)?.bench ?? props.version.bench;
  const theirs = main.builds.find((b) => !b.spsa)?.bench ?? main.bench;
  return mine != null && theirs != null && mine === theirs;
});

type Action = "test" | "tune" | "watch" | "play" | "lichess" | "build";

/** The primary action is the next step of the loop for this version. */
const primary = computed<Action>(() => {
  const v = props.version;
  if (v.is_main) return built.value ? "play" : "build";
  if (v.verdict?.status === "passed" && built.value && !v.on_lichess) return "lichess";
  if (v.tunables && sameAsMain.value) return "tune";
  return "test";
});

/** The actions that apply, the primary first and always in the same order after it. */
const actions = computed(() => {
  const v = props.version;
  const all: {
    key: Action;
    label: string;
    icon: typeof PhFlask;
    show: boolean;
    run: () => void;
    disabled?: boolean;
  }[] = [
    { key: "test", label: "Test it", icon: PhFlask, show: !v.is_main, run: test },
    { key: "tune", label: "Tune it", icon: PhSlidersHorizontal, show: !!v.tunables, run: tune },
    { key: "watch", label: "Watch it play", icon: PhSword, show: !v.is_main && built.value, run: watchIt },
    { key: "play", label: "Play it", icon: PhUser, show: built.value, run: playIt },
    {
      key: "lichess",
      label: "Put it on Lichess",
      icon: PhCrown,
      show: built.value && !v.on_lichess,
      run: () => (asking.value = true),
      disabled: !bot.value?.configured,
    },
    { key: "build", label: "Build now", icon: PhHammer, show: !built.value, run: () => void build() },
  ];
  const shown = all.filter((a) => a.show);
  return [...shown.filter((a) => a.key === primary.value), ...shown.filter((a) => a.key !== primary.value)];
});
</script>

<template>
  <div class="stage">
    <ENotice v-if="error" tone="danger" title="That did not work" closable :role="false" @close="error = ''">
      {{ error }}
    </ENotice>

    <header class="intro">
      <h2 class="subject">{{ version.subject }}</h2>
      <p class="meta num">
        {{ version.sha.slice(0, 7) }} ·
        <span :title="new Date(version.date).toLocaleString('en')">{{ ago(version.date) }}</span>
      </p>
    </header>

    <ENotice :tone="headline.tone" :title="headline.title" :role="false">
      {{ headline.text }}
      <template v-if="tested && plain">
        <ELink :link-component="RouterLink" :to="`/improve/experiments/${tested.experiment}`"> Open the test </ELink>
      </template>
    </ENotice>

    <ul v-if="version.tune || version.on_lichess" class="facts">
      <li v-if="version.tune">
        <PhSlidersHorizontal aria-hidden="true" />
        <span>{{ tuneBadge(version) }}</span>
        <ELink :link-component="RouterLink" :to="`/improve/tunes/${version.tune.id}`">Open the tune</ELink>
      </li>
      <li v-if="version.on_lichess">
        <PhCrown aria-hidden="true" />
        <span>Plays on Lichess</span>
        <ELink :link-component="RouterLink" to="/lichess">Open the bot</ELink>
      </li>
    </ul>

    <section class="section" aria-labelledby="actions-title">
      <h3 id="actions-title" class="section-title">What next</h3>
      <div class="actions">
        <EButton
          v-for="a in actions"
          :key="a.key"
          :variant="a.key === primary ? 'primary' : 'outline'"
          :icon="a.icon"
          :disabled="a.disabled"
          :loading="a.key === 'build' && (busy === 'build' || !!building)"
          @click="a.run"
        >
          {{ a.label }}
        </EButton>
      </div>
      <p v-if="primary === 'tune' && !version.is_main" class="note">
        Its fingerprint equals main's, so it plays the same games until its numbers are tuned. Testing it now would show
        nothing.
      </p>
      <p v-if="!built && !building" class="note">
        Not built yet. Build it to play it, watch it play or put it on Lichess. A test builds it by itself.
      </p>
    </section>

    <ENotice
      v-if="building"
      tone="info"
      :title="`${building.status === 'queued' ? 'Waiting to build' : 'Building'} ${version.name}`"
      :role="false"
    >
      <EProgress v-if="building.status === 'running'" :label="`Building ${version.name}`" />
      <p v-if="building.status === 'running' && lastLine" class="line">{{ lastLine }}</p>
    </ENotice>
    <ENotice
      v-else-if="failed"
      tone="danger"
      :title="`The build of ${version.name} failed`"
      closable
      :role="false"
      @close="dismissed = failed.id"
    >
      {{ failure(failed.error, "build") }}
    </ENotice>

    <section v-if="version.builds.length" class="section" aria-labelledby="builds-title">
      <h3 id="builds-title" class="section-title">
        Builds
        <span class="count num">{{ liveBuilds(version).length }} on disk</span>
      </h3>
      <BuildTable :version="version" />
    </section>

    <EDialog v-model:open="asking" :title="`Put ${version.name} on Lichess?`" size="sm">
      The bot plays {{ version.name }} from its next game on, and always its newest build. You can change it back in the
      Lichess settings.
      <template #actions>
        <EButton :loading="busy === 'lichess'" @click="putOnLichess">Put it on Lichess</EButton>
        <EButton variant="outline" @click="asking = false">Cancel</EButton>
      </template>
    </EDialog>
  </div>
</template>

<style scoped>
.stage {
  display: grid;
  align-content: start;
  gap: var(--e-gap-section);
  inline-size: 100%;
  max-inline-size: 60rem;
  padding: var(--ws-pad, var(--e-pad-control));
  box-sizing: border-box;
}

.intro {
  display: grid;
  gap: var(--e-gap-tight);
}

.subject {
  margin: 0;
  font: var(--e-type-h4);
}

.meta {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.facts {
  display: grid;
  gap: var(--e-gap-snug);
  margin: 0;
  padding: 0;
  list-style: none;
}

.facts li {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug) var(--e-gap-base);
}

.facts svg {
  flex: none;
  color: var(--e-primary);
}

.section {
  display: grid;
  gap: var(--e-gap-base);
  min-inline-size: 0;
}

.section-title {
  display: flex;
  align-items: baseline;
  gap: var(--e-gap-base);
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.count {
  font-weight: 400;
  letter-spacing: 0;
  text-transform: none;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-base);
}

.note {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.line {
  overflow: hidden;
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
