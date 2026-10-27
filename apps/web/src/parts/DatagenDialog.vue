<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { datagenEstimate, datagenText, validName, whyNoRoom } from "../sheets/training";
import { usePolicy } from "../stores/policy";
import { useTraining } from "../stores/training";

/**
 * Generate data: Dameless plays itself, in the background, and writes the positions down for training. The dialog says
 * what it will cost before it starts: the time, from the speed the machine makes data at, and the room it takes on
 * disk. The same name again adds to that dataset.
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
const blocked = computed(() => positions.value < 1000 || !!nameError.value || !!noRoom.value);

async function start() {
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
  <EDialog
    v-model:open="open"
    title="Generate data"
    description="Dameless plays itself and writes the positions down for training."
    size="md"
  >
    <EForm id="datagen-form" @submit.prevent="start">
      <ENumberField
        v-model="millions"
        label="Positions"
        suffix="million"
        :min="0.1"
        :step="5"
        :error="positions < 1000 ? 'At least a thousand positions.' : undefined"
      />
      <ENumberField
        v-model="threads"
        label="Threads"
        :min="1"
        :max="cores"
        :hint="`Half the machine's ${cores} is the default. More makes data faster and slows everything else.`"
      />
      <ETextField
        v-model="name"
        label="Name"
        optional
        autocomplete="off"
        :error="nameError"
        hint="The same name again adds more positions to that dataset."
      />
      <p class="sec-note" role="status">{{ datagenText(estimate) }}</p>
      <ENotice v-if="noRoom" tone="warning" title="There is not enough room">{{ noRoom }}</ENotice>
    </EForm>
    <template #actions>
      <EButton type="submit" form="datagen-form" :loading="busy" :disabled="blocked">Generate</EButton>
      <EButton variant="outline" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>
