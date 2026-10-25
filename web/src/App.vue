<script setup lang="ts">
import { computed, onUnmounted, ref } from "vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import {
  PhCpu,
  PhFlask,
  PhGraph,
  PhHammer,
  PhMagnifyingGlass,
  PhPlugsConnected,
  PhPlus,
  PhClipboardText,
  PhWrench,
} from "@phosphor-icons/vue";
import { EAppFooter } from "@eos-ui/vue/components/EAppFooter";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EAppShell } from "@eos-ui/vue/components/EAppShell";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECommandPalette, type CommandItem } from "@eos-ui/vue/components/ECommandPalette";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENavList } from "@eos-ui/vue/components/ENavList";
import { dispatch, followEvents } from "./api/events";
import { useBuilds, useExperiments } from "./api/queries";
import { useActivity } from "./activity";
import { verdict } from "./format";
import Logo from "./components/Logo.vue";
import RunnerDrawer from "./components/RunnerDrawer.vue";

const route = useRoute();
const router = useRouter();
const items = [
  { label: "Board", to: "/board", icon: PhGraph },
  { label: "Lab", to: "/lab", icon: PhFlask },
  { label: "Engines", to: "/engines", icon: PhWrench },
];
const isCurrent = (item: { to?: unknown }) =>
  route.path.startsWith(String(item.to)) || (item.to === "/board" && route.path.startsWith("/games/"));

/** Workspaces fill the window and scroll inside their panes; the cover page scrolls like a page. */
const workspace = computed(() => route.meta.workspace === true);

const { summary, status, queued } = useActivity();
const runnerOpen = ref(false);

// One palette for the whole app: every place, every action, every experiment and engine.
const { state: experiments } = useExperiments();
const { state: builds } = useBuilds();
const commands = computed<CommandItem[]>(() => [
  { id: "go:/board", label: "Open the board", group: "Go to", icon: PhGraph, keywords: ["analysis", "analyse"] },
  { id: "go:/lab", label: "Open the lab", group: "Go to", icon: PhFlask, keywords: ["experiments", "tests"] },
  { id: "go:/engines", label: "Open engines", group: "Go to", icon: PhWrench, keywords: ["builds"] },
  { id: "go:/lab?new=1", label: "Test an idea", group: "Do", icon: PhPlus, keywords: ["new experiment"] },
  { id: "go:/board?paste=1", label: "Paste a game or FEN", group: "Do", icon: PhClipboardText, keywords: ["pgn"] },
  { id: "go:/engines?build=1", label: "Build an engine", group: "Do", icon: PhHammer, keywords: ["branch", "commit"] },
  { id: "runner", label: "Show the runner", group: "Do", icon: PhCpu, keywords: ["jobs", "queue", "log"] },
  ...(experiments.value.data ?? []).slice(0, 30).map((e) => ({
    id: `go:/lab/${e.id}`,
    label: e.name,
    description: `${verdict(e).label} · ${e.dev} vs ${e.base}`,
    group: "Experiments",
    keywords: [e.dev, e.base],
  })),
  ...(builds.value.data ?? []).slice(0, 20).map((b) => ({
    id: `go:/board?build=${b.id}`,
    label: `Analyse with ${b.ref} #${b.id}`,
    description: b.commit.slice(0, 10),
    group: "Engines",
    keywords: [b.commit],
  })),
]);
function run(item: CommandItem) {
  if (item.id === "runner") runnerOpen.value = true;
  else if (item.id.startsWith("go:")) router.push(item.id.slice(3));
}
const palette = ref(false);

// One live stream for the whole app. A restarted runner may have a new build or schema, so everything refetches.
const cache = useQueryCache();
const stop = followEvents((event) => {
  if (event.kind === "runner.started") cache.invalidateQueries();
  if (event.kind === "job.updated") {
    cache.invalidateQueries({ key: ["jobs"] });
    if ((event.data as { status?: string }).status === "done") cache.invalidateQueries({ key: ["builds"] });
  }
  if (event.kind === "experiment.updated") {
    const { id } = event.data as { id: number };
    cache.invalidateQueries({ key: ["experiments"] });
    cache.invalidateQueries({ key: ["experiment", id] });
  }
  if (event.kind === "test.progress") {
    const { id } = event.data as { id: number };
    cache.invalidateQueries({ key: ["test", id] });
    cache.invalidateQueries({ key: ["games", id] });
  }
  dispatch(event);
});
onUnmounted(stop);
</script>

