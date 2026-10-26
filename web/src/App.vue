<script setup lang="ts">
import { computed, onUnmounted, ref, watch, watchEffect } from "vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import {
  PhCaretUp,
  PhCrown,
  PhClipboardText,
  PhSword,
  PhUser,
  PhClockCounterClockwise,
  PhCpu,
  PhFlask,
  PhGraph,
  PhKeyboard,
  PhMoon,
  PhMagnifyingGlass,
  PhPlugsConnected,
  PhPlus,
  PhSun,
  PhWrench,
  PhDesktop,
} from "@phosphor-icons/vue";
import { useShortcuts, useToast } from "@eos-ui/vue";
import { EAppFooter } from "@eos-ui/vue/components/EAppFooter";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EAppShell } from "@eos-ui/vue/components/EAppShell";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECommandPalette, type CommandItem } from "@eos-ui/vue/components/ECommandPalette";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENavList } from "@eos-ui/vue/components/ENavList";
import { EToastProvider } from "@eos-ui/vue/components/EToastProvider";
import { dispatch, followEvents } from "./api/events";
import { applyRunnerEvent } from "./api/cache";
import { useExperiments, useTunes, useVersions, versionPath } from "./api/queries";
import { useActivity } from "./activity";
import { ago, verdict } from "./format";
import { statusWord } from "./components/tune/tune";
import { recordChallenge } from "./lichess";
import { stored } from "./stored";
import Logo from "./components/Logo.vue";
import RunnerDrawer from "./components/RunnerDrawer.vue";
import Shortcuts from "./components/Shortcuts.vue";
import ThemeSwitch from "./components/ThemeSwitch.vue";
import { choice, type Choice } from "./scheme";

const route = useRoute();
const router = useRouter();
const items = [
  { label: "Board", to: "/board", icon: PhGraph },
  { label: "Improve", to: "/improve", icon: PhFlask },
  { label: "Versions", to: "/versions", icon: PhWrench },
  { label: "Lichess", to: "/lichess", icon: PhCrown },
];
const isCurrent = (item: { to?: unknown }) =>
  route.path.startsWith(String(item.to)) ||
  (item.to === "/board" && (route.path.startsWith("/games/") || route.path.startsWith("/matches/")));

/** Workspaces fill the window and scroll inside their panes; the cover page scrolls like a page. */
const workspace = computed(() => route.meta.workspace === true);

const { summary, queued } = useActivity();
const runnerOpen = ref(false);
const shortcutsOpen = ref(false);
const palette = ref(false);

useShortcuts({ "?": () => (shortcutsOpen.value = true) });

// ---- Runner connection: say so when it drops and when it is back, once.
const toast = useToast();
let seenOnline = false;
watch(
  () => summary.value.state,
  (state, before) => {
    if (state === "offline") {
      if (seenOnline)
        toast.show({
          key: "runner",
          title: "Runner disconnected",
          message: "Start it again with chess serve.",
          tone: "warning",
        });
    } else if (state !== "pending") {
      if (before === "offline" && seenOnline)
        toast.show({ key: "runner", title: "Runner reconnected", tone: "success" });
      seenOnline = true;
    }
  },
);

// ---- Titles: the place, then what is open in it.
const { state: experiments } = useExperiments();
const { state: versions } = useVersions();
const { state: tunes } = useTunes();
watchEffect(() => {
  const id = Number(route.params.id);
  let what = route.meta.title;
  if (route.name === "improve-test") what = experiments.value.data?.find((e) => e.id === id)?.name ?? what;
  if (route.name === "improve-tune") what = tunes.value.data?.find((t) => t.id === id)?.name ?? what;
  if (route.name === "version" && route.params.name) what = `${String(route.params.name)} · Versions`;
  document.title = what ? `${what} · Dameless` : "Dameless";
});

// ---- Command palette: places, actions, what you opened last, then everything else.
const recent = stored<string[]>("recent", []);
watch(
  () => route.path,
  (path) => {
    if (!/^\/(improve\/(experiments|tunes)\/\d+|versions\/[^/]+)$/.test(path)) return;
    recent.value = [path, ...recent.value.filter((p) => p !== path)].slice(0, 4);
  },
  { immediate: true },
);

