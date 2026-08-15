<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { FolderOpen, RotateCcw, Search } from "lucide-vue-next";
import { api } from "@/utils/api";
import { useProvidersStore } from "@/stores/providers";
import { useSettingsStore } from "@/stores/settings";
import type { SessionItem } from "@/types";

const { t } = useI18n();
const providers = useProvidersStore();
const settings = useSettingsStore();

const sessions = ref<SessionItem[]>([]);
const loading = ref(false);
const query = ref("");

const toolId = ref("claude_code");

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return sessions.value;
  return sessions.value.filter(
    (s) =>
      (s.title || "").toLowerCase().includes(q) ||
      (s.cwd || "").toLowerCase().includes(q),
  );
});

async function load() {
  loading.value = true;
  try {
    sessions.value = await api.listSessions(toolId.value);
  } finally {
    loading.value = false;
  }
}

function fmtTime(ts: string) {
  if (!ts) return "-";
  const d = new Date(ts);
  return d.toLocaleString();
}

async function setTool(id: string) {
  toolId.value = id;
  await load();
}

onMounted(async () => {
  await providers.loadTools();
  await load();
});
</script>

<template>
  <div class="p-6" v-if="settings.loaded">
    <div class="mb-5 flex items-center justify-between">
      <h1 class="text-xl font-semibold">{{ t("nav.sessions") }}</h1>
      <div class="flex items-center gap-3">
        <div class="relative">
          <Search class="pointer-events-none absolute left-2.5 top-2.5 h-4 w-4 text-slate-400" />
          <input
            v-model="query"
            :placeholder="t('common.search')"
            class="rounded-lg border border-slate-300 py-2 pl-8 pr-3 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"
          />
        </div>
        <select
          v-model="toolId"
          class="rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"
          @change="setTool(toolId)"
        >
          <option v-for="tool in providers.tools" :key="tool.id" :value="tool.id">
            {{ t(`tools.${tool.id}`) }}
          </option>
        </select>
      </div>
    </div>

    <div v-if="loading" class="text-sm text-slate-400">...</div>
    <div v-else-if="sessions.length === 0" class="rounded-lg border border-dashed border-slate-300 p-10 text-center text-sm text-slate-400">
      {{ t("sessions.noSource") }}
    </div>
    <div v-else>
      <div class="mb-2 text-xs text-slate-400">{{ filtered.length }} sessions</div>
      <div class="overflow-hidden rounded-xl border border-slate-200 dark:border-slate-800">
        <table class="w-full text-sm">
          <thead class="bg-slate-100 text-left text-xs text-slate-500 dark:bg-slate-800/60 dark:text-slate-400">
            <tr>
              <th class="px-4 py-2.5 font-medium">{{ t("prompts.title") }}</th>
              <th class="px-4 py-2.5 font-medium">{{ t("sessions.cwd") }}</th>
              <th class="px-4 py-2.5 font-medium">{{ t("sessions.messages") }}</th>
              <th class="px-4 py-2.5 font-medium">{{ t("sessions.updated") }}</th>
              <th class="px-4 py-2.5 text-right font-medium">{{ t("common.more") }}</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-slate-100 dark:divide-slate-800">
            <tr v-for="s in filtered" :key="s.id" class="bg-white dark:bg-slate-900">
              <td class="max-w-[260px] truncate px-4 py-3 font-medium">{{ s.title || "-" }}</td>
              <td class="max-w-[200px] truncate px-4 py-3 font-mono text-xs text-slate-500">{{ s.cwd }}</td>
              <td class="px-4 py-3 text-xs text-slate-500">{{ s.messageCount }}</td>
              <td class="px-4 py-3 text-xs text-slate-400">{{ fmtTime(s.updatedAt) }}</td>
              <td class="px-4 py-3 text-right">
                <div class="flex items-center justify-end gap-1">
                  <button
                    class="inline-flex items-center gap-1 rounded bg-emerald-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-emerald-700"
                    @click="api.restoreSession(s.path).catch(() => {})"
                  >
                    <RotateCcw class="h-3 w-3" /> {{ t("sessions.restore") }}
                  </button>
                  <button
                    class="inline-flex items-center gap-1 rounded bg-slate-800 px-2.5 py-1 text-xs font-medium text-white hover:bg-slate-700 dark:bg-white dark:text-slate-800"
                    @click="api.openPath(s.cwd || s.path)"
                  >
                    <FolderOpen class="h-3 w-3" /> {{ t("sessions.open") }}
                  </button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>