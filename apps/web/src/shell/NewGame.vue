<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { ENumberField } from "@eos-ui/vue/components/ENumberField";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import type { Color, Side, TimeControl } from "../api/types";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import { useLevels } from "../stores/levels";
import { useMachine } from "../stores/machine";
import { stored } from "../stored";
import { levelChoices, suggestedStrength } from "../sheets/levels";
import LevelPicker from "../parts/LevelPicker.vue";
import UiBadge from "../ui/UiBadge.vue";
import UiDialog from "../ui/UiDialog.vue";
import UiField from "../ui/UiField.vue";
import UiSeg from "../ui/UiSeg.vue";
import { readClock } from "./commands";
import { useUi } from "./ui";
import UiSelect from "../ui/UiSelect.vue";

/**
 * New game: the opponent (Dameless at a level, Lichess, Stockfish N), the time control, the colour, rated. One
 * dialog, showing only the choices that apply to the opponent, and a Start button that says what it will start. From a
 * position ("Play from here"), against Dameless.
 */
const ui = useUi();
const router = useRouter();
const toast = useToast();
const games = useGames();
const connections = useConnections();
const ladder = useLevels();
const machine = useMachine();

type Opponent = "dameless" | "stockfish" | "lichess";
/** The clock choices that are not a preset. */
const UNTIMED = "untimed";
const CUSTOM = "custom";
const opponent = stored<Opponent>("new.opponent", "dameless");
const clock = stored("new.clock", "10+5");
/** The custom clock: minutes, and seconds added each move. */
const customMinutes = stored<number | null>("new.custom.minutes", 5);
const customIncrement = stored<number | null>("new.custom.increment", 3);
const color = stored<Color>("new.color", "random");
const rated = stored("new.rated", true);
const level = stored("new.level", "5");
/**
 * Dameless's strength as last chosen: "full", or a level of its ladder (`dameless@<level>`). Empty until you choose, and
 * then the level nearest your rating is the one shown.
 */
const strength = stored("new.strength", "");
const chosenStrength = computed({
  get: () => strength.value || suggestedStrength(ladder.levels),
  set: (v: string) => (strength.value = v),
});
/** Begin from a random line of the opening book instead of the standard start. */
const book = ref(false);
/** The book is not installed yet (Settings › Health says how), so a book start would be refused. */
const noBook = computed(() => machine.health?.items.some((i) => i.key === "book" && !i.ok) ?? false);
const lichessHow = ref<"seek" | "challenge">("seek");
const username = ref("");
const busy = ref(false);

const fromPosition = computed(() => ui.newGameFrom);
/** The strength remembered for New game while a practice position borrows the dialog. */
let remembered: string | null = null;
watch(
  () => ui.newGame,
  async (open) => {
    if (!open) {
      if (remembered !== null) strength.value = remembered;
      remembered = null;
      return;
    }
    book.value = false;
    if (fromPosition.value) opponent.value = "dameless";
    if (!ladder.levels) await ladder.loadLevels().catch(() => {});
    if (!machine.health) void machine.loadHealth().catch(() => {});
    // Practising a position ("Play this out") starts at the level nearest your rating.
    if (ui.newGame && fromPosition.value) {
      remembered ??= strength.value;
      strength.value = suggestedStrength(ladder.levels);
    }
  },
);

const linked = computed(() => connections.you.configured);
const opponents = computed(() => [
  { value: "dameless" as const, label: "Dameless" },
  { value: "lichess" as const, label: "Lichess", disabled: !linked.value },
  { value: "stockfish" as const, label: "Stockfish", disabled: !linked.value },
]);

/**
 * Lichess's Board API plays no bullet; a seek must be rapid or slower (initial + 40 × increment ≥ 8 min). Against
 * Dameless a game can have no clock (the engine takes a second a move), and only such a game allows takebacks.
 */
