<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ArrowLeft, Eye, FilePlus2, FileText, Folder, RefreshCw, Save } from "lucide-vue-next";
import { api } from "@/utils/api";
import { useSettingsStore } from "@/stores/settings";
import type { WorkspaceFile } from "@/types";

const { t } = useI18n();
const settings = useSettingsStore();

const currentDir = ref("");
const files = ref<WorkspaceFile[]>([]);
const openPath = ref<string | null>(null);
const content = ref("");
const isPreview = ref(false);
const error = ref("");

const crumbs = computed(() =>
  currentDir.value ? currentDir.value.split("/").filter(Boolean) : [],
);

const currentFileHeading = computed(() => openPath.value || "-");

async function load(dir: string) {
  currentDir.value = dir || "";
  files.value = await api.workspaceFiles(currentDir.value);
}

async function openEntry(f: WorkspaceFile) {
  if (f.isDir) {
    const next = currentDir.value ? `${currentDir.value}/${f.name}` : f.name;
    await load(next);
  } else {
    openPath.value = f.path;
    content.value = await api.workspaceRead(f.path);
    isPreview.value = false;
  }
}

async function save() {
  if (!openPath.value) return;
  await api.workspaceSave(openPath.value, content.value);
  error.value = "";
}

async function createFile() {
  const name = window.prompt(t("workspace.fileName"));
  if (!name) return;
  if (name.includes("/") || name.includes("..")) {
    error.value = "invalid name";
    return;
  }
  const path = currentDir.value ? `${currentDir.value}/${name}` : name;
  await api.workspaceSave(path, "");
  openPath.value = path;
  content.value = "";
  isPreview.value = false;
  await load(currentDir.value);
}

function mdPreview(text: string): string {
  const esc = (s: string) =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const lines = text.split("\n");
  let html = "";
  let inCode = false;
  for (const line of lines) {
    if (line.trimStart().startsWith("```")) {
      html += inCode ? "</code></pre>" : "<pre><code>";
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      html += esc(line) + "\n";
      continue;
    }
    if (/^#\s/.test(line)) {
      html += `<h3 class="my-1 font-bold text-base">${esc(line.replace(/^#\s/, ""))}</h3>`;
    } else if (/^##\s/.test(line)) {
      html += `<h4 class="my-1 font-semibold">${esc(line.replace(/^##\s/, ""))}</h4>`;
    } else if (/^\s*[-*]\s/.test(line)) {
      html += `<div class="pl-3">• ${esc(line.replace(/^\s*[-*]\s/, ""))}</div>`;
    } else if (/^\s*\d+\.\s/.test(line)) {
      html += `<div class="pl-3">${esc(line)}</div>`;
    } else if (line.trim() === "") {
      html += "<div class='h-2'></div>";
    } else {
      html += `<div>${esc(line)}</div>`;
    }
  }
  if (inCode) html += "</code></pre>";
  return html;
}

onMounted(async () => {
  await settings.load();
  await load("");
});
</script>

<template>
  <div class="p-6" v-if="settings.loaded">
    <div class="mb-5 flex items-center justify-between">
      <h1 class="text-xl font-semibold">{{ t("workspace.title") }}</h1>
      <div class="flex items-center gap-2">
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          @click="load(currentDir)"
        >
          <RefreshCw class="h-3.5 w-3.5" /> {{ t("workspace.refresh") }}
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-cyan-700"
          @click="createFile"
        >
          <FilePlus2 class="h-3.5 w-3.5" /> {{ t("workspace.newFile") }}
        </button>
      </div>
    </div>

    <div class="mb-4 flex flex-wrap items-center gap-1 text-xs text-slate-400">
      <button
        class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 hover:bg-slate-100 dark:hover:bg-slate-800"
        @click="load('')"
      >
        <ArrowLeft class="h-3 w-3" /> ~/.openclaw
      </button>
      <template v-for="(c, i) in crumbs" :key="i">
        <span>/</span>
        <button
          class="rounded px-1.5 py-0.5 hover:bg-slate-100 dark:hover:bg-slate-800"
          @click="load(crumbs.slice(0, i + 1).join('/'))"
        >
          {{ c }}
        </button>
      </template>
    </div>

    <p v-if="error" class="mb-3 text-xs text-red-500">{{ error }}</p>

    <div class="grid grid-cols-1 gap-5 xl:grid-cols-3">
      <div class="rounded-xl border border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900">
        <div class="border-b border-slate-100 px-4 py-2 text-xs text-slate-400 dark:border-slate-800">{{ t("workspace.hint") }}</div>
        <div v-if="files.length === 0" class="p-6 text-center text-xs text-slate-400">{{ t("workspace.none") }}</div>
        <div v-else class="max-h-[60vh] overflow-y-auto p-2">
          <button
            v-for="f in files"
            :key="f.path"
            class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm hover:bg-slate-100 dark:hover:bg-slate-800"
            @click="openEntry(f)"
          >
            <Folder v-if="f.isDir" class="h-4 w-4 text-amber-500" />
            <FileText v-else class="ml-1 h-4 w-4 text-slate-400" />
            <span class="truncate">{{ f.name }}</span>
          </button>
        </div>
      </div>

      <div class="rounded-xl border border-slate-200 bg-white xl:col-span-2 dark:border-slate-800 dark:bg-slate-900">
        <div class="flex items-center justify-between border-b border-slate-100 px-4 py-2 dark:border-slate-800">
          <span class="truncate text-sm font-medium">{{ currentFileHeading }}</span>
          <div class="flex items-center gap-2">
            <button
              class="rounded border border-slate-300 px-2.5 py-1 text-xs hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
              @click="isPreview = !isPreview"
            >
              <span v-if="!isPreview"><Eye class="mr-1 inline h-3 w-3" />{{ t("workspace.preview") }}</span>
              <span v-else>{{ t("workspace.edit") }}</span>
            </button>
            <button
              class="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-cyan-700"
              @click="save"
            >
              <Save class="h-3 w-3" /> {{ t("workspace.save") }}
            </button>
          </div>
        </div>
        <div v-if="openPath" class="p-4">
          <textarea
            v-if="!isPreview"
            v-model="content"
            rows="24"
            class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"
          ></textarea>
          <div
            v-else
            class="min-h-[300px] rounded-lg border border-slate-200 p-4 text-sm leading-6 text-slate-700 dark:border-slate-700 dark:text-slate-300"
            v-html="mdPreview(content)"
          ></div>
        </div>
        <div v-else class="p-10 text-center text-xs text-slate-400">{{ t("workspace.openFile") }}</div>
      </div>
    </div>
  </div>
</template>