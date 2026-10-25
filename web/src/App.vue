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

const { summary, status } = useActivity();
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
      <EAppHeader brand-to="/" brand-label="Reckless Lab, home" :link-component="RouterLink">
        <template #brand><Logo /></template>
        <template #actions>
          <EButton variant="quiet" size="sm" :icon="PhMagnifyingGlass" class="search" @click="palette = true">
            <span class="search-label">Search</span>
            <EKbd class="search-kbd">Ctrl K</EKbd>
          </EButton>
        </template>
        <ENavList
          label="Main"
          orientation="horizontal"
          :link-component="RouterLink"
          :items="items"
          :is-current="isCurrent"
        />
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
          :aria-label="`Runner: ${summary.label}. Show the runner.`"
          @click="runnerOpen = true"
        >
          <span class="status-dot" aria-hidden="true"></span>
          <span class="status-label">{{ summary.label }}</span>
        </EButton>
        <span v-if="status.data" class="status-meta">runner {{ status.data.version }}</span>
        <ELink href="https://github.com/Dame-G/chess" external>Source</ELink>
        <ELink href="https://github.com/Dame-G/reckless" external>Engine</ELink>
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

.search-label {
  color: var(--e-muted);
}

@media (width < 40rem) {
  .search-label,
  .search-kbd {
    display: none;
  }
}

/* The footer is a status bar: the runner on the left, small print on the right. */
.status {
  font: var(--e-type-small);
}

.status-runner {
  max-width: min(36rem, 70vw);
  font-family: var(--e-font-mono);
}

.status-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-dot {
  flex: none;
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 50%;
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

.status-meta {
  color: var(--e-muted);
  font-family: var(--e-font-mono);
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
