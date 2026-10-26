import { createRouter, createWebHistory } from "vue-router";

const board = () => import("./views/BoardView.vue");
const improve = () => import("./views/ImproveView.vue");
const engines = () => import("./views/EnginesView.vue");

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", name: "home", component: () => import("./views/HomeView.vue"), meta: { workspace: true } },

    // The board: free analysis, a played game with its test's games as the playlist, or a match between two builds.
    { path: "/board", name: "board", component: board, meta: { title: "Board", workspace: true } },
    { path: "/games/:test/:index", name: "game", component: board, meta: { title: "Game", workspace: true } },
    { path: "/matches/:id", name: "match", component: board, meta: { title: "Match", workspace: true } },

    // Improve: tests and tunes in one library, the chosen one on the stage.
    { path: "/improve", name: "improve", component: improve, meta: { title: "Improve", workspace: true } },
    {
      path: "/improve/experiments/:id",
      name: "improve-test",
      component: improve,
      meta: { title: "Improve", workspace: true },
    },
    {
      path: "/improve/tunes/:id",
      name: "improve-tune",
      component: improve,
      meta: { title: "Improve", workspace: true },
    },

    // Versions: the engine builds. (Still the engines page, for now.)
    { path: "/versions", name: "engines", component: engines, meta: { title: "Versions", workspace: true } },
    { path: "/versions/:id", name: "engine", component: engines, meta: { title: "Versions", workspace: true } },

    // The Lichess bot: its games, rules and challenges.
    {
      path: "/lichess",
      name: "lichess",
      component: () => import("./views/LichessView.vue"),
      meta: { title: "Lichess", workspace: true },
    },

    // Old addresses.
    { path: "/analysis", redirect: (to) => ({ path: "/board", query: to.query }) },
    { path: "/lab", redirect: (to) => ({ path: "/improve", query: to.query }) },
    { path: "/lab/:id", redirect: (to) => ({ path: `/improve/experiments/${String(to.params.id)}`, query: to.query }) },
    { path: "/experiments", redirect: "/improve" },
    { path: "/experiments/:id", redirect: (to) => `/improve/experiments/${String(to.params.id)}` },
    // A new tune was ?new=1 on the old tune page: it is ?new=tune here.
    {
      path: "/tune",
      redirect: (to) => ({ path: "/improve", query: to.query.new ? { ...to.query, new: "tune" } : to.query }),
    },
    { path: "/tune/:id", redirect: (to) => ({ path: `/improve/tunes/${String(to.params.id)}`, query: to.query }) },
    { path: "/engines", redirect: (to) => ({ path: "/versions", query: to.query }) },
    { path: "/engines/:id", redirect: (to) => ({ path: `/versions/${String(to.params.id)}`, query: to.query }) },
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