const seeking = computed(() => opponent.value === "lichess" && lichessHow.value === "seek");
const clocks = computed(() => {
  const presets = (seeking.value ? ["10+0", "10+5", "15+10", "30+0"] : ["3+2", "5+3", "10+5", "15+10"]).map((c) => ({
    value: c,
    label: c,
  }));
  if (seeking.value) return presets;
  return [
    ...(opponent.value === "dameless" ? [{ value: UNTIMED, label: "No clock" }] : []),
    ...presets,
    { value: CUSTOM, label: "Custom" },
  ];
});
watch(clocks, (options) => {
  if (!options.some((o) => o.value === clock.value))
    clock.value = options.some((o) => o.value === "5+3") ? "5+3" : options[1]!.value;
});
const levelOptions = computed(() => levelChoices(ladder.levels));

/** The clock the game is played on. */
const time = computed<TimeControl>(() => {
  if (clock.value === UNTIMED) return { type: "movetime", ms: 1000 };
  if (clock.value === CUSTOM) {
    return {
      type: "clock",
      initial_ms: Math.round((customMinutes.value ?? 5) * 60_000),
      increment_ms: Math.round((customIncrement.value ?? 0) * 1000),
    };
  }
  return readClock(clock.value) ?? { type: "clock", initial_ms: 300_000, increment_ms: 3000 };
});
const customOk = computed(() => clock.value !== CUSTOM || (customMinutes.value ?? 0) > 0);
/** The clock as the button says it: "5+3", or nothing for a game without one. */
const clockText = computed(() => {
  if (clock.value === UNTIMED) return "";
  if (clock.value === CUSTOM) return `${customMinutes.value ?? 5}+${customIncrement.value ?? 0}`;
  return clock.value;
});

/** Dameless's seat: `dameless`, at a level (`@<n>`) when chosen. */
function damelessSeat(): string {
  return chosenStrength.value !== "full" ? `dameless@${chosenStrength.value}` : "dameless";
}

/** What Start will start, in the button's own words. */
const confirmLabel = computed(() => {
  if (opponent.value === "stockfish") return `Start vs Stockfish ${level.value}`;
  if (opponent.value === "lichess") {
    return lichessHow.value === "seek" ? "Find a game" : `Challenge ${username.value.trim() || "someone"}`;
  }
  return `Start ${clockText.value ? `${clockText.value} ` : ""}vs Dameless`;
});
const blocked = computed(
  () => !customOk.value || (opponent.value === "lichess" && lichessHow.value === "challenge" && !username.value.trim()),
);
/** What is worth knowing about the strength picked: that its rating may still move. */
const strengthHint = computed(() => {
  const picked = levelOptions.value.find((c) => c.value === chosenStrength.value);
  return picked && picked.standing !== "found" ? "Still being calibrated: this rating may move." : undefined;
});
/** Takebacks are only for games with no clock: said where the clock is chosen. */
const clockHint = computed(() => {
  if (opponent.value !== "dameless") return undefined;
  return clock.value === UNTIMED
    ? "No clock: the engine takes about a second a move, and you can take moves back."
    : "Taking moves back needs a game with no clock.";
});
const hint = computed(() => {
  if (!customOk.value) return "Give the clock some minutes.";
  if (opponent.value === "lichess" && lichessHow.value === "challenge" && !username.value.trim()) {
    return "Who do you want to challenge?";
  }
  return "";
});

