import { createI18n } from "vue-i18n";
import en from "./en";
import zh from "./zh";
import zhTW from "./zh-TW";
import ja from "./ja";

export type LocaleKey = keyof typeof zh;

const i18n = createI18n({
  legacy: false,
  locale: "zh",
  fallbackLocale: "en",
  messages: { en, zh, "zh-TW": zhTW, ja },
});

export function setLocale(locale: string) {
  i18n.global.locale.value = locale as "zh" | "en" | "zh-TW" | "ja";
  document.documentElement.lang = locale;
}

export default i18n;