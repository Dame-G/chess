<script setup lang="ts">
import { computed } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { PhChartLine, PhFlask } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { EHeading } from "@eos-ui/vue/components/EHeading";
import { ELink } from "@eos-ui/vue/components/ELink";
import { EMetric } from "@eos-ui/vue/components/EMetric";
import { EStack } from "@eos-ui/vue/components/EStack";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { useActivity } from "../activity";
import { useBuilds, useExperiment, useExperiments } from "../api/queries";
import { ago, speed, stageLabel, verdict } from "../format";
import DecisionGauge from "../components/DecisionGauge.vue";
import HowItWorks from "../components/HowItWorks.vue";

const router = useRouter();
const { summary, running, experiment } = useActivity();
const { state: experiments } = useExperiments();
const { state: builds } = useBuilds();

// The experiment running now, with its live test.
const { state: live } = useExperiment(() => experiment.value?.id ?? 0);
const liveTest = computed(() => (experiment.value ? live.value.data?.tests.at(-1) : undefined));

const finished = computed(() => (experiments.value.data ?? []).filter((e) => e.finished_at).slice(0, 5));
const stronger = computed(
  () => (experiments.value.data ?? []).filter((e) => e.status === "passed" && e.kind === "gainer").length,
);
const fastest = computed(() => Math.max(0, ...(builds.value.data ?? []).map((b) => b.nps)));
</script>

<template>
  <EStack gap="section">
    <section class="hero">
      <p class="hero-kicker">Reckless Lab</p>
      <h1 class="hero-title">Make a chess engine stronger, <em>one tested idea at a time.</em></h1>
      <p class="hero-text">
        Reckless is one of the strongest chess engines in the world. Here you can explore positions with it, change its
        code, and let thousands of games decide whether each change really helps.
      </p>
      <div class="hero-actions">
        <EButton size="lg" :icon="PhChartLine" @click="router.push('/analysis')">Analyse a position</EButton>
        <EButton size="lg" variant="outline" :icon="PhFlask" @click="router.push('/experiments')">Test an idea</EButton>
      </div>
    </section>

    <section class="now" aria-labelledby="now-title">
      <header class="now-head">
        <EHeading id="now-title" :level="2">Right now</EHeading>
        <EStamp
          :tone="
            summary.state === 'busy'
              ? 'info'
              : summary.state === 'idle'
                ? 'success'
                : summary.state === 'offline'
                  ? 'danger'
                  : 'neutral'
          "
          :label="summary.state === 'busy' ? 'Working' : summary.label"
        />
      </header>
      <template v-if="experiment">
        <p class="now-title">
          <ELink :link-component="RouterLink" :to="`/experiments/${experiment.id}`">{{ experiment.name }}</ELink>
          <span class="quiet"> · {{ experiment.current ? stageLabel(experiment.current) : "preparing" }}</span>
        </p>
        <p v-if="experiment.hypothesis" class="quiet">{{ experiment.hypothesis }}</p>
        <DecisionGauge v-if="liveTest?.spec.sprt" :test="liveTest" />
      </template>
      <p v-else-if="running" class="now-title">{{ summary.label }}</p>
      <p v-else-if="summary.state === 'offline'" class="quiet">
        No runner is connected. Start one with <code>chess serve</code>, then open the runner menu at the top right.
      </p>
      <p v-else class="quiet">The runner is free. Start an experiment or build an engine.</p>
    </section>

    <EGrid min="11rem" gap="base">
      <EMetric label="Experiments" :value="experiments.data?.length ?? 0" />
      <EMetric label="Found stronger" :value="stronger" />
      <EMetric label="Engines built" :value="builds.data?.length ?? 0" />
      <EMetric label="Fastest engine" :value="fastest ? speed(fastest) : '–'" />
    </EGrid>

    <EStack gap="base">
      <EHeading :level="2">Latest results</EHeading>
      <EEmptyState v-if="!finished.length" title="No finished experiments yet" icon="info" size="sm">
        Results show here once an experiment decides.
      </EEmptyState>
      <ul v-else class="results">
        <li v-for="e in finished" :key="e.id" class="result">
          <EStamp size="sm" :tone="verdict(e).tone" :label="verdict(e).label" />
          <ELink :link-component="RouterLink" :to="`/experiments/${e.id}`">{{ e.name }}</ELink>
          <span class="quiet result-summary">{{ e.summary }}</span>
          <span class="quiet">{{ ago(e.finished_at!) }}</span>
        </li>
      </ul>
    </EStack>

    <EStack gap="base">
      <EHeading :level="2">How testing works</EHeading>
      <HowItWorks />
    </EStack>
  </EStack>
</template>

<style scoped>
.hero {
  display: grid;
  gap: var(--e-gap-base);
  padding: var(--e-gap-section) var(--e-pad-card);
  overflow: hidden;
  border-radius: var(--e-radius-lg);
  background:
    radial-gradient(120% 140% at 100% 0%, color-mix(in oklab, var(--e-brand) 28%, transparent), transparent 60%),
    repeating-conic-gradient(color-mix(in oklab, var(--e-brand) 7%, transparent) 0 25%, transparent 0 50%) 100% 0 / 3rem
      3rem,
    var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.hero-kicker {
  margin: 0;
  color: var(--e-primary);
  font: var(--e-type-kicker);
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.hero-title {
  max-width: 22ch;
  margin: 0;
  font: var(--e-type-h1);
  letter-spacing: -0.02em;
}

.hero-title em {
  color: var(--e-primary);
}

.hero-text {
  max-width: 60ch;
  margin: 0;
  font: var(--e-type-deck);
}

.hero-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug);
  margin-top: var(--e-gap-snug);
}

.now {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-pad-card);
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.now-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.now-title {
  margin: 0;
  font: var(--e-type-h4);
}

.quiet {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.results {
  display: grid;
  margin: 0;
  padding: 0;
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
  list-style: none;
}

.result {
  display: grid;
  grid-template-columns: 8rem auto minmax(0, 1fr) auto;
  align-items: center;
  gap: var(--e-gap-base);
  padding: var(--e-gap-snug) var(--e-pad-card);
}

.result + .result {
  border-top: 1px solid color-mix(in oklab, var(--e-border) 35%, transparent);
}

.result-summary {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (width < 40rem) {
  .result {
    grid-template-columns: auto minmax(0, 1fr);
  }

  .result-summary {
    display: none;
  }
}
</style>
