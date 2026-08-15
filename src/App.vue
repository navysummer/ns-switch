<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useRouter, useRoute } from "vue-router";
import {
  Cable,
  CloudCog,
  History,
  MessagesSquare,
  Moon,
  PieChart,
  Settings,
  Sun,
  WandSparkles,
  FolderTree,
  X,
  Sparkles,
  Download,
} from "lucide-vue-next";
import { useSettingsStore } from "@/stores/settings";
import { useProvidersStore } from "@/stores/providers";
import { api, listen } from "@/utils/api";

const router = useRouter();
const route = useRoute();
const settings = useSettingsStore();
const providers = useProvidersStore();

const showOnboarding = ref(false);
const importing = ref(false);
const toast = ref("");
const toastTimer = ref<number | null>(null);

const nav = [
  { to: "/providers", icon: CloudCog, key: "providers" },
  { to: "/mcp", icon: Cable, key: "mcp" },
  { to: "/skills", icon: WandSparkles, key: "skills" },
  { to: "/prompts", icon: MessagesSquare, key: "prompts" },
  { to: "/sessions", icon: History, key: "sessions" },
  { to: "/usage", icon: PieChart, key: "usage" },
  { to: "/workspace", icon: FolderTree, key: "workspace" },
  { to: "/settings", icon: Settings, key: "settings" },
];

async function toggleTheme() {
  const next = settings.theme === "dark" ? "light" : "dark";
  await settings.update({ theme: next });
}

function notifyToast(msg: string) {
  toast.value = msg;
  if (toastTimer.value) window.clearTimeout(toastTimer.value);
  toastTimer.value = window.setTimeout(() => (toast.value = ""), 6000);
}

async function doImportExisting() {
  importing.value = true;
  try {
    const imported = await api.importAllFromCli();
    await api.completeOnboarding();
    showOnboarding.value = false;
    notifyToast(`Imported ${imported.length} existing configuration(s)`);
    await providers.refreshAll();
  } finally {
    importing.value = false;
  }
}

async function skipOnboarding() {
  await api.completeOnboarding();
  showOnboarding.value = false;
  router.push("/providers");
}

onMounted(async () => {
  try {
    await settings.load();
  } catch (e) {
    console.error("settings.load failed:", e);
  }
  try {
    await providers.loadTools();
  } catch (e) {
    console.error("providers.loadTools failed:", e);
  }
  try {
    showOnboarding.value = await api.onboardingStatus();
  } catch {
    showOnboarding.value = false;
  }
  listen<string>("deeplink-imported", (msg) => notifyToast(msg));
});
</script>

<template>
  <div class="flex h-full bg-slate-50 text-slate-800 dark:bg-slate-950 dark:text-slate-200">
    <aside
      class="flex w-56 shrink-0 flex-col border-r border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900"
    >
      <div class="flex items-center gap-2.5 px-5 py-4">
        <div
          class="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-cyan-500 to-blue-600 text-sm font-bold text-white shadow"
        >
          NS
        </div>
        <div>
          <div class="text-sm font-semibold leading-tight">NS Switch</div>
          <div class="text-[11px] leading-tight text-slate-400">{{ $t("app.tagline") }}</div>
        </div>
      </div>

      <nav class="mt-2 flex-1 space-y-1 px-3">
        <button
          v-for="item in nav"
          :key="item.to"
          class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors"
          :class="
            route.path === item.to
              ? 'bg-cyan-50 text-cyan-700 dark:bg-cyan-950/40 dark:text-cyan-300'
              : 'text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800'
          "
          @click="router.push(item.to)"
        >
          <component :is="item.icon" class="h-4 w-4" />
          {{ $t(`nav.${item.key}`) }}
        </button>
      </nav>

      <div class="border-t border-slate-200 p-3 dark:border-slate-800">
        <button
          class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800"
          @click="toggleTheme"
        >
          <Moon v-if="settings.theme !== 'light'" class="h-4 w-4" />
          <Sun v-else class="h-4 w-4" />
          {{ settings.theme === 'dark' ? $t('settings.dark') : $t('settings.light') }}
        </button>
      </div>
    </aside>

    <main class="flex-1 overflow-y-auto">
      <router-view />
    </main>

    <div
      v-if="toast"
      class="fixed bottom-5 left-1/2 z-50 -translate-x-1/2 rounded-full border border-emerald-300 bg-emerald-50 px-4 py-2 text-sm text-emerald-700 shadow-lg dark:border-emerald-900 dark:bg-emerald-950/90 dark:text-emerald-300"
    >
      {{ toast }}
    </div>

    <div
      v-if="showOnboarding"
      class="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/60 backdrop-blur-sm"
    >
      <div class="w-[520px] rounded-2xl bg-white p-8 shadow-2xl dark:bg-slate-900">
        <div class="flex items-start justify-between">
          <div>
            <div class="text-2xl font-semibold">NS Switch</div>
            <div class="mt-1 text-sm text-slate-400">{{ $t("app.tagline") }}</div>
          </div>
          <button class="text-slate-400 hover:text-slate-600" @click="skipOnboarding">
            <X class="h-5 w-5" />
          </button>
        </div>
        <div class="mt-6 space-y-3 text-sm text-slate-600 dark:text-slate-300">
          <div class="flex items-start gap-2">
            <Sparkles class="mt-0.5 h-4 w-4 text-cyan-500" />
            {{ $t("app.onboarding.b1") }}
          </div>
          <div class="flex items-start gap-2">
            <Sparkles class="mt-0.5 h-4 w-4 text-cyan-500" />
            {{ $t("app.onboarding.b2") }}
          </div>
          <div class="flex items-start gap-2">
            <Sparkles class="mt-0.5 h-4 w-4 text-cyan-500" />
            {{ $t("app.onboarding.b3") }}
          </div>
        </div>
        <div class="mt-8 flex gap-3">
          <button
            class="inline-flex flex-1 items-center justify-center gap-2 rounded-lg bg-cyan-600 px-4 py-2.5 text-sm font-medium text-white hover:bg-cyan-700 disabled:opacity-50"
            :disabled="importing"
            @click="doImportExisting"
          >
            <Download class="h-4 w-4" />
            {{ $t("app.onboarding.import") }}
          </button>
          <button
            class="flex-1 rounded-lg border border-slate-300 px-4 py-2.5 text-sm font-medium hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
            @click="skipOnboarding"
          >
            {{ $t("app.onboarding.skip") }}
          </button>
        </div>
        <p class="mt-3 text-center text-xs text-slate-400">{{ $t("app.onboarding.note") }}</p>
      </div>
    </div>
  </div>
</template>