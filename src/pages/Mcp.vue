<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { Check, Pencil, Plus, RefreshCw, Trash2, X } from "lucide-vue-next";
import { useMcpStore } from "@/stores/mcp";
import { useProvidersStore } from "@/stores/providers";
import { useSettingsStore } from "@/stores/settings";
import type { McpServer, McpServerInput } from "@/types";

const { t } = useI18n();
const store = useMcpStore();
const providers = useProvidersStore();
const settings = useSettingsStore();

const modalOpen = ref(false);
const isEdit = ref(false);
const form = ref<McpServerInput>(emptyForm());
const syncing = ref(false);
const syncResult = ref("");

function emptyForm(): McpServerInput {
  return {
    name: "",
    command: "npx",
    args: [],
    env: {},
    cwd: "",
    transport: "stdio",
    isActive: true,
    tools: ["claude_code"],
  };
}

const argsText = ref("");
const envText = ref("");

function openAdd() {
  isEdit.value = false;
  form.value = emptyForm();
  argsText.value = "";
  envText.value = "";
  modalOpen.value = true;
}

function openEdit(s: McpServer) {
  isEdit.value = true;
  form.value = {
    id: s.id,
    name: s.name,
    command: s.command,
    args: [...s.args],
    env: { ...s.env },
    cwd: s.cwd,
    transport: s.transport,
    isActive: !!s.isActive,
    tools: [...s.tools],
  };
  argsText.value = s.args.join("\n");
  envText.value = Object.entries(s.env)
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
  modalOpen.value = true;
}

async function save() {
  form.value.args = argsText.value
    .split("\n")
    .map((x) => x.trim())
    .filter(Boolean);
  form.value.env = Object.fromEntries(
    envText.value
      .split("\n")
      .map((x) => x.trim())
      .filter(Boolean)
      .map((line) => {
        const i = line.indexOf("=");
        return i > 0 ? [line.slice(0, i).trim(), line.slice(i + 1)] : [line, ""];
      }),
  );
  await store.save(form.value);
  modalOpen.value = false;
}

async function remove(s: McpServer) {
  if (confirm(t("mcp.deleteConfirm", { name: s.name }))) await store.remove(s.id);
}

async function toggle(s: McpServer) {
  await store.save({ ...s, isActive: !s.isActive });
}

async function sync() {
  syncing.value = true;
  syncResult.value = "";
  try {
    const r = await store.sync();
    const count = Object.values(r).filter((v) => v === "ok").length;
    syncResult.value = t("mcp.synced", { count });
  } finally {
    syncing.value = false;
  }
}

onMounted(async () => {
  await store.load();
  await providers.loadTools();
});

function toggleTool(t: string) {
  const i = form.value.tools.indexOf(t);
  if (i >= 0) form.value.tools.splice(i, 1);
  else form.value.tools.push(t);
}
</script>

