<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useRouter } from "vue-router";
import { useShortcuts } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EKbd } from "@eos-ui/vue/components/EKbd";
import { comesBack, scopeTitle, summarise } from "../board/drill";
import { THEMES } from "../board/themes";
import { useBoard } from "../board/board";
import { formatPercent, plural } from "../format";
import { useStartDrills } from "../shell/drillFlow";
import { useDrills } from "../stores/drills";
import UiStat from "../ui/UiStat.vue";
import UiStats from "../ui/UiStats.vue";
import UiTable from "../ui/UiTable.vue";
import UiToolbar from "../ui/UiToolbar.vue";

/**
 * A sitting's summary: the hit rate (a hint counts half), how the answers fell, which kinds of mistake were in it and
 * how each did, and when they come back. Done goes back to the board; another sitting is offered when there is more
 * to do. Enter is Done.
 */
const drills = useDrills();
const board = useBoard();
const router = useRouter();
const startDrills = useStartDrills();

const s = computed(() => summarise(drills.answers));
const title = computed(() => scopeTitle(drills.scope));
const rows = computed(() => s.value.themes.map((t) => ({ ...t, name: THEMES[t.kind].name })));
const columns = [
  { key: "name", label: "Kind of mistake" },
  { key: "result", label: "Right", fig: true },
  { key: "back", label: "Comes back", fig: true },
];

/** More to do: the day's set still has drills, or the theme or line still has some to try. */
const more = computed(() => (drills.scope.type === "day" ? drills.waiting > 0 : true));
const againLabel = computed(() => (drills.scope.type === "day" ? "Train more" : `More ${title.value.toLowerCase()}`));
onMounted(() => void drills.loadToday().catch(() => {}));

/** "3 of 4", a hint counting half ("2½ of 4"). */
function credit(n: number, of: number): string {
  const whole = Math.floor(n);
  const text = n - whole === 0.5 ? `${whole ? whole : ""}½` : String(whole);
  return `${text} of ${of}`;
}

function done() {
  drills.leave();
  void router.push({ path: "/", query: board.address });
}
async function again() {
  const scope = drills.scope;
  drills.leave();
  await startDrills(scope);
}
useShortcuts(
  {
    enter: (e) => {
      if (!(e.target as Element | null)?.closest?.("button, a, [role=button]")) done();
    },
  },
  { enabled: () => drills.finished, preventDefault: false },
);
</script>

<template>
  <div class="summary">
    <header class="head">
      <h2 class="title">{{ title }}: {{ plural(s.total, "drill") }} done</h2>
    </header>
    <UiStats :columns="2" dense>
      <UiStat label="Hit rate" :value="formatPercent(s.rate * 100, 0)" size="xl" sub="A hint counts half" />
      <UiStat
        label="Right"
        :value="s.clean + s.hinted"
        :sub="`${s.hinted} with a hint · ${s.missed} missed`"
        size="xl"
      />
    </UiStats>

    <UiTable v-if="rows.length" label="How each kind of mistake went" :columns="columns" :rows="rows" row-key="kind">
      <template #cell-result="{ row }">{{ credit(row.credit, row.total) }}</template>
      <template #cell-back="{ row }">{{ comesBack(row.back) }}</template>
    </UiTable>

    <UiToolbar align="between" label="Next">
      <EButton variant="primary" aria-keyshortcuts="Enter" @click="done">
        Done <EKbd size="sm" class="kbd">Enter</EKbd>
      </EButton>
      <EButton v-if="more" variant="outline" @click="again">
        {{ againLabel }}
      </EButton>
    </UiToolbar>
  </div>
</template>

<style scoped>
.summary {
  display: grid;
  gap: var(--sp-3);
  min-width: 0;
}

.title {
  margin: 0;
  font: var(--t-head);
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.meta::first-letter {
  text-transform: uppercase;
}

.kbd {
  margin-inline-start: var(--sp-1);
}

@media (pointer: coarse) {
  .kbd {
    display: none;
  }
}
</style>
