<script setup lang="ts">
import { ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import NewTune from "../tune/NewTune.vue";
import NewTest from "./NewTest.vue";

/** A branch to start from, when the form is opened for one. */
defineProps<{ branch?: string }>();
/** The one way to start something: first what for, then the form for it. */
const open = defineModel<boolean>("open", { default: false });
const mode = defineModel<"test" | "tune">("mode", { default: "test" });

const valid = ref(false);
const busy = ref(false);

const about = {
  test: "Does a branch make Reckless stronger than main? The two play each other until the result is clear.",
  tune: "A tune adjusts the engine's numbers by playing it against itself, and writes the result to a new branch. That branch still has to pass a test.",
} as const;
</script>

<template>
  <EDialog
    v-model:open="open"
    :title="mode === 'test' ? 'Test a change' : 'Tune numbers'"
    :description="about[mode]"
    size="lg"
  >
    <div class="body">
      <ESegmented
        v-model="mode"
        label="What do you want to do?"
        :options="[
          { value: 'test', label: 'Test a change' },
          { value: 'tune', label: 'Tune numbers' },
        ]"
      />
      <NewTest v-if="mode === 'test'" v-model:valid="valid" v-model:busy="busy" :branch="branch" @done="open = false" />
      <NewTune v-else v-model:valid="valid" v-model:busy="busy" :branch="branch" @done="open = false" />
    </div>
    <template #actions>
      <EButton type="submit" :form="mode === 'test' ? 'new-test' : 'new-tune'" :loading="busy" :disabled="!valid">
        {{ mode === "test" ? "Start test" : "Start tune" }}
      </EButton>
      <EButton variant="outline" @click="open = false">Cancel</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.body {
  display: grid;
  gap: var(--e-gap-loose);
  min-width: 0;
}
</style>
