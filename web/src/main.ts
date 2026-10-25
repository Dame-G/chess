import { createApp } from "vue";
import { createPinia } from "pinia";
import { PiniaColada } from "@pinia/colada";
// Components are imported by their own paths, never the barrel, so each route only loads what it uses (as eos-site does).
import { createEos } from "@eos-ui/vue/plugin";
import "@eos-ui/vue/icons/builtin";
import "@eos-ui/vue/style.css";
import "./base.css";
import App from "./App.vue";
import { router } from "./router";

createApp(App)
  .use(createEos({ locale: "en-US", theme: { scheme: "auto", density: "compact" } }))
  .use(createPinia())
  .use(PiniaColada, { queryOptions: { staleTime: 5_000 } })
  .use(router)
  .mount("#app");
