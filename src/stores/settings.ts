import { defineStore } from "pinia";
import { api } from "@/utils/api";
import { setLocale } from "@/locales";
import type { AppSettings } from "@/types";

const applyTheme = (theme: AppSettings["theme"]) => {
  const root = document.documentElement;
  root.classList.remove("light", "dark");
  if (theme === "system") {
    const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    root.classList.toggle("dark", dark);
  } else {
    root.classList.toggle("dark", theme === "dark");
  }
};

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    settings: null as AppSettings | null,
    loaded: false,
  }),
  getters: {
    theme: (s) => s.settings?.theme ?? "system",
    language: (s) => s.settings?.language ?? "zh",
  },
  actions: {
    async load() {
      try {
        this.settings = await api.getSettings();
        applyTheme(this.settings.theme);
        setLocale(this.settings.language);
        localStorage.setItem("ns-switch-locale", this.settings.language);
      } catch (e) {
        console.error("Failed to load settings:", e);
        // Fallback to defaults
        this.settings = {
          theme: "system",
          language: "zh",
          dataDir: "",
          autostart: false,
          proxyPorts: {},
          proxyEnabled: {},
          notifications: true,
          openaiProxyPath: "",
        };
        applyTheme(this.settings.theme);
        setLocale(this.settings.language);
      }
      this.loaded = true;
    },
    async update(patch: Partial<AppSettings>) {
      this.settings = await api.setSettings(patch);
      applyTheme(this.settings.theme);
      setLocale(this.settings.language);
      localStorage.setItem("ns-switch-locale", this.settings.language);
      return this.settings;
    },
  },
});