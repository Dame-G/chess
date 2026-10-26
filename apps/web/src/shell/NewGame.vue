<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { EStack } from "@eos-ui/vue/components/EStack";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import type { Color } from "../api/types";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import { useVersions } from "../stores/versions";
import { stored } from "../stored";
import { readClock } from "./commands";
import { useUi } from "./ui";

/**
 * New game: the opponent (Dameless at a version, Lichess, Stockfish N), the time control, the colour, rated. One
 * dialog, showing only the choices that apply to the opponent. From a position ("Play from here"), against Dameless.
 */
const ui = useUi();
const router = useRouter();
const toast = useToast();
const games = useGames();
const connections = useConnections();
const versions = useVersions();

type Opponent = "dameless" | "stockfish" | "lichess";
const opponent = stored<Opponent>("new.opponent", "dameless");
const clock = stored("new.clock", "5+3");
const color = stored<Color>("new.color", "random");
const rated = stored("new.rated", true);
const level = stored("new.level", "5");
const version = ref<string>("");
const lichessHow = ref<"seek" | "challenge">("seek");
const username = ref("");
const busy = ref(false);

const fromPosition = computed(() => ui.newGameFrom);
watch(
  () => ui.newGame,
  (open) => {
    if (open && fromPosition.value) opponent.value = "dameless";
    if (open && !version.value && versions.current) version.value = String(versions.current.id);
  },
);

const opponents = computed(() =>
  fromPosition.value
    ? [{ value: "dameless" as const, label: "Dameless" }]
    : [
        { value: "dameless" as const, label: "Dameless" },
        { value: "lichess" as const, label: "Lichess", disabled: !connections.you.configured },
        { value: "stockfish" as const, label: "Stockfish", disabled: !connections.you.configured },
      ],
);
/** Lichess's Board API plays no bullet; a seek must be rapid or slower (initial + 40 × increment ≥ 8 min). */
const clocks = computed(() =>
  opponent.value === "lichess" && lichessHow.value === "seek"
    ? ["10+0", "10+5", "15+10", "30+0"]
    : ["3+2", "5+3", "10+5", "15+10"],
);
watch(clocks, (c) => {
  if (!c.includes(clock.value)) clock.value = c[1]!;
});
const versionOptions = computed(() =>
  [...versions.line]
    .reverse()
    .map((v) => ({ value: String(v.id), label: v.id === versions.current?.id ? `${v.name} (champion)` : v.name })),
);

async function start() {
  const time = readClock(clock.value) ?? { type: "clock" as const, initial_ms: 300_000, increment_ms: 3000 };
  busy.value = true;
  try {
    if (opponent.value === "dameless") {
      const you = color.value === "black" ? "black" : "white";
      const s = await games.start({
        white: you === "white" ? "you" : "dameless",
        black: you === "white" ? "dameless" : "you",
        color: color.value,
        time,
        version: version.value ? Number(version.value) : undefined,
        from: fromPosition.value ?? undefined,
      });
      ui.newGame = false;
      await router.push({ path: "/", query: { game: String(s.id) } });
    } else if (opponent.value === "stockfish") {
      await connections.challenge("you", { ai_level: Number(level.value), time, rated: false, color: color.value });
      ui.newGame = false;
      toast.show({ title: `Starting a game against Stockfish level ${level.value}`, tone: "info" });
    } else if (lichessHow.value === "seek") {
      await connections.seek({ time, rated: rated.value, color: color.value });
      ui.newGame = false;
      toast.show({
        title: "Looking for a game on Lichess",
        message: "It opens on the board when someone takes it.",
        tone: "info",
      });
    } else {
      await connections.challenge("you", {
        username: username.value.trim(),
        time,
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
  <EDialog
    v-model:open="ui.newGame"
    :title="fromPosition ? 'Play from this position' : 'New game'"
    :description="fromPosition ? 'Against Dameless, from the position on the board.' : undefined"
    size="md"
  >
    <EStack gap="base">
      <ESegmented v-model="opponent" label="Opponent" :options="opponents" />
      <template v-if="opponent === 'dameless'">
        <ESelect v-if="versionOptions.length" v-model="version" label="Version" :options="versionOptions" />
      </template>
      <template v-else-if="opponent === 'stockfish'">
        <ESelect v-model="level" label="Level" :options="['1', '2', '3', '4', '5', '6', '7', '8']" />
      </template>
      <template v-else>
        <ESegmented
          v-model="lichessHow"
          label="How"
          :options="[
            { value: 'seek', label: 'Find a game' },
            { value: 'challenge', label: 'Challenge someone' },
          ]"
        />
        <ETextField v-if="lichessHow === 'challenge'" v-model="username" label="Lichess username" autocomplete="off" />
      </template>
      <ESegmented v-model="clock" label="Time control" :options="clocks" hint="Minutes + seconds added each move." />
      <ESegmented
        v-model="color"
        label="You play"
        :options="[
          { value: 'white', label: 'White' },
          { value: 'random', label: 'Either' },
          { value: 'black', label: 'Black' },
        ]"
      />
      <ESwitch v-if="opponent === 'lichess'" v-model="rated" label="Rated" />
    </EStack>
    <template #actions>
      <EButton
        :loading="busy"
        :disabled="opponent === 'lichess' && lichessHow === 'challenge' && !username.trim()"
        @click="start"
      >
        {{ opponent === "lichess" && lichessHow === "seek" ? "Find a game" : "Start" }}
      </EButton>
      <EButton variant="outline" @click="ui.newGame = false">Cancel</EButton>
    </template>
  </EDialog>
</template>
