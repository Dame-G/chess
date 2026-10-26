<script setup lang="ts">
import { ref } from "vue";
import { PhPlus } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { fetchLichessPlayer } from "../../api/queries";

const emit = defineEmits<{ add: [username: string] }>();
const text = ref("");
const error = ref("");
const busy = ref(false);

/** Lichess's usernames: letters, digits, underscore and hyphen. A pasted profile link works too. */
const clean = (raw: string) =>
  raw
    .trim()
    .replace(/^.*lichess\.org\/@\//, "")
    .replace(/^@/, "");

async function submit() {
  const name = clean(text.value);
  if (!name) return;
  busy.value = true;
  error.value = "";
  try {
    const profile = await fetchLichessPlayer(name);
    if (profile.disabled) error.value = `${profile.username} has closed their account.`;
    else {
      emit("add", profile.username);
      text.value = "";
    }
  } catch (e) {
    const message = (e as Error).message;
    error.value = /404|not found|no such/i.test(message) ? `Lichess has no player called ${name}.` : message;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <form class="add" @submit.prevent="submit">
    <ETextField
      v-model="text"
      label="Add a Lichess player"
      placeholder="Username"
      size="sm"
      autocomplete="off"
      :error="error"
    />
    <EButton type="submit" variant="outline" size="sm" :icon="PhPlus" :loading="busy" :disabled="!text.trim()">
      Add
    </EButton>
  </form>
</template>

<style scoped>
.add {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: end;
  gap: var(--e-gap-snug);
}
</style>