async function start() {
  busy.value = true;
  try {
    if (opponent.value === "dameless") {
      const you: Side = color.value === "random" ? (Math.random() < 0.5 ? "white" : "black") : color.value;
      const seat = damelessSeat();
      const from = fromPosition.value;
      const s = await games.start({
        white: you === "white" ? "you" : seat,
        black: you === "white" ? seat : "you",
        time: time.value,
        ...(from
          ? { from: { type: "position" as const, fen: from.fen, moves: from.moves } }
          : book.value
            ? { from: { type: "book" as const } }
            : {}),
      });
      ui.newGame = false;
      ui.railOpen = false;
      await router.push({ path: "/", query: { game: String(s.id) } });
    } else if (opponent.value === "stockfish") {
      await connections.challenge({
        ai_level: Number(level.value),
        time: time.value,
        rated: false,
        color: color.value,
      });
      ui.newGame = false;
      toast.show({ title: `Starting a game against Stockfish level ${level.value}`, tone: "info" });
    } else if (lichessHow.value === "seek") {
      await connections.seek({ time: time.value, rated: rated.value, color: color.value });
      ui.newGame = false;
      toast.show({
        title: "Looking for a game on Lichess",
        message: "It opens on the board when someone takes it.",
        tone: "info",
      });
    } else {
      await connections.challenge({
        username: username.value.trim(),
        time: time.value,
        rated: rated.value,
        color: color.value,
      });
      ui.newGame = false;
      toast.show({ title: `Challenged ${username.value.trim()}`, tone: "info" });
    }
  } catch (e) {
    toast.show({
      title: "The game did not start",
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
    v-model:open="ui.newGame"
    :title="fromPosition ? 'Play from this position' : 'New game'"
    :confirm-label="confirmLabel"
    :confirm-disabled="blocked"
    :busy="busy"
    @confirm="start"
  >
    <div class="form">
      <UiField v-if="!fromPosition" label="Opponent" group>
        <UiSeg v-model="opponent" label="Opponent" :options="opponents" full />
        <p v-if="connections.loaded && !linked" class="login">
          <UiBadge>Log in first</UiBadge>
          Lichess and Stockfish need your Lichess account.
          <RouterLink to="/settings" @click="ui.newGame = false">Open Settings</RouterLink>
        </p>
      </UiField>

      <template v-if="opponent === 'dameless'">
        <UiField label="Strength" group :hint="strengthHint">
          <LevelPicker v-if="levelOptions.length" v-model="chosenStrength" label="Strength" :choices="levelOptions" />
          <span v-else class="skel picker-skel" aria-label="Reading the levels" />
        </UiField>
      </template>
      <UiSelect
        v-else-if="opponent === 'stockfish'"
        v-model="level"
        label="Level"
        :options="['1', '2', '3', '4', '5', '6', '7', '8']"
      />
      <template v-else-if="opponent === 'lichess'">
        <UiField label="How" group>
          <UiSeg
            v-model="lichessHow"
            label="How"
            :options="[
              { value: 'seek', label: 'Find a game' },
              { value: 'challenge', label: 'Challenge someone' },
            ]"
            full
          />
        </UiField>
        <ETextField v-if="lichessHow === 'challenge'" v-model="username" label="Lichess username" autocomplete="off" />
      </template>

      <UiField label="Time control" group :hint="clockHint">
        <UiSeg v-model="clock" label="Time control" :options="clocks" full />
        <div v-if="clock === 'custom'" class="pair custom">
          <ENumberField v-model="customMinutes" label="Minutes" :min="0.25" :step="1" suffix="min" />
          <ENumberField v-model="customIncrement" label="Added each move" :min="0" :max="180" :step="1" suffix="s" />
        </div>
      </UiField>

      <UiField label="You play" group>
        <UiSeg
          v-model="color"
          label="You play"
          :options="[
            { value: 'white', label: 'White' },
            { value: 'random', label: 'Random' },
            { value: 'black', label: 'Black' },
          ]"
          full
        />
      </UiField>

      <ESwitch v-if="opponent === 'lichess'" v-model="rated" label="Rated" />

      <EDisclosure v-if="opponent === 'dameless' && !fromPosition" title="Options" title-style="text">
        <ESwitch
          v-model="book"
          label="Start from a book line"
          :disabled="noBook"
          :description="noBook ? 'The opening book is not installed yet: Settings › Health says how.' : undefined"
        />
      </EDisclosure>
    </div>
    <template #footer-start>{{ hint }}</template>
  </UiDialog>
</template>

<style scoped>
.form {
  display: grid;
  gap: var(--sp-3);
  min-width: 0;
}

.pair {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--sp-3);
}

.custom {
  margin-block-start: var(--sp-2);
}

@media (width < 26rem) {
  .pair {
    grid-template-columns: minmax(0, 1fr);
  }
}

.login {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
  margin: var(--sp-2) 0 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.login a {
  color: inherit;
  text-decoration: underline;
  text-decoration-color: var(--c-line-strong);
  text-underline-offset: 0.2em;
}

.picker-skel {
  display: block;
  block-size: calc(4 * var(--h-control));
}
</style>
