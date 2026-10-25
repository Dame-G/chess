import { createRouter, createWebHistory } from "vue-router";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", name: "runner", component: () => import("./views/RunnerView.vue"), meta: { title: "Runner" } },
    {
      path: "/engines",
      name: "engines",
      component: () => import("./views/EnginesView.vue"),
      meta: { title: "Engines" },
    },
    {
      path: "/experiments",
      name: "experiments",
      component: () => import("./views/ExperimentsView.vue"),
      meta: { title: "Experiments" },
    },
    {
      path: "/experiments/:id",
      name: "experiment",
      component: () => import("./views/ExperimentView.vue"),
      meta: { title: "Experiment" },
    },
    {
      path: "/tests/:id/games/:index",
      name: "game",
      component: () => import("./views/GameView.vue"),
      meta: { title: "Game" },
    },
    {
      path: "/analysis",
      name: "analysis",
      component: () => import("./views/AnalysisView.vue"),
      meta: { title: "Analysis" },
    },
  ],
});

router.afterEach((to) => {
  document.title = to.meta.title ? `${to.meta.title} · chess` : "chess";
});

declare module "vue-router" {
  interface RouteMeta {
    title?: string;
  }
}
