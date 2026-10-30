<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { useToast } from "@eos-ui/vue";
import { ECommandPalette, type CommandItem } from "@eos-ui/vue/components/ECommandPalette";
import type { GameSummary, Side } from "../api/types";
import { ago, resultText, seatName, timeControl } from "../format";
import { useBoard } from "../board/board";
import { useDrills } from "../stores/drills";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { usePolicy } from "../stores/policy";
import { useVersions } from "../stores/versions";
import { choice, shown } from "../scheme";
import { readVerb, type Verb } from "./commands";
import { useStartDrills } from "./drillFlow";
import { useUi } from "./ui";

/** The command bar: places, games, ideas, versions, and verbs in the CLI's words (`play 5+3 dameless`). */
const ui = useUi();
const router = useRouter();
const board = useBoard();
const games = useGames();
const ideas = useIdeas();
const versions = useVersions();
const toast = useToast();
const drills = useDrills();
const startDrills = useStartDrills();

const query = ref("");

const places: CommandItem[] = [
  { id: "go:/", label: "Board", group: "Places", keywords: ["home", "play"] },
  { id: "go:/you", label: "You", description: "Your profile, trend and weak spots", group: "Places", shortcut: "g y" },
  {
    id: "go:/dameless",
    label: "Dameless",
    description: "The engine: champion, ideas, the bot",
    group: "Places",
    shortcut: "g d",
  },
  { id: "go:/compare", label: "Compare players", group: "Places", keywords: ["compare"] },
  { id: "go:/settings", label: "Settings", group: "Places", shortcut: "g s" },
];

const policy = usePolicy();
const verbs = computed<CommandItem[]>(() => [
  { id: "do:new-game", label: "New game", group: "Do", shortcut: "n", keywords: ["play"] },
  ...(drills.available
    ? [
        {
          id: "do:drills",
          label: "Train",
          group: "Do",
          shortcut: "d",
          keywords: ["drills", "practice", "mistakes", "themes"],
        },
      ]
    : []),
  ...(policy.allowed
    ? [
        {
          id: "do:review-20",
          label: "Review your last 20 games",
          group: "Do",
          keywords: ["review", "accuracy"],
        },
      ]
    : []),
  { id: "do:flip", label: "Flip the board", group: "Do", shortcut: "f" },
  { id: "do:shortcuts", label: "Keyboard shortcuts", group: "Do", shortcut: "?" },
  { id: "do:new-idea", label: "New idea", group: "Do", keywords: ["test", "experiment"] },
  {
    id: `do:theme-${shown.value === "dark" ? "light" : "dark"}`,
    label: shown.value === "dark" ? "Light theme" : "Dark theme",
    group: "Do",
    keywords: ["scheme", "mode"],
  },
]);

/** The typed verb, as the first command, so Enter runs it. */
const typed = computed<CommandItem[]>(() => {
  const v = readVerb(query.value);
  if (!v || (v.verb === "drills" && !drills.available) || (v.verb === "review" && !policy.allowed)) return [];
  const label = describe(v);
  return [
    { id: `verb:${JSON.stringify(v)}`, label, description: query.value.trim(), group: "Do", keywords: [query.value] },
  ];
});

function describe(v: Verb): string {
  switch (v.verb) {
    case "play":
      return `Play ${v.time ? timeControl(v.time) + " " : ""}against ${v.opponent === "stockfish" ? `Stockfish level ${v.level}` : v.opponent === "lichess" ? "someone on Lichess" : "Dameless"}`;
    case "review":
      return `Review your last ${v.last} games`;
    case "compare":
      return `Compare with ${v.players.join(", ")}`;
    case "new-idea":
      return "New idea";
    case "drills":
      return "Start today's drills";
  }
}

const name = (g: GameSummary, side: Side) => (g.you === side ? "You" : seatName(g[side]));
const gameItems = computed<CommandItem[]>(() =>
  games.list
    .filter((g) => g.source.type !== "trial")
    .slice(0, 40)
    .map((g) => ({
      id: `game:${g.id}`,
      label: `${name(g, "white")} vs ${name(g, "black")}`,
      description: `${g.status === "live" ? "live" : resultText(g.outcome)} · ${timeControl(g.time)} · ${ago(g.started_at)}`,
      group: "Games",
      keywords: [g.opening?.name ?? "", g.white.name, g.black.name],
    })),
);
const ideaItems = computed<CommandItem[]>(() =>
  ideas.list.map((i) => ({
    id: `go:/dameless/ideas/${i.id}`,
    label: i.title,
    description: `${i.status} · ${i.branch}`,
    group: "Ideas",
    keywords: [i.branch, i.kind, ...i.tags],
  })),
);
const versionItems = computed<CommandItem[]>(() =>
  versions.list.slice(0, 30).map((v) => ({
    id: `version:${v.id}`,
    label: v.name,
    description:
      v.line.type === "champion" ? `Champion line · ${ago(v.created_at)}` : `${v.line.type} · ${ago(v.created_at)}`,
    group: "Versions",
    keywords: [v.commit.slice(0, 7), v.branch ?? ""],
  })),
);

const items = computed(() => [
  ...typed.value,
  ...places,
  ...verbs.value,
  ...gameItems.value,
  ...ideaItems.value,
  ...versionItems.value,
]);

async function runVerb(v: Verb) {
  if (v.verb === "play") {
    if (v.opponent !== "dameless") {
      ui.startNewGame();
      return;
    }
    const youWhite = Math.random() < 0.5;
    const s = await games.start({
      white: youWhite ? "you" : "dameless",
      black: youWhite ? "dameless" : "you",
      time: v.time ?? { type: "clock", initial_ms: 300_000, increment_ms: 3000 },
    });
    await router.push({ path: "/", query: { game: String(s.id) } });
  } else if (v.verb === "review") {
    await games.reviewPlayer("you", v.last);
    toast.show({
      title: `Reviewing your last ${v.last} games`,
      message: "The work indicator in the header shows how it goes.",
      tone: "info",
    });
  } else if (v.verb === "compare") {
    await router.push({
      path: "/compare",
      query: { players: ["you", ...v.players.map((p) => `lichess:${p}`)].join(",") },
    });
  } else if (v.verb === "drills") {
    await startDrills();
  } else {
    await router.push({ path: "/dameless", query: { new: "idea" } });
  }
}

async function select(item: CommandItem) {
  const [kind, ...rest] = item.id.split(":");
  const arg = rest.join(":");
  try {
    if (kind === "go") await router.push(arg === "/" ? { path: "/", query: board.address } : arg);
    else if (kind === "game") await router.push({ path: "/", query: { game: arg } });
    else if (kind === "version") await router.push("/dameless");
    else if (kind === "verb") await runVerb(JSON.parse(arg) as Verb);
    else if (arg === "new-game") ui.startNewGame();
    else if (arg === "drills") await startDrills();
    else if (arg === "review-20") await runVerb({ verb: "review", last: 20 });
    else if (arg === "flip") board.flip();
    else if (arg === "shortcuts") ui.shortcuts = true;
    else if (arg === "new-idea") await router.push({ path: "/dameless", query: { new: "idea" } });
    else if (arg === "theme-dark") choice.value = "dark";
    else if (arg === "theme-light") choice.value = "light";
  } catch (e) {
    toast.show({
      title: "That did not go through",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  }
}
</script>

<template>
  <ECommandPalette
    v-model:open="ui.palette"
    v-model:query="query"
    :items="items"
    label="Command bar"
    placeholder="Go to, find a game, or type: play 5+3 dameless"
    @select="select"
  />
</template>
