<script setup lang="ts">
import { onUnmounted, ref } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";
import { useQueryCache } from "@pinia/colada";
import { PhChartLine, PhCpu, PhFlask, PhHouse, PhPlugsConnected, PhWrench } from "@phosphor-icons/vue";
import { EAppFooter } from "@eos-ui/vue/components/EAppFooter";
import { EAppHeader } from "@eos-ui/vue/components/EAppHeader";
import { EAppShell } from "@eos-ui/vue/components/EAppShell";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENavList } from "@eos-ui/vue/components/ENavList";
import { dispatch, followEvents } from "./api/events";
import { useActivity } from "./activity";
import Logo from "./components/Logo.vue";
import RunnerDrawer from "./components/RunnerDrawer.vue";

const route = useRoute();
const items = [
  { label: "Overview", to: "/", icon: PhHouse },
  { label: "Analysis", to: "/analysis", icon: PhChartLine },
  { label: "Experiments", to: "/experiments", icon: PhFlask },
  { label: "Engines", to: "/engines", icon: PhWrench },
];
const isCurrent = (item: { to?: unknown }) =>
  item.to === "/" ? route.path === "/" : route.path.startsWith(String(item.to)) || isGame(item.to);
// A game belongs to the experiment it was played in.
const isGame = (to: unknown) => to === "/experiments" && route.path.startsWith("/tests/");

const { summary, status } = useActivity();
const runnerOpen = ref(false);

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
  <EAppShell>
    <template #header>
      <EAppHeader sticky brand-to="/" brand-label="Reckless Lab, home" :link-component="RouterLink">
        <template #brand><Logo /></template>
        <template #actions>
          <EButton
            variant="outline"
            size="sm"
            :icon="summary.state === 'offline' ? PhPlugsConnected : PhCpu"
            :class="['runner-button', `runner-button--${summary.state}`]"
            :aria-label="`Runner: ${summary.label}`"
            @click="runnerOpen = true"
          >
            <span class="runner-dot" aria-hidden="true"></span>
            <span class="runner-label">{{ summary.label }}</span>
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
      <EAppFooter>
        <p class="footer-note">
          Reckless Lab · a workbench for the
          <ELink href="https://github.com/codedeliveryservice/Reckless" external>Reckless</ELink> chess engine
          <template v-if="status.data"> · runner {{ status.data.version }}</template>
        </p>
        <nav aria-label="Links">
          <ELink href="https://github.com/Dame-G/chess" external>Source</ELink>
          <ELink href="https://github.com/Dame-G/reckless" external>Engine fork</ELink>
          <ELink href="https://eos.dame.gg" external>Built with eos</ELink>
        </nav>
      </EAppFooter>
    </template>
  </EAppShell>
  <RunnerDrawer v-model:open="runnerOpen" />
</template>

<style scoped>
.runner-button {
  max-width: 18rem;
}

.runner-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.runner-dot {
  flex: none;
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 50%;
  background: var(--e-muted);
}

.runner-button--idle .runner-dot {
  background: var(--e-success);
}

.runner-button--busy .runner-dot {
  background: var(--e-primary);
  animation: pulse 1.6s ease-in-out infinite;
}

.runner-button--offline .runner-dot {
  background: var(--e-danger);
}

@keyframes pulse {
  50% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .runner-button--busy .runner-dot {
    animation: none;
  }
}

@media (width < 40rem) {
  .runner-label {
    display: none;
  }
}

.footer-note {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

nav {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-base);
  font: var(--e-type-small);
}
</style>
