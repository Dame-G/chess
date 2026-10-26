<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { api } from "../../api/runner";
import { unwrap, type Tune } from "../../api/queries";

const props = defineProps<{ tune: Tune }>();
const open = defineModel<boolean>("open", { default: false });

const branch = ref("");
const base = ref("main");
const experiment = ref(true);
const ltc = ref(false);
const push = ref(false);
const error = ref("");
const submitting = ref(false);

// A fresh suggestion each time the dialog opens for a tune.
watch(
  [open, () => props.tune.id],
  () => {
    if (open.value) {
      branch.value = `tune/${props.tune.id}`;
      error.value = "";
    }
  },
  { immediate: true },
);

const plain = computed(
  () =>
    `The ${props.tune.params.length} values after iteration ${props.tune.iteration} are rounded and written into a new branch, which is built and benched.`,
);

async function submit() {
  submitting.value = true;
  error.value = "";
  try {
    await unwrap(
      api.POST("/api/tunes/{id}/apply", {
        params: { path: { id: props.tune.id } },
        body: {
          branch: branch.value.trim() || null,
          base: base.value.trim() || null,
          experiment: experiment.value,
          ltc: experiment.value && ltc.value,
          push: push.value,
        },
      }),
    );
    open.value = false;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <EDialog v-model:open="open" title="Apply this tune" :description="plain" size="md">
    <EForm id="apply-tune" :submitting="submitting" @submit="submit">
      <ETextField v-model="branch" label="New branch" hint="It must not exist yet." autocomplete="off" required />
      <ESwitch
        v-model="experiment"
        label="Test it against main"
        hint="Queues an experiment that the branch must pass at [0, 4] before the change is trusted."
      />
      <ETextField v-if="experiment" v-model="base" label="Compare with" autocomplete="off" required />
      <ESwitch v-if="experiment" v-model="ltc" label="Confirm with long games too" hint="A few more hours." />
      <ESwitch v-model="push" label="Push the branch" hint="Sends it to the fork's origin." />
      <ENotice v-if="error" tone="danger" title="Could not apply it" :role="false">{{ error }}</ENotice>
    </EForm>
    <template #actions>
      <EButton type="submit" form="apply-tune" :loading="submitting" :disabled="!branch.trim()">Apply</EButton>
      <EButton variant="quiet" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>