/** A touch screen has no keyboard to take shortcuts from. */
const touch = matchMedia("(hover: none) and (pointer: coarse)").matches;

type Exp = NonNullable<typeof experiments.value.data>[number];
type Tune = NonNullable<typeof tunes.value.data>[number];
type Version = NonNullable<typeof versions.value.data>[number];
const experimentItem = (e: Exp, group: string): CommandItem => ({
  id: `go:/improve/experiments/${e.id}`,
  label: e.name,
  description: `${verdict(e).label} · ${e.dev} vs ${e.base}`,
  group,
  keywords: [e.dev, e.base, "test", "experiment"],
});
const tuneItem = (t: Tune, group: string): CommandItem => ({
  id: `go:/improve/tunes/${t.id}`,
  label: t.name,
  description: `Tune · ${statusWord(t)} · ${ago(t.created_at)}`,
  group,
  keywords: [t.ref, "tune", "spsa"],
});
const versionItem = (v: Version, group: string): CommandItem => ({
  id: `go:${versionPath(v.name)}`,
  label: v.name,
  description: `Version · ${v.subject} · ${ago(v.date)}`,
  group,
  keywords: [v.sha, "branch", "build", "version"],
});

const commands = computed<CommandItem[]>(() => {
  const exps = experiments.value.data ?? [];
  const tns = tunes.value.data ?? [];
  const vers = versions.value.data ?? [];
  // What you opened last comes first, found among everything, not only what the lists below show.
  const everything = [
    ...exps.map((e) => experimentItem(e, "Tests")),
    ...tns.map((t) => tuneItem(t, "Tunes")),
    ...vers.map((v) => versionItem(v, "Versions")),
  ];
  const recents = recent.value.flatMap((path) => {
    const item = everything.find((i) => i.id === `go:${path}`);
    return item ? [{ ...item, group: "Recent", icon: PhClockCounterClockwise }] : [];
  });
  const seen = new Set(recents.map((r) => r.id));
  return [
    ...recents,
    { id: "go:/board", label: "Open the board", group: "Go to", icon: PhGraph, keywords: ["analysis", "analyse"] },
    {
      id: "go:/improve",
      label: "Open Improve",
      group: "Go to",
      icon: PhFlask,
      keywords: ["lab", "experiments", "tests", "tune", "spsa"],
    },
    {
      id: "go:/versions",
      label: "Open Versions",
      group: "Go to",
      icon: PhWrench,
      keywords: ["engines", "builds", "branches"],
    },
    { id: "go:/lichess", label: "Open Lichess", group: "Go to", icon: PhCrown, keywords: ["bot", "online", "play"] },
    {
      id: "go:/improve?new=test",
      label: "New test",
      group: "Do",
      icon: PhPlus,
      keywords: ["test a change", "experiment", "branch"],
    },
    {
      id: "go:/improve?new=tune",
      label: "New tune",
      group: "Do",
      icon: PhPlus,
      keywords: ["tune numbers", "spsa", "parameters", "tuning"],
    },
    { id: "go:/board?paste=1", label: "Load a game or FEN", group: "Do", icon: PhClipboardText, keywords: ["pgn"] },
    {
      id: "go:/board?mode=play",
      label: "Play a game",
      group: "Do",
      icon: PhUser,
      keywords: ["engine", "versus", "human", "chess"],
    },
    {
      id: "go:/board?mode=engines",
      label: "Watch two engines play",
      group: "Do",
      icon: PhSword,
      keywords: ["engine", "play", "versus", "live"],
    },
    {
      id: "go:/lichess?challenge=1",
      label: "Challenge a bot",
      group: "Do",
      icon: PhCrown,
      keywords: ["lichess", "play", "opponent"],
    },
    { id: "runner", label: "Show the runner", group: "Do", icon: PhCpu, keywords: ["jobs", "queue", "log"] },
    ...(touch
      ? []
      : [
          {
            id: "shortcuts",
            label: "Keyboard shortcuts",
            group: "Do",
            icon: PhKeyboard,
            shortcut: "?",
            keywords: ["keys", "help"],
          },
        ]),
    ...(["system", "light", "dark"] as const).map((value) => ({
      id: `theme:${value}`,
      label: `Theme: ${value[0]!.toUpperCase()}${value.slice(1)}`,
      group: "Do",
      icon: { system: PhDesktop, light: PhSun, dark: PhMoon }[value],
      keywords: ["colour", "color", "scheme", "appearance", "dark mode", "light mode"],
    })),
    ...[
      ...exps.slice(0, 30).map((e) => experimentItem(e, "Tests")),
      ...tns.slice(0, 15).map((t) => tuneItem(t, "Tunes")),
      ...vers
        .filter((v) => !v.hidden)
        .slice(0, 20)
        .map((v) => versionItem(v, "Versions")),
    ].filter((i) => !seen.has(i.id)),
  ];
});
function run(item: CommandItem) {
  if (item.id === "runner") runnerOpen.value = true;
  else if (item.id === "shortcuts") shortcutsOpen.value = true;
  else if (item.id.startsWith("theme:")) choice.value = item.id.slice(6) as Choice;
  else if (item.id.startsWith("go:")) void router.push(item.id.slice(3));
}

