<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  Check,
  ChevronRight,
  Download,
  GripVertical,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  Upload,
  X,
} from "lucide-vue-next";
import { api } from "@/utils/api";
import { useProvidersStore } from "@/stores/providers";
import { useSettingsStore } from "@/stores/settings";
import type { Provider, ProviderInput } from "@/types";
import { listen } from "@/utils/api";

const { t } = useI18n();
const store = useProvidersStore();
const settings = useSettingsStore();

const isUniversalTab = computed(() => store.activeToolId === "universal");

const toolId = computed({
  get: () => store.activeToolId,
  set: (v: string) => store.selectTool(v),
});

const providers = computed(() => store.activeProviders);

const modalOpen = ref(false);
const isEdit = ref(false);
const form = ref<ProviderInput>(emptyForm("claude_code"));
const formHeadersError = ref("");

const presetsOpen = ref(false);
const importOpen = ref(false);
const importText = ref("");
const menuFor = ref<number | null>(null);
const dragId = ref<number | null>(null);

function emptyForm(tid: string): ProviderInput {
  return {
    toolId: tid,
    name: "",
    baseUrl: "",
    apiKey: "",
    model: "",
    headers: "",
    isUniversal: tid === "universal",
    sharedConfig: "",
    protocol: "",
  };
}

function openAdd() {
  isEdit.value = false;
  form.value = emptyForm(toolId.value);
  formHeadersError.value = "";
  modalOpen.value = true;
}

function openEdit(p: Provider) {
  isEdit.value = true;
  form.value = {
    id: p.id,
    toolId: p.toolId,
    name: p.name,
    baseUrl: p.baseUrl,
    apiKey: p.apiKey,
    model: p.model,
    headers: p.headers,
    isUniversal: p.isUniversal,
    sharedConfig: p.sharedConfig,
    protocol: p.protocol,
  };
  formHeadersError.value = "";
  modalOpen.value = true;
}

function openPresets() {
  store.loadPresets(toolId.value);
  presetsOpen.value = true;
}

async function addFromPreset(p: Provider) {
  const input: ProviderInput = {
    toolId: toolId.value,
    name: p.name,
    baseUrl: p.baseUrl,
    apiKey: p.apiKey,
    model: p.model,
    headers: p.headers,
    isUniversal: isUniversalTab.value,
    sharedConfig: "",
  };
  await store.create(input);
  presetsOpen.value = false;
}

async function save() {
  if (!form.value.name.trim()) return;
  if (form.value.headers.trim()) {
    try {
      JSON.parse(form.value.headers);
    } catch {
      formHeadersError.value = t("common.invalidJson");
      return;
    }
  }
  const shared = form.value.sharedConfig ?? "";
  if (shared.trim()) {
    try {
      JSON.parse(shared);
    } catch {
      formHeadersError.value = t("common.invalidJson");
      return;
    }
  }
  if (isEdit.value) await store.update(form.value);
  else await store.create(form.value);
  modalOpen.value = false;
}

async function extractShared() {
  try {
    const snap = await api.getConfigSnapshot(toolId.value);
    form.value.sharedConfig = snap === "{}" ? "" : snap;
  } catch (e) {
    window.alert(e instanceof Error ? e.message : String(e));
  }
}

async function del(p: Provider) {
  if (p.isActive) {
    window.alert(t("providers.deletedActive"));
    return;
  }
  if (confirm(t("providers.deleteConfirm", { name: p.name }))) {
    await store.remove(toolId.value, p.id);
  }
}

async function duplicate(p: Provider) {
  await api.duplicateProvider(p.id);
  await store.loadProviders(toolId.value);
}

async function doSwitch(p: Provider) {
  await store.switchProvider(toolId.value, p.id);
}

async function resetOfficial() {
  if (confirm(t("providers.resetOfficial") + "?")) {
    await api.resetToOfficial(toolId.value);
    await store.loadProviders(toolId.value);
  }
}

