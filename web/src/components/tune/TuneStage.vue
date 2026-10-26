<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ETable } from "@eos-ui/vue/components/ETable";
import type { Job, Tune } from "../../api/queries";
import { failure } from "../../format";
import Term from "../Term.vue";
import TuneChart from "./TuneChart.vue";
import TuneExplainer from "./TuneExplainer.vue";
import { change, fmt, live, trajectory, tuneLine, type Iteration } from "./tune";

/** One tune: how it stands in a sentence, where every parameter is heading, and the parameters themselves. */
const props = defineProps<{
  tune: Tune;
  history: Iteration[];
  /** The time left, when the page has watched long enough to say. */
  remaining?: string;
  /** The newest job that applied this tune to a branch. */
  applying?: Job;
}>();

const path = computed(() => trajectory(props.tune.params, props.history));

// ---- Reading the chart: the line under the pointer reads out, else the chosen parameter
const picked = ref<number>();
const hovered = ref<{ series: number; index: number }>();
const readout = computed(() => {
  const h = hovered.value;
  const i = h?.series ?? picked.value;
  const p = i == null ? undefined : props.tune.params[i];
  if (i == null || !p) return undefined;
  const at = h?.index ?? path.value.x.length - 1;
  const v = path.value.values[i]?.[at] ?? p.value;
  const c = change(p, v);
  return { name: p.name, iteration: path.value.x[at] ?? 0, value: fmt(p, v), change: c.text, share: c.share };
});

function toggle(i: number) {
  picked.value = picked.value === i ? undefined : i;
}

// ---- The table
const order = ref<"change" | "declared">("change");
const rows = computed(() => {
  const all = props.tune.params.map((p, i) => ({ p, i, c: change(p, p.value) }));
  return order.value === "change" ? all.toSorted((a, b) => Math.abs(b.c.share) - Math.abs(a.c.share)) : all;
});
const tone = (delta: number) => (delta > 0 ? "up" : delta < 0 ? "down" : "flat");

const bench = computed(() => (result.value?.bench ? ` (fingerprint ${result.value.bench.toLocaleString("en")})` : ""));

const result = computed(() => {
  const r = props.applying?.result as
    { branch?: string; bench?: number; experiment?: number | null } | null | undefined;
  return r && typeof r === "object" ? r : undefined;
});
</script>