// One live stream for the whole app: the cache follows it, then whatever listens for an event kind gets it.
const cache = useQueryCache();
const stop = followEvents((event) => {
  applyRunnerEvent(cache, event);
  if (event.kind === "lichess.challenge") recordChallenge(event.data);
  dispatch(event);
});
onUnmounted(stop);
</script>

<template>
  <EToastProvider>
    <EAppShell :class="['shell', { 'shell--workspace': workspace }]">
      <template #header>
        <EAppHeader class="top" brand-to="/" brand-label="Dameless, home" :link-component="RouterLink">
          <template #brand><Logo /></template>
          <template #logo>
            <ENavList
              class="places"
              label="Main"
              orientation="horizontal"
              :link-component="RouterLink"
              :items="items"
              :is-current="isCurrent"
            />
          </template>
          <template #actions>
            <EButton
              variant="quiet"
              size="sm"
              :icon="PhMagnifyingGlass"
              class="search"
              title="Search (Ctrl K)"
              @click="palette = true"
            >
              <span class="search-label">Search</span>
              <EKbd class="search-kbd" size="sm">Ctrl K</EKbd>
            </EButton>
            <ThemeSwitch class="theme" />
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
            title="Show the runner: jobs, queue and output"
            :aria-label="`Runner: ${[summary.label, ...summary.detail].join(', ')}. Show the runner.`"
            @click="runnerOpen = true"
          >
            <span class="status-dot" aria-hidden="true"></span>
            <span class="status-label">{{ summary.label }}</span>
            <span v-if="summary.detail.length" class="status-detail">{{ summary.detail.join(" · ") }}</span>
            <span v-if="queued" class="status-queue">{{ queued }} queued</span>
            <PhCaretUp class="status-caret" aria-hidden="true" />
          </EButton>
          <span class="status-end">
            <EButton
              variant="quiet"
              size="sm"
              :icon="PhKeyboard"
              class="status-keys"
              title="Keyboard shortcuts (?)"
              @click="shortcutsOpen = true"
            >
              Shortcuts
            </EButton>
            <ELink href="https://github.com/Dame-G/dameless" external>Source</ELink>
            <ELink href="https://github.com/Dame-G/reckless" external>Engine</ELink>
          </span>
        </EAppFooter>
      </template>
    </EAppShell>
    <ECommandPalette
      v-model:open="palette"
      :items="commands"
      label="Search Dameless"
      placeholder="Search tests, tunes, versions, actions"
      shortcut="mod+k"
      @select="run"
    />
    <RunnerDrawer v-model:open="runnerOpen" />
    <Shortcuts v-model:open="shortcutsOpen" />
  </EToastProvider>
</template>

<style scoped>
/* A workspace is the whole window: header, panes, status bar. Nothing outside the panes scrolls. */
.shell--workspace {
  --e-shell-width: 100%;

  height: 100dvh;
  min-height: 0;
}

/* A workspace is dense but not cramped: 40px controls where a pointer is precise. Touch keeps eos's targets. */
@media (pointer: fine) {
  .shell--workspace {
    --e-control-height: 2.5rem;
    --e-tap: 2.25rem;
  }
}

.shell--workspace :deep(.e-shell-body) {
  min-height: 0;
  padding: 0;
}