async function exportProviders() {
  const data = await api.exportProviders();
  const blob = new Blob([data], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `ns-switch-providers-${new Date().toISOString().slice(0, 10)}.json`;
  a.click();
  URL.revokeObjectURL(a.href);
}

async function importProviders() {
  try {
    JSON.parse(importText.value);
  } catch {
    window.alert(t("common.invalidJson"));
    return;
  }
  await store.importData(importText.value);
  importOpen.value = false;
  importText.value = "";
}

async function importFromCli() {
  try {
    await api.importFromCli(toolId.value);
    await store.loadProviders(toolId.value);
  } catch (e) {
    window.alert(e instanceof Error ? e.message : String(e));
  }
}

function onDragStart(index: number) {
  dragId.value = providers.value[index].id;
}

function onDragOver(e: Event) {
  e.preventDefault();
}

function onDropAt(index: number) {
  if (dragId.value == null) return;
  const from = providers.value.findIndex((p) => p.id === dragId.value);
  if (from < 0 || from === index) return;
  const ids = providers.value.map((p) => p.id);
  const [moved] = ids.splice(from, 1);
  ids.splice(index, 0, moved);
  void (async () => {
    await api.reorderProviders(toolId.value, ids);
    await store.loadProviders(toolId.value);
  })();
  dragId.value = null;
}

const toolDetails = computed((): any => {
  if (isUniversalTab.value) return undefined;
  return (store.tools as Array<{ id: string; configPath?: string; hasConfig: boolean }>).find(
    (x) => x.id === store.activeToolId,
  );
});

onMounted(() => {
  store.selectTool(store.activeToolId).then(() => store.loadProviders());
  listen<{ toolId: string; providerId: number }>("provider-switched", (e) => {
    if (e.toolId === toolId.value) store.loadProviders(toolId.value);
  });
});
</script>

<template>
  <div class="p-6" v-if="settings.loaded">
    <div class="mb-5 flex items-center justify-between">
      <h1 class="text-xl font-semibold">{{ t("nav.providers") }}</h1>
      <div class="flex items-center gap-2">
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          @click="importOpen = true"
        >
          <Upload class="h-3.5 w-3.5" /> {{ t("providers.import") }}
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          @click="exportProviders"
        >
          <Download class="h-3.5 w-3.5" /> {{ t("providers.export") }}
        </button>
        <button
          v-if="!isUniversalTab && toolDetails?.hasConfig"
          class="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          @click="importFromCli"
        >
          <Download class="h-3.5 w-3.5" /> {{ t("providers.importFromCli") }}
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-cyan-700"
          @click="openPresets"
        >
          <ChevronRight class="h-3.5 w-3.5" /> {{ t("providers.presets") }}
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-sm font-medium text-white hover:bg-slate-700 dark:bg-white dark:text-slate-800 dark:hover:bg-slate-200"
          @click="openAdd"
        >
          <Plus class="h-3.5 w-3.5" /> {{ t("common.add") }}
        </button>
      </div>
    </div>

    <div class="mb-5 flex flex-wrap items-center gap-2">
      <button
        v-for="tool in store.visibleTools"
        :key="tool.id"
        class="rounded-full px-4 py-1.5 text-sm font-medium transition-colors"
        :class="
          toolId === tool.id
            ? 'bg-cyan-600 text-white'
            : 'bg-slate-200 text-slate-600 hover:bg-slate-300 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700'
        "
        @click="toolId = tool.id"
      >
        {{ t(`tools.${tool.id}`) }}
      </button>
      <button
        class="rounded-full px-4 py-1.5 text-sm font-medium transition-colors"
        :class="
          isUniversalTab
            ? 'bg-cyan-600 text-white'
            : 'bg-slate-200 text-slate-600 hover:bg-slate-300 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700'
        "
        @click="toolId = 'universal'"
      >
        {{ t("providers.universalTab") }}
      </button>
    </div>

    <div
      v-if="isUniversalTab"
      class="mb-4 rounded-lg border border-blue-200 bg-blue-50/50 p-3 text-xs text-blue-700 dark:border-blue-900 dark:bg-blue-950/30 dark:text-blue-300"
    >
      {{ t("providers.universalDesc") }}
    </div>

    <div v-else-if="toolDetails" class="mb-4 flex items-center gap-3 rounded-lg border border-slate-200 p-3 text-xs text-slate-500 dark:border-slate-800 dark:text-slate-400">
      {{ t("providers.configPath") }}:
      <code class="text-cyan-700 dark:text-cyan-300">{{ toolDetails.configPath || "-" }}</code>
      <span
        v-if="toolDetails.hasConfig"
        class="inline-flex items-center gap-1 text-emerald-600 dark:text-emerald-400"
      >
        <Check class="h-3 w-3" /> {{ t("providers.configStatus") }}
      </span>
      <span v-else class="text-amber-600 dark:text-amber-400">{{ t("providers.configMissing") }}</span>
      <button
        v-if="store.activeProvider && !isUniversalTab"
        class="ml-auto inline-flex items-center gap-1 rounded-md bg-red-500/10 px-2.5 py-1 text-xs font-medium text-red-600 hover:bg-red-500/20 dark:text-red-400"
        @click="resetOfficial"
      >
        <RefreshCw class="h-3 w-3" /> {{ t("providers.resetOfficial") }}
      </button>
    </div>

    <p class="mb-3 text-xs text-slate-400">{{ t("providers.dragHint") }}</p>

    <div v-if="store.loading" class="text-sm text-slate-400">...</div>

    <div v-else-if="providers.length === 0" class="rounded-lg border border-dashed border-slate-300 p-10 text-center text-sm text-slate-400">
      {{ t("providers.noProviders") }}
    </div>

    <div v-else class="grid grid-cols-1 gap-3 lg:grid-cols-2">
      <div
        v-for="(p, index) in providers"
        :key="p.id"
        class="group relative cursor-grab rounded-xl border p-4 transition-colors active:cursor-grabbing"
        :class="
          p.isActive
            ? 'border-cyan-500 bg-cyan-50/60 dark:border-cyan-500/60 dark:bg-cyan-950/30'
            : 'border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900'
        "
        draggable="true"
        @dragstart="onDragStart(index)"
        @dragover="onDragOver"
        @drop="onDropAt(index)"
      >
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <GripVertical class="h-4 w-4 shrink-0 text-slate-300" />
              <span class="truncate font-medium">{{ p.name }}</span>
              <span v-if="p.isActive" class="inline-flex items-center gap-1 rounded-full bg-cyan-600 px-2 py-0.5 text-[10px] font-medium text-white">
                <Check class="h-3 w-3" /> {{ t("providers.active") }}
              </span>
              <span v-if="p.isPreset" class="rounded-full bg-slate-200 px-2 py-0.5 text-[10px] text-slate-500 dark:bg-slate-800 dark:text-slate-400">
                {{ t("providers.presets") }}
              </span>
              <span v-if="isUniversalTab" class="rounded-full bg-blue-100 px-2 py-0.5 text-[10px] text-blue-600 dark:bg-blue-950 dark:text-blue-300">
                {{ t("providers.universalTab") }}
              </span>
            </div>
            <div class="mt-1 text-xs text-slate-400">
              {{ p.baseUrl || "-" }}
              <span v-if="p.model"> · {{ p.model }}</span>
            </div>
          </div>
          <div class="relative">
            <button
              class="rounded p-1 text-slate-400 opacity-0 transition-opacity hover:bg-slate-100 group-hover:opacity-100 dark:hover:bg-slate-800"
              @click="menuFor = menuFor === p.id ? null : p.id"
            >
              <MoreHorizontal class="h-4 w-4" />
            </button>
            <div
              v-if="menuFor === p.id"
              class="absolute right-0 top-8 z-10 w-36 overflow-hidden rounded-lg border border-slate-200 bg-white py-1 text-sm shadow-lg dark:border-slate-700 dark:bg-slate-800"
            >
              <button class="flex w-full items-center gap-2 px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700" @click="openEdit(p); menuFor = null">
                <Pencil class="h-3.5 w-3.5" /> {{ t("common.edit") }}
              </button>
              <button class="flex w-full items-center gap-2 px-3 py-1.5 hover:bg-slate-100 dark:hover:bg-slate-700" @click="duplicate(p); menuFor = null">
                {{ t("common.copy") }}
              </button>
              <button class="flex w-full items-center gap-2 px-3 py-1.5 text-red-500 hover:bg-red-50 dark:hover:bg-red-950" @click="del(p); menuFor = null">
                {{ t("common.delete") }}
              </button>
            </div>
          </div>
        </div>
        <div class="mt-3 flex items-center justify-between">
          <button
            :disabled="p.isActive"
            class="rounded-lg px-3 py-1.5 text-sm font-medium disabled:cursor-default disabled:opacity-100"
            :class="
              p.isActive
                ? 'bg-cyan-600 text-white'
                : 'bg-slate-800 text-white hover:bg-slate-700 disabled:hover:bg-slate-800 dark:bg-white dark:text-slate-800'
            "
            @click="doSwitch(p)"
          >
            {{ p.isActive ? t("providers.active") : t("providers.switch") }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="modalOpen" class="fixed inset-0 z-40 flex items-center justify-center bg-black/40" @click.self="modalOpen = false">
      <div class="max-h-[85vh] w-[560px] overflow-y-auto rounded-2xl bg-white p-6 shadow-xl dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">{{ isEdit ? t("providers.editProvider") : t("providers.addProvider") }}</h2>
          <button class="text-slate-400 hover:text-slate-600" @click="modalOpen = false"><X class="h-5 w-5" /></button>
        </div>
        <div class="space-y-3">
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("providers.name") }}</label>
            <input v-model="form.name" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("providers.baseUrl") }}</label>
            <input v-model="form.baseUrl" placeholder="https://api.anthropic.com" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("providers.apiKey") }}</label>
            <input v-model="form.apiKey" type="password" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("providers.model") }}</label>
            <input v-model="form.model" placeholder="claude-sonnet-4-5" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("providers.headers") }}</label>
            <textarea v-model="form.headers" rows="2" placeholder='{"anthropic-beta": "..."}' class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"></textarea>
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("providers.protocol") }}</label>
            <select
              v-model="form.protocol"
              class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"
            >
              <option value="">{{ t("providers.protocolAuto") }}</option>
              <option value="anthropic">Anthropic</option>
              <option value="openai">OpenAI</option>
            </select>
            <p class="mt-1 text-xs text-slate-400">{{ t("providers.protocolHint") }}</p>
          </div>
          <div>
            <div class="mb-1 flex items-center justify-between">
              <label class="block text-sm font-medium">{{ t("providers.sharedConfig") }}</label>
              <button class="text-xs text-cyan-600 hover:underline dark:text-cyan-300" @click="extractShared">
                {{ t("providers.extractShared") }}
              </button>
            </div>
            <textarea v-model="form.sharedConfig" rows="3" class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"></textarea>
          </div>
          <label v-if="isUniversalTab" class="flex items-center gap-2 text-sm">
            <input type="checkbox" checked disabled class="h-4 w-4 text-cyan-600" />
            <span>{{ t("providers.universalOnly") }}</span>
          </label>
          <p v-if="formHeadersError" class="text-xs text-red-500">{{ formHeadersError }}</p>
        </div>
        <div class="mt-5 flex justify-end gap-2">
          <button class="rounded-lg border border-slate-300 px-4 py-2 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800" @click="modalOpen = false">{{ t("common.cancel") }}</button>
          <button class="rounded-lg bg-cyan-600 px-4 py-2 text-sm font-medium text-white hover:bg-cyan-700" @click="save">{{ t("common.save") }}</button>
        </div>
      </div>
    </div>

    <div v-if="presetsOpen" class="fixed inset-0 z-40 flex items-center justify-center bg-black/40" @click.self="presetsOpen = false">
      <div class="max-h-[70vh] w-[680px] overflow-y-auto rounded-2xl bg-white p-6 shadow-xl dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">{{ t("providers.presetsTitle") }} · {{ isUniversalTab ? t("providers.universalTab") : t(`tools.${toolId}`) }}</h2>
          <button class="text-slate-400 hover:text-slate-600" @click="presetsOpen = false"><X class="h-5 w-5" /></button>
        </div>
        <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
          <button
            v-for="p in store.presets"
            :key="`${p.name}-${p.baseUrl}`"
            class="rounded-xl border border-slate-200 p-3 text-left transition-colors hover:border-cyan-400 dark:border-slate-700"
            @click="addFromPreset(p)"
          >
            <div class="text-sm font-medium">{{ p.name }}</div>
            <div class="mt-0.5 truncate text-xs text-slate-400">{{ p.baseUrl }}</div>
            <div v-if="p.model" class="mt-0.5 text-xs text-slate-500">{{ p.model }}</div>
          </button>
        </div>
      </div>
    </div>

    <div v-if="importOpen" class="fixed inset-0 z-40 flex items-center justify-center bg-black/40" @click.self="importOpen = false">
      <div class="w-[560px] rounded-2xl bg-white p-6 shadow-xl dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">{{ t("providers.import") }}</h2>
          <button class="text-slate-400 hover:text-slate-600" @click="importOpen = false"><X class="h-5 w-5" /></button>
        </div>
        <p class="mb-2 text-xs text-slate-400">{{ t("providers.importTip") }}</p>
        <textarea v-model="importText" rows="8" class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"></textarea>
        <div class="mt-4 flex justify-end gap-2">
          <button class="rounded-lg border border-slate-300 px-4 py-2 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800" @click="importOpen = false">{{ t("common.cancel") }}</button>
          <button class="rounded-lg bg-cyan-600 px-4 py-2 text-sm font-medium text-white hover:bg-cyan-700" @click="importProviders">{{ t("providers.import") }}</button>
        </div>
      </div>
    </div>
  </div>
</template>