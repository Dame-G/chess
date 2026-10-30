<script setup lang="ts">
import { computed, reactive, watch } from "vue";
import { RouterLink } from "vue-router";
import { HUMAN, opponentFamilies, type OpponentChoice } from "../sheets/opponents";
import UiField from "../ui/UiField.vue";
import UiSeg from "../ui/UiSeg.vue";
import LevelPicker from "./LevelPicker.vue";

/**
 * Which engine to play, in two steps: the family first (the analysis engine, the human-like Maia, each engine you
 * added), then its strength (a level with its rating, or a Maia's rating). An engine that plays as it is has no second
 * step. The value is the seat the server reads (`stockfish@5`, `maia-1500`), and each family remembers the strength
 * last chosen in it, so going back and forth does not lose it. What is not installed is shown but not chosen, and the
 * note under the steps says where it is set up.
 */
const props = defineProps<{
  modelValue: string;
  choices: OpponentChoice[];
  /** The analysis engine's id: it names the family that starts at `suggested`. */
  analysis: string;
  /** The seat to start a family at when it has no strength chosen yet: the level nearest your rating. */
  suggested: string;
}>();
const emit = defineEmits<{ "update:modelValue": [seat: string]; settings: [] }>();

const families = computed(() => opponentFamilies(props.choices, props.analysis));
const chosen = computed(() => props.choices.find((c) => c.value === props.modelValue));
const current = computed(() => families.value.find((f) => f.key === chosen.value?.family) ?? families.value[0]);

/** The strength last chosen in each family. */
const lastIn = reactive<Record<string, string>>({});
watch(
  chosen,
  (c) => {
    if (c) lastIn[c.family] = c.value;
  },
  { immediate: true },
);

const options = computed(() => families.value.map((f) => ({ value: f.key, label: f.name, disabled: f.disabled })));

function pickFamily(key: string) {
  const f = families.value.find((x) => x.key === key);
  if (!f || f.key === current.value?.key) return;
  const open = f.choices.filter((c) => !c.disabled);
  const next = open.find((c) => c.value === lastIn[f.key]) ?? open.find((c) => c.value === props.suggested) ?? open[0];
  if (next) emit("update:modelValue", next.value);
}

/** A rating still being calibrated may move. */
const strengthHint = computed(() =>
  chosen.value && chosen.value.standing !== "found" ? "Still being calibrated: this rating may move." : undefined,
);
const second = computed(() => (current.value?.choices.length ?? 0) > 1);

/** What is not installed, as a sentence for the note: this family's missing ratings, else whole families by name. */
const missing = computed(() => {
  if (current.value?.choices.some((c) => c.disabled)) return "Ratings marked not installed need setting up.";
  const gone = families.value.filter((f) => f.disabled).map((f) => f.name);
  return gone.length ? `${gone.join(" and ")} ${gone.length > 1 ? "are" : "is"} not installed.` : "";
});
</script>

<template>
  <div class="opp">
    <UiField label="Engine" group :hint="current?.detail">
      <UiSeg
        :model-value="current?.key ?? ''"
        label="Engine"
        :options="options"
        full
        @update:model-value="pickFamily"
      />
    </UiField>
    <UiField v-if="second" :label="current?.key === HUMAN ? 'Rating' : 'Strength'" group :hint="strengthHint">
      <LevelPicker
        :model-value="modelValue"
        :label="current?.name ?? 'Strength'"
        :choices="current?.choices ?? []"
        @update:model-value="emit('update:modelValue', $event)"
      />
    </UiField>
    <p v-if="missing" class="miss">
      {{ missing }}
      <RouterLink to="/settings?tab=engines" @click="emit('settings')">Set up in Settings › Engines</RouterLink>
    </p>
  </div>
</template>

<style scoped>
.opp {
  display: grid;
  gap: var(--sp-3);
  min-width: 0;
}

.miss {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.miss a {
  color: inherit;
  text-decoration: underline;
  text-decoration-color: var(--c-line-strong);
  text-underline-offset: 0.2em;
}
</style>