<template>
  <div class="stage">
    <header class="intro">
      <p :class="['verdict', `verdict--${tune.status}`]" aria-live="polite">{{ tuneLine(tune, remaining) }}</p>
      <EProgress
        v-if="live(tune)"
        label="Iterations"
        :value="tune.iteration"
        :max="tune.iterations"
        :value-text="`${tune.iteration}/${tune.iterations}`"
      />
    </header>

    <ENotice
      v-if="applying && (applying.status === 'queued' || applying.status === 'running')"
      tone="info"
      title="Applying to a branch"
      :role="false"
    >
      The current values are being written into a new branch of the engine, built and benched.
    </ENotice>
    <ENotice v-else-if="applying?.status === 'failed'" tone="danger" title="Could not apply the tune" :role="false">
      {{ failure(applying.error, applying.spec.kind) }}
    </ENotice>
    <ENotice v-else-if="applying?.status === 'done' && result" tone="success" title="Applied" :role="false">
      Branch <code>{{ result.branch }}</code> holds the new values{{ bench }}.
      <template v-if="result.experiment">
        <ELink :link-component="RouterLink" :to="`/lab/${result.experiment}`">See its confirmation test</ELink> in the
        lab. It must pass before the change is trusted.
      </template>
    </ENotice>

    <section class="unit" aria-labelledby="path-title">
      <div class="unit-head">
        <h2 id="path-title" class="unit-title">Where each parameter is heading</h2>
        <p class="unit-note">
          Each line is one parameter, as a share of its allowed range, so big and small constants share a chart.
        </p>
      </div>
      <div v-if="history.length" class="chart">
        <TuneChart
          label="Each parameter's change over the iterations, as a share of its range"
          :x="path.x"
          :series="path.scaled"
          :selected="picked"
          @point="hovered = $event ?? undefined"
          @select="toggle"
        />
        <p class="readout num">
          <template v-if="readout">
            <b>{{ readout.name }}</b>
            at iteration {{ readout.iteration }}: {{ readout.value }}
            <span :class="['change', `change--${tone(readout.share)}`]">
              ({{ readout.change }}, {{ Math.abs(readout.share).toFixed(1) }}% of its range)
            </span>
          </template>
          <span v-else class="hint">Point at a line to read its value, or choose a parameter below.</span>
        </p>
      </div>
      <EEmptyState v-else bare size="sm" title="No iterations yet">
        {{
          live(tune)
            ? "The chart appears after the first iteration, and grows as the tune plays."
            : "This tune stopped before its first iteration."
        }}
      </EEmptyState>
    </section>

    <section v-if="tune.params.length" class="unit" aria-labelledby="params-title">
      <div class="unit-head">
        <h2 id="params-title" class="unit-title">
          Parameters <span class="count num">{{ tune.params.length }}</span>
        </h2>
        <ESegmented
          v-model="order"
          label="Order"
          hide-label
          size="sm"
          :options="[
            { value: 'change', label: 'Largest change' },
            { value: 'declared', label: 'In engine order' },
          ]"
        />
      </div>
      <ETable caption="Parameters" hide-caption dense max-height="28rem">
        <thead>
          <tr>
            <th scope="col">Parameter</th>
            <th scope="col" class="e-num">Start</th>
            <th scope="col" class="e-num">Now</th>
            <th scope="col" class="e-num">Change</th>
            <th scope="col" class="e-num">Range</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in rows" :key="r.p.name" :class="{ picked: picked === r.i }">
            <th scope="row" class="name">
              <EButton
                variant="quiet"
                size="sm"
                class="name-button"
                :aria-pressed="picked === r.i"
                :title="picked === r.i ? 'Stop highlighting' : 'Highlight on the chart'"
                @click="toggle(r.i)"
              >
                {{ r.p.name }}
              </EButton>
            </th>
            <td class="e-num num">{{ fmt(r.p, r.p.start) }}</td>
            <td class="e-num num">{{ fmt(r.p, r.p.value) }}</td>
            <td :class="['e-num', 'num', 'change', `change--${tone(r.c.delta)}`]">
              {{ r.c.text }}
              <span class="share">{{ Math.abs(r.c.share).toFixed(0) }}%</span>
            </td>
            <td class="e-num num range">{{ fmt(r.p, r.p.min) }} to {{ fmt(r.p, r.p.max) }}</td>
          </tr>
        </tbody>
      </ETable>
      <p class="unit-note foot">
        Percentages are the share of each parameter's range it has moved. Applying rounds each value to a whole number.
        <Term of="spsa" inline>What is SPSA?</Term>
      </p>
    </section>

    <EDisclosure class="how" title="What is a tune?">
      <TuneExplainer />
    </EDisclosure>
  </div>
</template>

<style scoped>
.stage {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  align-content: start;
  gap: var(--e-gap-loose);
  padding: var(--e-gap-section) var(--e-pad-card);
  min-width: 0;
}

.intro {
  display: grid;
  gap: var(--e-gap-base);
}

.verdict {
  margin: 0;
  padding-inline-start: var(--e-gap-base);
  border-inline-start: 3px solid var(--e-border-strong);
  font: var(--e-type-deck);
  text-wrap: balance;
}

.verdict--running {
  border-color: var(--e-primary);
}

.verdict--done {
  border-color: var(--e-success);
}

.verdict--failed {
  border-color: var(--e-danger);
}

.unit {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  min-width: 0;
  border: var(--e-stroke-hair) solid var(--e-border);
  border-radius: var(--e-radius-sm);
  background: var(--e-surface);
}

.unit-head {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: var(--e-gap-tight) var(--e-gap-base);
  padding: var(--e-gap-base) var(--e-pad-card);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
}

.unit-title {
  margin: 0;
  font: var(--e-type-h5);
}

.unit-note {
  flex-basis: 100%;
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.unit-note.foot {
  padding: var(--e-gap-base) var(--e-pad-card);
  border-block-start: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
}

.count {
  margin-inline-start: var(--e-gap-tight);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.chart {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: var(--e-gap-snug);
  padding: var(--e-gap-base) var(--e-pad-card);
  min-width: 0;
}

.readout {
  min-height: 1.5em;
  margin: 0;
  font: var(--e-type-small);
  overflow-wrap: anywhere;
}

.hint {
  color: var(--e-muted);
  font-family: var(--e-font-sans);
}

.name {
  font-weight: 400;
  text-align: start;
}

.name-button {
  font-family: var(--e-font-mono);
}

tr.picked {
  background: var(--e-primary-soft);
}

.change--up {
  color: var(--e-primary);
}

.change--down {
  color: var(--e-info);
}

.change--flat {
  color: var(--e-muted);
}

.share {
  display: inline-block;
  min-width: 3ch;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
}

.range {
  color: var(--e-muted);
  white-space: nowrap;
}

.how {
  min-width: 0;
}
</style>
