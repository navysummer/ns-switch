import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import router from "./router";
import i18n, { setLocale } from "./locales";
import "./styles/index.css";

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.use(i18n);

setLocale((localStorage.getItem("ns-switch-locale") as "zh" | "en") || "zh");

app.mount("#app");