<template>
  <div class="p-6" v-if="settings.loaded">
    <div class="mb-5 flex items-center justify-between">
      <h1 class="text-xl font-semibold">{{ t("nav.mcp") }}</h1>
      <div class="flex items-center gap-2">
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 disabled:opacity-50 dark:border-slate-700 dark:hover:bg-slate-800"
          :disabled="syncing"
          @click="sync"
        >
          <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': syncing }" /> {{ t("mcp.sync") }}
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-cyan-700"
          @click="openAdd"
        >
          <Plus class="h-3.5 w-3.5" /> {{ t("mcp.add") }}
        </button>
      </div>
    </div>

    <p v-if="syncResult" class="mb-4 rounded-lg bg-emerald-50 px-3 py-2 text-sm text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300">
      {{ syncResult }}
    </p>

    <div v-if="store.list.length === 0" class="rounded-lg border border-dashed border-slate-300 p-10 text-center text-sm text-slate-400">
      {{ t("mcp.none") }}
    </div>

    <div v-else class="overflow-hidden rounded-xl border border-slate-200 dark:border-slate-800">
      <table class="w-full text-sm">
        <thead class="bg-slate-100 text-left text-xs text-slate-500 dark:bg-slate-800/60 dark:text-slate-400">
          <tr>
            <th class="px-4 py-2.5 font-medium">{{ t("mcp.name") }}</th>
            <th class="px-4 py-2.5 font-medium">{{ t("mcp.command") }}</th>
            <th class="px-4 py-2.5 font-medium">{{ t("mcp.applyTo") }}</th>
            <th class="px-4 py-2.5 text-right font-medium">{{ t("common.more") }}</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-slate-100 dark:divide-slate-800">
          <tr v-for="s in store.list" :key="s.id" class="bg-white dark:bg-slate-900" :class="{ 'opacity-50': !s.isActive }">
            <td class="px-4 py-3">
              <div class="flex items-center gap-2">
                <span class="font-medium">{{ s.name }}</span>
                <span
                  v-if="s.isActive"
                  class="inline-flex items-center gap-1 rounded-full bg-emerald-600 px-2 py-0.5 text-[10px] text-white"
                >
                  <Check class="h-3 w-3" /> {{ t("common.enable") }}
                </span>
              </div>
            </td>
            <td class="max-w-[260px] truncate px-4 py-3 font-mono text-xs text-slate-500">{{ s.command }} {{ s.args.join(" ") }}</td>
            <td class="px-4 py-3">
              <span v-for="tid in s.tools" :key="tid" class="mr-1.5 rounded bg-slate-200 px-1.5 py-0.5 text-[11px] text-slate-600 dark:bg-slate-800 dark:text-slate-300">
                {{ t(`tools.${tid}`) }}
              </span>
            </td>
            <td class="px-4 py-3 text-right">
              <div class="flex items-center justify-end gap-1">
                <button class="rounded p-1.5 text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800" @click="toggle(s)">
                  {{ s.isActive ? t("common.disable") : t("common.enable") }}
                </button>
                <button class="rounded p-1.5 text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800" @click="openEdit(s)">
                  <Pencil class="h-4 w-4" />
                </button>
                <button class="rounded p-1.5 text-red-400 hover:bg-red-50 dark:hover:bg-red-950" @click="remove(s)">
                  <Trash2 class="h-4 w-4" />
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="modalOpen" class="fixed inset-0 z-40 flex items-center justify-center bg-black/40" @click.self="modalOpen = false">
      <div class="max-h-[85vh] w-[620px] overflow-y-auto rounded-2xl bg-white p-6 shadow-xl dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">{{ isEdit ? t("mcp.edit") : t("mcp.add") }}</h2>
          <button class="text-slate-400 hover:text-slate-600" @click="modalOpen = false"><X class="h-5 w-5" /></button>
        </div>
        <div class="space-y-3">
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("mcp.name") }}</label>
            <input v-model="form.name" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="mb-1 block text-sm font-medium">{{ t("mcp.command") }}</label>
              <input v-model="form.command" placeholder="e.g. npx / uvx / deno" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
            </div>
            <div>
              <label class="mb-1 block text-sm font-medium">{{ t("mcp.transport") }}</label>
              <select v-model="form.transport" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800">
                <option value="stdio">stdio</option>
                <option value="sse">sse</option>
                <option value="http">http</option>
              </select>
            </div>
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("mcp.args") }}</label>
            <textarea v-model="argsText" rows="2" :placeholder="t('mcp.argsHint')" class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"></textarea>
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("mcp.env") }}</label>
            <textarea v-model="envText" rows="3" class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"></textarea>
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("mcp.cwd") }}</label>
            <input v-model="form.cwd" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
          </div>
          <div>
            <label class="mb-2 block text-sm font-medium">{{ t("mcp.applyTo") }}</label>
            <div class="flex flex-wrap gap-2">
              <button
                v-for="tool in providers.tools"
                :key="tool.id"
                class="rounded-full px-3 py-1 text-xs font-medium transition-colors"
                :class="form.tools.includes(tool.id) ? 'bg-cyan-600 text-white' : 'bg-slate-200 text-slate-500 dark:bg-slate-800 dark:text-slate-400'"
                @click="toggleTool(tool.id)"
              >
                {{ t(`tools.${tool.id}`) }}
              </button>
            </div>
          </div>
        </div>
        <div class="mt-5 flex justify-end gap-2">
          <button class="rounded-lg border border-slate-300 px-4 py-2 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800" @click="modalOpen = false">{{ t("common.cancel") }}</button>
          <button class="rounded-lg bg-cyan-600 px-4 py-2 text-sm font-medium text-white hover:bg-cyan-700" @click="save">{{ t("common.save") }}</button>
        </div>
      </div>
    </div>
  </div>
</template>