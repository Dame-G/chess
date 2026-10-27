<script setup lang="ts">
import { ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { ApiError } from "../api/client";
import { nameOfKey, playerFromInput } from "../sheets/compare";
import { usePlayers } from "../stores/players";

/**
 * Adds a Lichess player to a comparison: a name or a pasted link to their profile, checked against Lichess before it
 * is added so a typo is caught here and not as an empty column.
 */
const props = defineProps<{ taken: string[]; full: boolean }>();
const emit = defineEmits<{ add: [key: string] }>();

const players = usePlayers();
const text = ref("");
const error = ref("");
const busy = ref(false);

async function add() {
  const key = playerFromInput(text.value);
  error.value = "";
  if (!key) {
    error.value = "That is not a Lichess name. Letters, numbers, - and _, up to 30. A link to a profile works too.";
    return;
  }
  if (props.taken.includes(key)) {
    error.value = `${nameOfKey(key)} is already in the comparison.`;
    return;
  }
  busy.value = true;
  try {
    await players.lookup(nameOfKey(key));
    emit("add", key);
    text.value = "";
  } catch (e) {
    error.value =
      e instanceof ApiError && e.status === 404
        ? `Lichess has no player called ${nameOfKey(key)}.`
        : e instanceof Error
          ? e.message
          : String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <form class="add" @submit.prevent="add">
    <ETextField
      v-model="text"
      class="field"
      label="Add a player"
      placeholder="A Lichess name or a link to their profile"
      autocomplete="off"
      :error="error || undefined"
      :disabled="full"
      :hint="full ? 'Five players is the most that fit side by side.' : undefined"
    />
    <EButton type="submit" variant="outline" :loading="busy" :disabled="full || !text.trim()">Add</EButton>
  </form>
</template>

<style scoped>
.add {
  display: flex;
  align-items: flex-start;
  gap: var(--e-gap-snug);
}

.field {
  flex: 1 1 auto;
  min-width: 0;
}

/* Lines up the button with the input, below the label. */
.add :deep(.e-button) {
  margin-block-start: 1.5rem;
}
</style>
