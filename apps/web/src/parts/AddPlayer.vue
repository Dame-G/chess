<script setup lang="ts">
import { ref } from "vue";
import { ESearchBar } from "@eos-ui/vue/components/ESearchBar";
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
  if (!text.value.trim() || busy.value) return;
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
  <div class="add">
    <ESearchBar
      v-model="text"
      label="Add a player"
      placeholder="Add a player: a Lichess name or a link"
      action="Add"
      :loading="busy"
      :disabled="full"
      @submit="add"
    />
    <p v-if="error" class="err" role="alert">{{ error }}</p>
    <p v-else-if="full" class="note">Five players is the most that fit side by side.</p>
  </div>
</template>

<style scoped>
.add {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}

.err,
.note {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.err {
  color: var(--c-loss);
}
</style>
