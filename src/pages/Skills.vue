<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { Plus, Trash2, X } from "lucide-vue-next";
import { api } from "@/utils/api";
import { useSkillsStore } from "@/stores/skills";
import { useSettingsStore } from "@/stores/settings";
import type { SkillSource } from "@/types";

const { t } = useI18n();
const store = useSkillsStore();
const settings = useSettingsStore();

const installOpen = ref(false);
const sourcesOpen = ref(false);
const source = ref("");
const mode = ref("symlink");
const error = ref("");

const sources = ref<SkillSource[]>([]);
const newSourceName = ref("");
const newSourceUrl = ref("");

async function install() {
  error.value = "";
  try {
    await api.skillInstall(source.value.trim(), mode.value);
    source.value = "";
    installOpen.value = false;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

async function installFromSource(url: string) {
  error.value = "";
  try {
    await api.skillInstall(url, mode.value);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

async function remove(s: { name: string; id: number }) {
  if (confirm(t("skills.deleteConfirm", { name: s.name }))) await store.remove(s.id);
}

async function loadSources() {
  sources.value = await api.skillSourcesList();
}

async function addSource() {
  if (!newSourceName.value.trim() || !newSourceUrl.value.trim()) return;
  await api.skillSourceAdd(newSourceName.value.trim(), newSourceUrl.value.trim());
  newSourceName.value = "";
  newSourceUrl.value = "";
  await loadSources();
}

async function removeSource(id?: number) {
  if (!id) return;
  await api.skillSourceRemove(id);
  await loadSources();
}

onMounted(() => {
  store.load();
  loadSources();
});
</script>

<template>
  <div class="p-6" v-if="settings.loaded">
    <div class="mb-5 flex items-center justify-between">
      <h1 class="text-xl font-semibold">{{ t("nav.skills") }}</h1>
      <div class="flex items-center gap-2">
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          @click="sourcesOpen = true"
        >
          {{ t("skills.onlineRepos") }}
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-cyan-700"
          @click="installOpen = true"
        >
          {{ t("common.install") }}
        </button>
      </div>
    </div>

    <p v-if="error" class="mb-3 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600 dark:bg-red-950/40 dark:text-red-300">{{ error }}</p>

    <div v-if="store.list.length === 0" class="rounded-lg border border-dashed border-slate-300 p-10 text-center text-sm text-slate-400">
      {{ t("skills.none") }}
    </div>

    <div class="grid grid-cols-1 gap-3 lg:grid-cols-2">
      <div v-for="s in store.list" :key="s.id" class="group flex items-start justify-between rounded-xl border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
        <div class="min-w-0">
          <div class="flex items-center gap-2">
            <span class="font-medium">{{ s.name }}</span>
            <span class="rounded-full bg-emerald-100 px-2 py-0.5 text-[10px] text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300">
              {{ t("skills.installed") }}
            </span>
            <span class="rounded-full bg-slate-200 px-2 py-0.5 text-[10px] text-slate-500 dark:bg-slate-800 dark:text-slate-400">
              {{ s.installMode === "copy" ? t("skills.copy") : t("skills.symlink") }}
            </span>
          </div>
          <p v-if="s.description" class="mt-1 line-clamp-2 text-xs text-slate-400">{{ s.description }}</p>
          <div class="mt-1.5 truncate text-xs text-slate-400">{{ s.source }}</div>
        </div>
        <button class="rounded p-1.5 text-slate-300 opacity-0 transition-opacity hover:bg-red-50 hover:text-red-500 group-hover:opacity-100 dark:hover:bg-red-950" @click="remove(s)">
          <Trash2 class="h-4 w-4" />
        </button>
      </div>
    </div>

    <div v-if="installOpen" class="fixed inset-0 z-40 flex items-center justify-center bg-black/40" @click.self="installOpen = false">
      <div class="w-[520px] rounded-2xl bg-white p-6 shadow-xl dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">{{ t("skills.install") }}</h2>
          <button class="text-slate-400 hover:text-slate-600" @click="installOpen = false"><X class="h-5 w-5" /></button>
        </div>
        <template v-if="sources.length">
          <label class="mb-1 block text-sm font-medium">{{ t("skills.onlineRepos") }}</label>
          <div class="mb-3 max-h-40 space-y-1 overflow-y-auto">
            <div v-for="rs in sources" :key="rs.id" class="flex items-center justify-between rounded-lg border border-slate-200 px-3 py-1.5 text-sm dark:border-slate-700">
              <span class="truncate">{{ rs.name }}</span>
              <button class="rounded bg-slate-800 px-2 py-0.5 text-[11px] font-medium text-white hover:bg-slate-700 dark:bg-white dark:text-slate-800" @click="installFromSource(rs.url)">
                {{ t("common.install") }}
              </button>
            </div>
          </div>
        </template>
        <label class="mb-1 block text-sm font-medium">{{ t("skills.source") }}</label>
        <input v-model="source" :placeholder="t('skills.sourceHint')" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
        <label class="mb-1 mt-3 block text-sm font-medium">{{ t("skills.mode") }}</label>
        <div class="flex gap-2">
          <button
            v-for="m in (['symlink', 'copy'] as const)"
            :key="m"
            class="rounded-lg px-3 py-1.5 text-sm font-medium"
            :class="mode === m ? 'bg-cyan-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300'"
            @click="mode = m"
          >
            {{ t(`skills.${m}`) }}
          </button>
        </div>
        <p v-if="error" class="mt-2 text-xs text-red-500">{{ error }}</p>
        <div class="mt-4 flex justify-end gap-2">
          <button class="rounded-lg border border-slate-300 px-4 py-2 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800" @click="installOpen = false">{{ t("common.cancel") }}</button>
          <button class="rounded-lg bg-cyan-600 px-4 py-2 text-sm font-medium text-white hover:bg-cyan-700" @click="install">{{ t("common.install") }}</button>
        </div>
      </div>
    </div>

    <div v-if="sourcesOpen" class="fixed inset-0 z-40 flex items-center justify-center bg-black/40" @click.self="sourcesOpen = false">
      <div class="w-[520px] rounded-2xl bg-white p-6 shadow-xl dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">{{ t("skills.onlineRepos") }}</h2>
          <button class="text-slate-400 hover:text-slate-600" @click="sourcesOpen = false"><X class="h-5 w-5" /></button>
        </div>
        <div class="space-y-2">
          <div
            v-for="rs in sources"
            :key="rs.id"
            class="flex items-center justify-between rounded-lg border border-slate-200 px-3 py-2 text-sm dark:border-slate-700"
          >
            <div class="min-w-0">
              <div class="truncate font-medium">{{ rs.name }}</div>
              <div class="truncate text-xs text-slate-400">{{ rs.url }}</div>
            </div>
            <button class="rounded p-1.5 text-red-400 hover:bg-red-50 dark:hover:bg-red-950" @click="removeSource(rs.id)">
              <Trash2 class="h-3.5 w-3.5" />
            </button>
          </div>
          <div class="flex gap-2">
            <input v-model="newSourceName" :placeholder="t('skills.repoName')" class="flex-1 rounded-lg border border-slate-300 px-3 py-1.5 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
            <input v-model="newSourceUrl" :placeholder="t('skills.repoUrl')" class="flex-1 rounded-lg border border-slate-300 px-3 py-1.5 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
            <button class="rounded-lg bg-cyan-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-cyan-700" @click="addSource">
              <Plus class="h-4 w-4" />
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>