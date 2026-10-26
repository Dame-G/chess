import { createRouter, createWebHistory } from "vue-router";

const board = () => import("./views/BoardView.vue");
const lab = () => import("./views/LabView.vue");
const engines = () => import("./views/EnginesView.vue");
const tune = () => import("./views/TuneView.vue");

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", name: "home", component: () => import("./views/HomeView.vue"), meta: { workspace: true } },

    // The board: free analysis, a played game with its test's games as the playlist, or a match between two builds.
    { path: "/board", name: "board", component: board, meta: { title: "Board", workspace: true } },
    { path: "/games/:test/:index", name: "game", component: board, meta: { title: "Game", workspace: true } },
    { path: "/matches/:id", name: "match", component: board, meta: { title: "Match", workspace: true } },

    // The lab: experiments in the library, the chosen one on the stage.
    { path: "/lab", name: "lab", component: lab, meta: { title: "Lab", workspace: true } },
    { path: "/lab/:id", name: "experiment", component: lab, meta: { title: "Lab", workspace: true } },

    { path: "/engines", name: "engines", component: engines, meta: { title: "Engines", workspace: true } },
    { path: "/engines/:id", name: "engine", component: engines, meta: { title: "Engines", workspace: true } },

    // Tunes: SPSA runs that nudge the engine's search constants.
    { path: "/tune", name: "tune", component: tune, meta: { title: "Tune", workspace: true } },
    { path: "/tune/:id", name: "tune-run", component: tune, meta: { title: "Tune", workspace: true } },

    // The Lichess bot: its games, rules and challenges.
    {
      path: "/lichess",
      name: "lichess",
      component: () => import("./views/LichessView.vue"),
      meta: { title: "Lichess", workspace: true },
    },

    // Old addresses.
    { path: "/analysis", redirect: (to) => ({ path: "/board", query: to.query }) },
    { path: "/experiments", redirect: "/lab" },
    { path: "/experiments/:id", redirect: (to) => `/lab/${String(to.params.id)}` },
    { path: "/tests/:id/games/:index", redirect: (to) => `/games/${String(to.params.id)}/${String(to.params.index)}` },
    { path: "/runner", redirect: "/" },
    { path: "/:rest(.*)*", redirect: "/" },
  ],
});

router.afterEach((to) => {
  document.title = to.meta.title ? `${to.meta.title} · Dameless` : "Dameless";
});

declare module "vue-router" {
  interface RouteMeta {
    title?: string;
    /** Fills the window with panes that scroll on their own, instead of scrolling as a page. */
    workspace?: boolean;
  }
}
