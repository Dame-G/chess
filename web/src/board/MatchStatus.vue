<script setup lang="ts">
import { computed } from "vue";
import { PhArrowCounterClockwise, PhStop, PhSwap } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { matchTimeText, outcomeText, type Match } from "./matchText";

const props = defineProps<{
  match: Match;
  /** Whether the board shows the latest move. */
  attached: boolean;
  /** A request to the runner is under way. */
  busy?: boolean;
}>();
defineEmits<{ stop: []; rematch: []; swap: [] }>();

const running = computed(() => props.match.status === "running");
const moves = computed(() => Math.ceil(props.match.moves.length / 2));
const stamp = computed(() => {
  if (running.value) return { label: props.attached ? "Live" : "Behind", tone: "info" } as const;
  if (props.match.status === "aborted") return { label: "Stopped", tone: "neutral" } as const;
  return { label: "Final", tone: "neutral" } as const;
});
const sentence = computed(() => (running.value ? "In progress" : outcomeText(props.match)));
const facts = computed(() => {
  const m = props.match;
  const parts = [matchTimeText(m.time), `${moves.value} move${moves.value === 1 ? "" : "s"}`];
  if (m.book) parts.push("book opening");
  return parts.join(" · ");
});
</script>

<template>
  <header class="status">
    <p class="line">
      <EStamp size="sm" :tone="stamp.tone" :label="stamp.label" />
      <strong class="sentence" aria-live="polite">{{ sentence }}</strong>
    </p>
    <p class="facts num">{{ facts }}</p>
    <div class="actions">
      <EButton v-if="running" variant="danger" size="sm" :icon="PhStop" :loading="busy" @click="$emit('stop')">
        Stop match
      </EButton>
      <template v-else>
        <EButton variant="outline" size="sm" :icon="PhArrowCounterClockwise" :loading="busy" @click="$emit('rematch')">
          Rematch
        </EButton>
        <EButton variant="outline" size="sm" :icon="PhSwap" :loading="busy" @click="$emit('swap')">
          Swap colours
        </EButton>
      </template>
    </div>
  </header>
</template>

<style scoped>
.status {
  display: grid;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-base) var(--ws-pad);
}

.line {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  margin: 0;
}

.sentence {
  font: var(--e-type-h6);
}

.facts {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug);
}
</style>
