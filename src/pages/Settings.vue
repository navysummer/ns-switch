<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { CloudUpload, CloudDownload, DatabaseBackup, Play, RefreshCw, RotateCcw, Square, Wrench } from "lucide-vue-next";
import { isEnabled, enable, disable } from "@tauri-apps/plugin-autostart";
import { api } from "@/utils/api";
import { useSettingsStore } from "@/stores/settings";
import { useProvidersStore } from "@/stores/providers";
import type { BackupInfo, CloudSettings, ProxyHealthItem, ProxyStatus, UpdateResult } from "@/types";

const { t } = useI18n();
const settings = useSettingsStore();
const providers = useProvidersStore();

const autostart = ref(false);
const backups = ref<BackupInfo[]>([]);
const proxies = ref<ProxyStatus[]>([]);
const healthByTool = ref<Record<string, ProxyHealthItem[]>>({});

const deeplinkText = ref("");
const deeplinkResult = ref("");

const cloud = ref<CloudSettings>({ url: "", username: "", password: "", enabled: false });
const cloudResult = ref("");
const newConfigDir = ref("");

const updateUrl = ref("");
const updateResult = ref<UpdateResult | null>(null);

const languages = [
  ["zh", "简体中文"],
  ["zh-TW", "繁體中文"],
  ["en", "English"],
  ["ja", "日本語"],
] as const;

async function refreshBackups() {
  try {
    backups.value = await api.listBackups();
  } catch {
    backups.value = [];
  }
}

async function refreshProxies() {
  try {
    proxies.value = await api.proxyStatuses();
  } catch {
    proxies.value = [];
  }
}

async function loadCloud() {
  try {
    const info = await api.cloudInfo();
    updateUrl.value = updateUrl.value || "";
    cloud.value = { ...cloud.value, enabled: info.enabled };
    updateUrlFromSettings();
    newConfigDir.value = info.dataDir;
  } catch {
    /* ignore */
  }
}

async function updateUrlFromSettings() {
  try {
    const s = await api.getSettings();
    const url = (s as unknown as Record<string, unknown>).updateUrl as string | undefined;
    updateUrl.value = url || "";
  } catch {
    /* ignore */
  }
}

async function setUpdateUrl() {
  await settings.update({ updateUrl: updateUrl.value } as never);
}

async function backupNow() {
  await api.backupNow();
  await refreshBackups();
}

async function restore(path: string) {
  if (confirm(`Restore backup -> ${path}?`)) {
    await api.restoreBackup(path);
    window.location.reload();
  }
}

