<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { datagenEstimate, datagenSummary, datagenText, validName, whyNoRoom } from "../sheets/training";
import { usePolicy } from "../stores/policy";
import { useTraining } from "../stores/training";
import UiDialog from "../ui/UiDialog.vue";
import UiField from "../ui/UiField.vue";

/**
 * Generate data: Dameless plays itself, in the background, and writes the positions down for training. The footer says
 * what it will cost before it starts: the room on disk and the time, from the speed the machine makes data at. The same
 * name again adds to that dataset.
 */
const open = defineModel<boolean>("open", { required: true });
const training = useTraining();
const policy = usePolicy();
const toast = useToast();

const name = ref("");
const millions = ref<number | null>(20);
const cores = computed(() => policy.status?.cores ?? 8);
const defaultThreads = computed(() => Math.max(1, Math.floor(cores.value / 2)));
const threads = ref<number | null>(null);
const busy = ref(false);

watch(open, (isOpen) => {
  if (!isOpen) return;
  name.value = "";
  millions.value = 20;
  threads.value = defaultThreads.value;
});

const positions = computed(() => Math.round((millions.value ?? 0) * 1_000_000));
const estimate = computed(() =>
  datagenEstimate(positions.value, threads.value ?? defaultThreads.value, defaultThreads.value),
);
const noRoom = computed(() => (training.data ? whyNoRoom(estimate.value.bytes, training.data) : null));
const nameError = computed(() =>
  name.value && !validName(name.value) ? "Use letters, digits, dashes and underscores." : undefined,
);
const positionsError = computed(() =>
  positions.value < 1000 ? "At least a thousand positions." : (noRoom.value ?? undefined),
);
const blocked = computed(() => !!positionsError.value || !!nameError.value);

async function start() {
  if (blocked.value) return;
  busy.value = true;
  try {
    const w = await training.generate({
      positions: positions.value,
      ...(name.value.trim() ? { name: name.value.trim() } : {}),
      ...(threads.value ? { threads: threads.value } : {}),
    });
    open.value = false;
    toast.show({ title: "Generating data", message: w.title, tone: "success" });
  } catch (e) {
    toast.show({
      title: "Data generation was not queued",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <UiDialog
    v-model:open="open"
    title="Generate data"
    size="sm"
    confirm-label="Generate"
    :busy="busy"
    :confirm-disabled="blocked"
    @confirm="start"
  >
    <div class="stack">
      <UiField v-slot="{ id, describedby, invalid }" label="Positions" :error="positionsError">
        <ENumberField
          :id="id"
          v-model="millions"
          label="Positions"
          hide-label
          suffix="million"
          class="num"
          :min="0.1"
          :step="5"
          :aria-describedby="describedby"
          :aria-invalid="invalid || undefined"
        />
      </UiField>
      <UiField
        v-slot="{ id }"
        label="Threads"
        :hint="`Default ${defaultThreads}, half of the machine's ${cores}. More makes data faster and slows everything else.`"
      >
        <ENumberField :id="id" v-model="threads" label="Threads" hide-label :min="1" :max="cores" class="num" />
      </UiField>
      <UiField v-slot="{ id, describedby, invalid }" label="Name" optional :error="nameError">
        <ETextField
          :id="id"
          v-model="name"
          label="Name"
          hide-label
          autocomplete="off"
          placeholder="Reuse a name to add to it"
          :aria-describedby="describedby"
          :aria-invalid="invalid || undefined"
        />
      </UiField>
    </div>
    <template #footer-start>
      <span role="status" class="summary" :title="datagenText(estimate)">{{ datagenSummary(estimate) }}</span>
    </template>
  </UiDialog>
</template>

<style scoped>
/* A number takes the room a number needs: narrow enough that the field drops its step buttons. */
.num {
  inline-size: 11rem;
}

.summary {
  font: var(--t-fig);
}
</style>
