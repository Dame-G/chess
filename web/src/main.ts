import { createApp } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
// Components are imported by their own paths, never the barrel, so each route only loads what it uses (as eos-site does).
import { createEos } from "@eos-ui/vue/plugin";
import "@eos-ui/vue/icons/builtin";
import "@eos-ui/vue/style.css";
// Self-hosted, so the UI the runner embeds works offline.
import "@fontsource-variable/fraunces/opsz.css";
import "@fontsource-variable/hanken-grotesk";
import "@fontsource-variable/jetbrains-mono";
import "./base.css";
import App from "./App.vue";
import { router } from "./router";
import { startScheme } from "./scheme";
import { saddleLight, themeCss } from "./theme";

// Both schemes ship as CSS, chosen by data-scheme on <html>; index.html sets it before the first paint.
startScheme(themeCss);

createApp(App)
  .use(createEos({ locale: "en-US", theme: saddleLight, applyTheme: false }))
  .use(createPinia())
  .use(PiniaColada, { queryOptions: { staleTime: 5_000 } })
  .use(router)
  .mount("#app");