.shell--workspace :deep(.e-shell-main) {
  min-height: 0;
}

@media (width < 75rem) {
  .shell--workspace {
    height: auto;
    min-height: 100dvh;
  }
}

/* One slim row: logo, the places, search. */
.top {
  --e-control-height: 2.25rem;
  --e-tap: 2.25rem;
}

.top :deep(.e-appheader-bar) {
  min-block-size: 3rem;
  gap: var(--e-gap-loose);
}

.top :deep(.e-appheader-logo) {
  gap: var(--e-gap-section);
  font: var(--e-type-body);
}

.top :deep(.e-appheader-logo .e-nav > ul) {
  gap: var(--e-gap-tight);
  flex-wrap: nowrap;
}

/* The current place is underlined in the brand colour: crisp, and readable at a glance. */
.top :deep(.e-nav-item) {
  padding-inline: var(--e-pad-control-sm);
  border-color: transparent;
  font-weight: 500;
}

.top :deep(.e-nav-item[aria-current="page"]) {
  border-color: transparent;
  background: var(--e-primary-soft);
  box-shadow: inset 0 -0.125rem 0 var(--e-primary);
  color: var(--e-primary);
  font-weight: 600;
}

@media (width < 36rem) {
  .top :deep(.e-nav-item .e-icon) {
    display: none;
  }

  .top :deep(.e-appheader-logo) {
    gap: var(--e-gap-base);
  }

  /* Four places fit a phone only when each is as wide as its word. */
  .top :deep(.e-nav-item) {
    padding-inline: var(--e-gap-snug);
    font-size: var(--e-text-sm);
  }

  .top :deep(.e-appheader-bar) {
    gap: var(--e-gap-base);
  }
}

/* The theme is one command away in Search: on a phone the header keeps room for the four places. */
@media (width < 30rem) {
  .top :deep(.theme) {
    display: none;
  }

  .top :deep(.e-nav-item) {
    padding-inline: var(--e-gap-tight);
    font-size: var(--e-text-xs, 0.75rem);
  }

  .top :deep(.e-appheader-bar),
  .top :deep(.e-appheader-logo),
  .top :deep(.e-appheader-logo .e-nav > ul) {
    gap: var(--e-gap-tight);
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

/* The footer is a status bar, like a media player's: one slim line of 40px, the runner on the left, small print on the right. */
.status {
  --status-height: 2rem;

  /* The shell is a grid: without this a long status ("Live match · main vs main · ...") sets the width of the whole page. */
  min-inline-size: 0;
  min-block-size: 2.5rem;
  padding-block: var(--e-gap-tight);
  box-sizing: border-box;
  font: var(--e-type-small);
}

.status :deep(.e-shell-inner) {
  gap: var(--e-gap-snug) var(--e-gap-loose);
  flex-wrap: nowrap;
}

.status :deep(.e-btn),
.status-runner {
  min-block-size: var(--status-height);
  block-size: var(--status-height);
}

.status-runner {
  gap: var(--e-gap-snug);
  min-inline-size: 0;
  max-inline-size: min(48rem, 100%);
  padding-inline: var(--e-pad-control-sm);
  font-family: var(--e-font-mono);
  font-variant-numeric: tabular-nums;
}

.status-runner :deep(.e-btn-label),
.status-runner :deep(.e-btn-text) {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  min-inline-size: 0;
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

.status-detail::before,
.status-queue::before {
  margin-inline-end: var(--e-gap-snug);
  content: "\00b7";
}

.status-caret {
  flex: none;
  color: var(--e-muted);
  opacity: 0;
  transition: opacity 0.12s;
}

.status-runner:hover .status-caret,
.status-runner:focus-visible .status-caret {
  opacity: 1;
}

/* No hover on a touch screen: the caret is always there, and the bar is a proper tap target. */
@media (pointer: coarse) {
  .status {
    --status-height: 2.75rem;

    min-block-size: 3.25rem;
  }

  .status-caret {
    opacity: 1;
  }
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
  flex: none;
  align-items: center;
  gap: var(--e-gap-loose);
  margin-inline-start: auto;
  white-space: nowrap;
}

@media (width < 56rem) {
  .status-keys {
    display: none;
  }
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

  .status-caret {
    transition: none;
  }
}
</style>
