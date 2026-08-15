<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { Check, Pencil, Plus, Trash2, X } from "lucide-vue-next";
import { usePromptsStore } from "@/stores/prompts";
import { useSettingsStore } from "@/stores/settings";

const { t } = useI18n();
const store = usePromptsStore();
const settings = useSettingsStore();

const modalOpen = ref(false);
const isEdit = ref(false);
const form = ref({ id: undefined as number | undefined, name: "", content: "", fileType: "CLAUDE_MD" });

const fileTypes = ["CLAUDE_MD", "AGENTS_MD", "GEMINI_MD", "OPENCODE_MD"];

function openAdd() {
  isEdit.value = false;
  form.value = { id: undefined, name: "", content: "", fileType: "CLAUDE_MD" };
  modalOpen.value = true;
}

function openEdit(p: any) {
  isEdit.value = true;
  form.value = { id: p.id, name: p.name, content: p.content, fileType: p.fileType };
  modalOpen.value = true;
}

async function save() {
  if (!form.value.name.trim()) return;
  await store.save(form.value);
  modalOpen.value = false;
}

async function remove(p: any) {
  if (confirm(t("prompts.deleteConfirm", { name: p.name }))) await store.remove(p.id);
}

onMounted(() => store.load());
</script>

<template>
  <div class="p-6" v-if="settings.loaded">
    <div class="mb-5 flex items-center justify-between">
      <h1 class="text-xl font-semibold">{{ t("nav.prompts") }}</h1>
      <button class="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-cyan-700" @click="openAdd">
        <Plus class="h-3.5 w-3.5" /> {{ t("common.add") }}
      </button>
    </div>

    <div v-if="store.list.length === 0" class="rounded-lg border border-dashed border-slate-300 p-10 text-center text-sm text-slate-400">
      {{ t("prompts.none") }}
    </div>

    <div class="grid grid-cols-1 gap-3">
      <div v-for="p in store.list" :key="p.id" class="group rounded-xl border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
        <div class="flex items-center justify-between gap-2">
          <div class="flex items-center gap-2">
            <span class="font-medium">{{ p.name }}</span>
            <span class="rounded bg-slate-200 px-2 py-0.5 text-[11px] text-slate-500 dark:bg-slate-800 dark:text-slate-400">
              {{ t(`prompts.fileTypes.${p.fileType}`) }}
            </span>
            <span
              v-if="p.isActive"
              class="inline-flex items-center gap-1 rounded-full bg-emerald-600 px-2 py-0.5 text-[10px] text-white"
            >
              <Check class="h-3 w-3" /> {{ t("prompts.activated") }}
            </span>
          </div>
          <div class="flex items-center gap-1">
            <button
              v-if="p.isActive"
              class="rounded bg-slate-800 px-2.5 py-1 text-xs font-medium text-white hover:bg-slate-700 dark:bg-white dark:text-slate-800"
              @click="store.deactivate(p.id)"
            >
              {{ t("prompts.deactivate") }}
            </button>
            <button v-else class="rounded bg-slate-800 px-2.5 py-1 text-xs font-medium text-white hover:bg-slate-700 dark:bg-white dark:text-slate-800" @click="store.activate(p.id)">
              {{ t("prompts.activate") }}
            </button>
            <button class="rounded p-1.5 text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800" @click="openEdit(p)">
              <Pencil class="h-4 w-4" />
            </button>
            <button class="rounded p-1.5 text-red-400 hover:bg-red-50 dark:hover:bg-red-950" @click="remove(p)">
              <Trash2 class="h-4 w-4" />
            </button>
          </div>
        </div>
        <p class="mt-2 line-clamp-3 whitespace-pre-wrap text-xs text-slate-500 dark:text-slate-400">{{ p.content }}</p>
      </div>
    </div>

    <div v-if="modalOpen" class="fixed inset-0 z-40 flex items-center justify-center bg-black/40" @click.self="modalOpen = false">
      <div class="max-h-[85vh] w-[680px] overflow-y-auto rounded-2xl bg-white p-6 shadow-xl dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">{{ isEdit ? t("prompts.edit") : t("prompts.add") }}</h2>
          <button class="text-slate-400 hover:text-slate-600" @click="modalOpen = false"><X class="h-5 w-5" /></button>
        </div>
        <div class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="mb-1 block text-sm font-medium">{{ t("prompts.name") }}</label>
              <input v-model="form.name" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
            </div>
            <div>
              <label class="mb-1 block text-sm font-medium">{{ t("prompts.fileType") }}</label>
              <select v-model="form.fileType" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800">
                <option v-for="ft in fileTypes" :key="ft" :value="ft">{{ t(`prompts.fileTypes.${ft}`) }}</option>
              </select>
            </div>
          </div>
          <div>
            <label class="mb-1 block text-sm font-medium">{{ t("prompts.content") }}</label>
            <textarea v-model="form.content" rows="10" class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"></textarea>
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