<template>
  <EAppShell :class="['shell', { 'shell--workspace': workspace }]">
    <template #header>
      <EAppHeader class="top" brand-to="/" brand-label="Reckless Lab, home" :link-component="RouterLink">
        <template #brand><Logo /></template>
        <template #logo>
          <ENavList
            label="Main"
            orientation="horizontal"
            :link-component="RouterLink"
            :items="items"
            :is-current="isCurrent"
          />
        </template>
        <template #actions>
          <EButton variant="quiet" size="sm" :icon="PhMagnifyingGlass" class="search" @click="palette = true">
            <span class="search-label">Search</span>
            <EKbd class="search-kbd">Ctrl K</EKbd>
          </EButton>
        </template>
      </EAppHeader>
    </template>

    <RouterView />

    <template #footer>
      <EAppFooter class="status">
        <EButton
          variant="quiet"
          size="sm"
          :icon="summary.state === 'offline' ? PhPlugsConnected : PhCpu"
          :class="['status-runner', `status-runner--${summary.state}`]"
          :aria-label="`Runner: ${[summary.label, ...summary.detail].join(', ')}. Show the runner.`"
          @click="runnerOpen = true"
        >
          <span class="status-dot" aria-hidden="true"></span>
          <span class="status-label">{{ summary.label }}</span>
          <span v-if="summary.detail.length" class="status-detail">· {{ summary.detail.join(" · ") }}</span>
          <span v-if="queued" class="status-queue num">+{{ queued }} queued</span>
        </EButton>
        <span class="status-end">
          <span v-if="status.data" class="status-meta">runner {{ status.data.version }}</span>
          <ELink href="https://github.com/Dame-G/chess" external>Source</ELink>
          <ELink href="https://github.com/Dame-G/reckless" external>Engine</ELink>
        </span>
      </EAppFooter>
    </template>
  </EAppShell>
  <ECommandPalette
    v-model:open="palette"
    :items="commands"
    label="Search Reckless Lab"
    placeholder="Jump to an experiment, an engine or an action"
    shortcut="mod+k"
    @select="run"
  />
  <RunnerDrawer v-model:open="runnerOpen" />
</template>

<style scoped>
/* A workspace is the whole window: header, panes, status bar. Nothing outside the panes scrolls. */
.shell--workspace {
  --e-shell-width: 100%;

  height: 100dvh;
  min-height: 0;
}

/* A workspace is dense: controls and pane bars are 2rem tall where a pointer is precise. Touch keeps eos's targets. */
@media (pointer: fine) {
  .shell--workspace {
    --e-control-height: 2.25rem;
    --e-tap: 2rem;
  }
}

.shell--workspace :deep(.e-shell-body) {
  min-height: 0;
  padding: 0;
}

.shell--workspace :deep(.e-shell-main) {
  min-height: 0;
}

@media (width < 64rem) {
  .shell--workspace {
    height: auto;
    min-height: 100dvh;
  }
}

/* One slim row: logo, the three places, search. */
.top {
  --e-control-height: 2rem;
  --e-tap: 2rem;
}

.top :deep(.e-appheader-bar) {
  min-block-size: 2.75rem;
}

.top :deep(.e-appheader-logo) {
  gap: var(--e-gap-base);
  font: var(--e-type-body);
}

.top :deep(.e-appheader-logo .e-nav > ul) {
  flex-wrap: nowrap;
}

@media (width < 36rem) {
  .top :deep(.e-nav-item .e-icon) {
    display: none;
  }
}

.search-label {
  color: var(--e-muted);
}

@media (width < 40rem) {
  .search-label,
  .search-kbd {
    display: none;
  }
}

/* The footer is a status bar, like a media player's: one slim line, the runner on the left, small print on the right. */
.status {
  --status-height: 1.75rem;

  min-block-size: 0;
  padding-block: 0;
  font: var(--e-type-small);
}

.status :deep(.e-btn),
.status-runner {
  min-block-size: var(--status-height);
  block-size: var(--status-height);
}

.status-runner {
  max-inline-size: min(48rem, 100%);
  font-family: var(--e-font-mono);
}

.status-label {
  flex: none;
  font-weight: 600;
}

.status-label,
.status-detail {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-detail,
.status-queue {
  color: var(--e-muted);
}

.status-dot {
  flex: none;
  width: 0.5rem;
  height: 0.5rem;
  background: var(--e-muted);
}

.status-runner--idle .status-dot {
  background: var(--e-success);
}

.status-runner--busy .status-dot {
  background: var(--e-primary);
  animation: pulse 1.6s ease-in-out infinite;
}

.status-runner--offline .status-dot {
  background: var(--e-danger);
}

.status-end {
  display: flex;
  align-items: center;
  gap: var(--e-gap-base);
  margin-inline-start: auto;
}

.status-meta {
  color: var(--e-muted);
  font-family: var(--e-font-mono);
}

@media (width < 40rem) {
  .status-end {
    display: none;
  }
}

@keyframes pulse {
  50% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .status-runner--busy .status-dot {
    animation: none;
  }
}
</style>
