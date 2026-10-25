import { createRouter, createWebHistory } from "vue-router";

export const router = createRouter({
  history: createWebHistory(),
  routes: [{ path: "/", name: "runner", component: () => import("./views/RunnerView.vue"), meta: { title: "Runner" } }],
});

router.afterEach((to) => {
  document.title = to.meta.title ? `${to.meta.title} · chess` : "chess";
});

declare module "vue-router" {
  interface RouteMeta {
    title?: string;
  }
}