function prettySize(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function fmtTime(ts: string) {
  if (!ts) return "-";
  return new Date(ts).toLocaleString();
}

async function setAutostart(v: boolean) {
  autostart.value = v;
  try {
    if (v) await enable();
    else await disable();
  } catch {
    autostart.value = !v;
  }
}

async function importDeepLinks() {
  const r = await api.importDeepLinks(deeplinkText.value);
  deeplinkResult.value = t("settings.linked", { count: r.count }) + (r.message ? `\n${r.message}` : "");
}

async function registerScheme() {
  deeplinkResult.value = await api.registerScheme();
}

async function cloudTest() {
  cloudResult.value = await api.cloudTest();
}

async function cloudPush() {
  await api.cloudSet(cloud.value);
  cloudResult.value = await api.cloudPush();
}

async function cloudPull() {
  await api.cloudSet(cloud.value);
  cloudResult.value = await api.cloudPull();
}

async function moveConfigDir() {
  if (!newConfigDir.value.trim()) return;
  const r = await api.cloudSetConfigDir(newConfigDir.value.trim());
  alert(r);
  await settings.load();
}

async function checkUpdates() {
  await setUpdateUrl();
  updateResult.value = await api.checkUpdates();
}

async function checkHealth(toolId: string) {
  try {
    const items = await api.proxyHealth(toolId);
    healthByTool.value[toolId] = items;
    healthByTool.value = { ...healthByTool.value };
  } catch {
    /* ignore */
  }
}

const portInputs = ref<Record<string, string>>({});

function effectivePort(toolId: string, def: number) {
  return settings.settings?.proxyPorts?.[toolId] ?? def;
}

async function setPort(toolId: string) {
  const raw = (portInputs.value[toolId] ?? "").trim();
  const n = parseInt(raw, 10);
  if (Number.isNaN(n) || n < 1 || n > 65535) return;
  const ports = { ...(settings.settings?.proxyPorts ?? {}) };
  ports[toolId] = n;
  await settings.update({ proxyPorts: ports });
  // restart the running proxy on the new port
  const running = proxies.value.find((p) => p.toolId === toolId)?.running;
  if (running) {
    try {
      await api.proxyStop(toolId);
    } catch {
      /* ignore */
    }
    await api.proxyStart(toolId).catch(() => {});
  }
  await refreshProxies();
}

function enabledFor(toolId: string) {
  return settings.settings?.proxyEnabled?.[toolId] ?? true;
}

async function toggleTakeover(toolId: string) {
  const next = !enabledFor(toolId);
  const enabled = { ...(settings.settings?.proxyEnabled ?? {}) };
  enabled[toolId] = next;
  await settings.update({ proxyEnabled: enabled });
  if (!next) {
    await api.proxyStop(toolId).catch(() => {});
  } else {
    await api.proxyStart(toolId).catch(() => {});
  }
  await refreshProxies();
}

onMounted(async () => {
  isEnabled().then((v) => (autostart.value = v));
  await settings.load();
  loadCloud();
  await refreshBackups();
  await refreshProxies();
});
</script>

<template>
  <div class="p-6" v-if="settings.loaded && settings.settings">
    <h1 class="mb-5 text-xl font-semibold">{{ t("nav.settings") }}</h1>

    <div class="grid grid-cols-1 gap-5 xl:grid-cols-2">
      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <h2 class="mb-4 text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("settings.general") }}</h2>
        <div class="space-y-4">
          <div>
            <label class="mb-1.5 block text-sm font-medium">{{ t("settings.theme") }}</label>
            <div class="flex gap-2">
              <button
                v-for="th in (['light', 'dark', 'system'] as const)"
                :key="th"
                class="rounded-lg px-3 py-1.5 text-sm font-medium"
                :class="settings.theme === th ? 'bg-cyan-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300'"
                @click="settings.update({ theme: th })"
              >
                {{ t(`settings.${th}`) }}
              </button>
            </div>
          </div>
          <div>
            <label class="mb-1.5 block text-sm font-medium">{{ t("settings.language") }}</label>
            <div class="flex flex-wrap gap-2">
              <button
                v-for="lg in languages"
                :key="lg[0]"
                class="rounded-lg px-3 py-1.5 text-sm font-medium"
                :class="settings.language === lg[0] ? 'bg-cyan-600 text-white' : 'bg-slate-100 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300'"
                @click="settings.update({ language: lg[0] })"
              >
                {{ lg[1] }}
              </button>
            </div>
          </div>
          <div class="flex items-center justify-between">
            <div class="text-sm font-medium">{{ t("settings.autostart") }}</div>
            <button
              class="h-6 w-11 rounded-full transition-colors"
              :class="autostart ? 'bg-cyan-600' : 'bg-slate-300 dark:bg-slate-700'"
              @click="setAutostart(!autostart)"
            >
              <span class="block h-5 w-5 rounded-full bg-white shadow transition-transform" :class="autostart ? 'translate-x-5' : 'translate-x-0.5'" />
            </button>
          </div>
          <div class="flex items-center justify-between">
            <div class="text-sm font-medium">{{ t("settings.notifications") }}</div>
            <button
              class="h-6 w-11 rounded-full transition-colors"
              :class="settings.settings.notifications ? 'bg-cyan-600' : 'bg-slate-300 dark:bg-slate-700'"
              @click="settings.update({ notifications: !settings.settings.notifications })"
            >
              <span class="block h-5 w-5 rounded-full bg-white shadow transition-transform" :class="settings.settings.notifications ? 'translate-x-5' : 'translate-x-0.5'" />
            </button>
          </div>
          <div>
            <div class="text-sm font-medium">{{ t("settings.dataDir") }}</div>
            <div class="mt-1 text-xs text-slate-400">{{ settings.settings.dataDir }}</div>
            <div class="mt-0.5 text-xs text-slate-400">{{ t("settings.dataDirDesc") }}</div>
            <div class="mt-2 flex gap-2">
              <input v-model="newConfigDir" class="flex-1 rounded-lg border border-slate-300 px-3 py-1.5 text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
              <button class="rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-medium text-white hover:bg-slate-700 dark:bg-white dark:text-slate-800" @click="moveConfigDir">
                {{ t("settings.changeDir") }}
              </button>
            </div>
          </div>
          <div>
            <div class="text-sm font-medium">{{ t("settings.updates") }}</div>
            <input v-model="updateUrl" :placeholder="t('settings.updateUrlHint')" class="mt-1 w-full rounded-lg border border-slate-300 px-3 py-1.5 text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
            <div class="mt-2 flex items-center gap-3">
              <button class="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-cyan-700" @click="checkUpdates">
                <RefreshCw class="h-3.5 w-3.5" /> {{ t("settings.checkUpdates") }}
              </button>
              <template v-if="updateResult">
                <span v-if="!updateResult.hasUpdate" class="text-xs text-emerald-600 dark:text-emerald-400">{{ t("settings.upToDate") }} ({{ updateResult.current }})</span>
                <a v-else class="text-xs text-cyan-600 underline dark:text-cyan-300" :href="updateResult.url || '#'" target="_blank">
                  {{ t("settings.updateAvailableTitle") }}: {{ updateResult.latest }}
                </a>
              </template>
            </div>
          </div>
        </div>
      </section>

      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <h2 class="mb-4 text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("settings.tools") }}</h2>
        <div class="mb-1 text-xs text-slate-400">{{ t("settings.toolsDesc") }}</div>
        <div class="space-y-1">
          <div v-for="tool in providers.tools" :key="tool.id" class="flex items-center justify-between rounded-lg px-2 py-1.5 hover:bg-slate-50 dark:hover:bg-slate-800">
            <span class="text-sm">{{ t(`tools.${tool.id}`) }}</span>
            <button
              class="h-6 w-11 rounded-full transition-colors"
              :class="tool.visible ? 'bg-cyan-600' : 'bg-slate-300 dark:bg-slate-700'"
              @click="providers.setToolVisible(tool.id, !tool.visible)"
            >
              <span class="block h-5 w-5 rounded-full bg-white shadow transition-transform" :class="tool.visible ? 'translate-x-5' : 'translate-x-0.5'" />
            </button>
          </div>
        </div>
      </section>

      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("settings.proxy") }}</h2>
          <button class="text-xs text-slate-400 hover:text-slate-600" @click="refreshProxies">{{ t("common.refresh") }}</button>
        </div>
        <p class="mb-4 text-xs text-slate-400">{{ t("settings.proxyDesc") }}</p>
        <div class="space-y-2">
          <div v-for="p in proxies" :key="p.toolId" class="rounded-lg border border-slate-200 px-3 py-2 dark:border-slate-800">
            <div class="flex items-center justify-between">
              <div class="min-w-0">
                <div class="flex items-center gap-2 text-sm font-medium">
                  {{ t(`tools.${p.toolId}`) }}
                  <span
                    class="rounded-full px-2 py-0.5 text-[10px]"
                    :class="p.running ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300' : 'bg-slate-200 text-slate-500 dark:bg-slate-800 dark:text-slate-400'"
                  >
                    {{ p.running ? t("settings.proxyRunning") : t("settings.proxyStopped") }}
                  </span>
                </div>
                <div class="mt-1 flex items-center gap-2 text-xs text-slate-400">
                  <label class="flex items-center gap-1.5">
                    <input
                      type="checkbox"
                      class="h-3.5 w-3.5 text-cyan-600"
                      :checked="enabledFor(p.toolId)"
                      @change="toggleTakeover(p.toolId)"
                    />
                    <span>{{ t("settings.takeover") }}</span>
                  </label>
                  <span class="inline-flex items-center gap-1">
                    <span class="text-slate-300">http://127.0.0.1:</span>
                    <input
                      class="w-20 rounded border border-slate-300 px-1.5 py-0.5 text-center outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"
                      :value="portInputs[p.toolId] ?? String(effectivePort(p.toolId, p.port))"
                      @input="portInputs[p.toolId] = ($event.target as HTMLInputElement).value"
                      @change="setPort(p.toolId)"
                    />
                  </span>
                  <span v-if="p.activeProvider" class="truncate">→ {{ p.activeProvider }}</span>
                </div>
              </div>
              <div class="flex items-center gap-1.5">
                <button class="rounded-lg border border-slate-300 px-2.5 py-1 text-xs hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800" @click="checkHealth(p.toolId)">
                  <Wrench class="h-3 w-3" />
                </button>
                <button
                  class="rounded-lg px-3 py-1.5 text-xs font-medium text-white"
                  :class="p.running ? 'bg-red-500 hover:bg-red-600' : 'bg-emerald-600 hover:bg-emerald-700'"
                  :disabled="!!p.error"
                  @click="p.running ? api.proxyStop(p.toolId).then(refreshProxies) : api.proxyStart(p.toolId).then(refreshProxies)"
                >
                  <component :is="p.running ? Square : Play" class="mr-1 inline h-3 w-3" />
                  {{ p.running ? t("settings.proxyStop") : t("settings.proxyStart") }}
                </button>
              </div>
            </div>
            <div v-if="healthByTool[p.toolId]" class="mt-2 space-y-1 border-t border-slate-100 pt-2 dark:border-slate-800">
              <div v-for="h in healthByTool[p.toolId]" :key="h.providerId" class="flex items-center justify-between text-xs">
                <span class="flex items-center gap-1.5">
                  <span class="h-2 w-2 rounded-full" :class="h.ok ? 'bg-emerald-500' : 'bg-red-500'"></span>
                  {{ h.name }}
                </span>
                <span class="text-slate-400">{{ h.ok ? `${h.latencyMs}ms` : "-" }}</span>
              </div>
            </div>
          </div>
        </div>
        <p class="mt-3 text-xs text-slate-400">{{ t("settings.proxyRouteHint", { port: "PORT" }) }}</p>
      </section>

      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("settings.backup") }}</h2>
          <button class="inline-flex items-center gap-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-medium text-white hover:bg-slate-700 dark:bg-white dark:text-slate-800" @click="backupNow">
            <DatabaseBackup class="h-3.5 w-3.5" /> {{ t("settings.backupNow") }}
          </button>
        </div>
        <div class="space-y-1">
          <div v-if="backups.length === 0" class="text-xs text-slate-400">{{ t("settings.noBackups") }}</div>
          <div v-for="b in backups.slice().reverse()" :key="b.path" class="flex items-center justify-between text-xs">
            <button class="truncate text-slate-500 hover:text-cyan-600" :title="t('settings.restore')" @click="restore(b.path)">{{ b.name }}</button>
            <span class="shrink-0 text-slate-400">{{ fmtTime(b.created) }} · {{ prettySize(b.size) }}</span>
          </div>
        </div>
      </section>

      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <h2 class="mb-2 text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("settings.deeplink") }}</h2>
        <p class="mb-2 text-xs text-slate-400">{{ t("settings.deeplinkDesc") }}</p>
        <textarea v-model="deeplinkText" rows="4" class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" placeholder="ns-switch://provider?tool=claude_code&name=X&baseUrl=..."></textarea>
        <div class="mt-2 flex items-center gap-3">
          <button class="rounded-lg bg-cyan-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-cyan-700" @click="importDeepLinks">
            {{ t("settings.importLinks") }}
          </button>
          <button class="rounded-lg border border-slate-300 px-3 py-1.5 text-xs hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800" @click="registerScheme">
            {{ t("settings.registerScheme") }}
          </button>
          <pre v-if="deeplinkResult" class="whitespace-pre-wrap text-xs text-emerald-600 dark:text-emerald-400">{{ deeplinkResult }}</pre>
        </div>
      </section>

      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <h2 class="mb-2 text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("settings.cloud") }}</h2>
        <p class="mb-3 text-xs text-slate-400">{{ t("settings.cloudDesc") }}</p>
        <div class="space-y-2">
          <input v-model="cloud.url" :placeholder="t('settings.webdavUrl')" class="w-full rounded-lg border border-slate-300 px-3 py-1.5 text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
          <div class="grid grid-cols-2 gap-2">
            <input v-model="cloud.username" :placeholder="t('settings.httpUser')" class="w-full rounded-lg border border-slate-300 px-3 py-1.5 text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
            <input v-model="cloud.password" type="password" :placeholder="t('settings.httpPass')" class="w-full rounded-lg border border-slate-300 px-3 py-1.5 text-xs outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800" />
          </div>
          <div class="flex items-center gap-2">
            <button class="rounded-lg border border-slate-300 px-3 py-1.5 text-xs hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800" @click="cloudTest">
              {{ t("settings.testConn") }}
            </button>
            <button class="inline-flex items-center gap-1.5 rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700" @click="cloudPush">
              <CloudUpload class="h-3.5 w-3.5" /> {{ t("settings.cloudPushUp") }}
            </button>
            <button class="inline-flex items-center gap-1.5 rounded-lg bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-700" @click="cloudPull">
              <CloudDownload class="h-3.5 w-3.5" /> {{ t("settings.cloudPullDown") }}
            </button>
          </div>
          <pre v-if="cloudResult" class="whitespace-pre-wrap text-xs text-emerald-600 dark:text-emerald-400">{{ cloudResult }}</pre>
        </div>
      </section>
    </div>
  </div>
</template>