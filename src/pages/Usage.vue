<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw, ScanSearch, Save, Trash2 } from "lucide-vue-next";
import { api } from "@/utils/api";
import { useSettingsStore } from "@/stores/settings";
import type { UsageLogItem, UsageModelRow, UsageOverview, UsagePoint } from "@/types";

const { t } = useI18n();
const settings = useSettingsStore();

const overview = ref<UsageOverview>({
  totalCost: 0,
  todayCost: 0,
  todayRequests: 0,
  totalRequests: 0,
  models: 0,
});
const byDay = ref<UsagePoint[]>([]);
const byModel = ref<UsageModelRow[]>([]);
const logs = ref<UsageLogItem[]>([]);
const pricing = ref("");
const scanMsg = ref("");
const events = ref<Array<Record<string, unknown>>>([]);

async function load() {
  overview.value = await api.usageOverview();
  byDay.value = await api.usageByDay(30);
  byModel.value = await api.usageByModel(10);
  logs.value = await api.usageLogs(20);
  events.value = await api.usageEvents(30);
  pricing.value = await api.usagePricingGet();
}

async function savePricing() {
  await api.usagePricingSet(pricing.value);
  await load();
}

async function scan() {
  const n = await api.usageScanSessions();
  scanMsg.value = `${t("usage.scanSessions")}: ${n}`;
  await load();
}

async function resetUsage() {
  if (confirm(t("usage.resetConfirm"))) {
    await api.usageReset();
    await load();
  }
}

const maxDayCost = computed(() => {
  let m = 0;
  for (const d of byDay.value) if (d.cost > m) m = d.cost;
  return m || 1;
});

function fmtMoney(v: number) {
  return `$${v.toFixed(4)}`;
}

const bars = computed(() =>
  byDay.value.map((d) => ({
    day: d.day.slice(5),
    height: Math.max(2, Math.round((d.cost / maxDayCost.value) * 100)),
    cost: d.cost,
    requests: d.requests,
  })),
);

onMounted(async () => {
  await settings.load();
  await load();
});
</script>

<template>
  <div class="p-6" v-if="settings.loaded">
    <div class="mb-5 flex items-center justify-between">
      <h1 class="text-xl font-semibold">{{ t("usage.title") }}</h1>
      <div class="flex items-center gap-2">
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          @click="scan"
        >
          <ScanSearch class="h-3.5 w-3.5" /> {{ t("usage.scanSessions") }}
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 px-3 py-1.5 text-sm hover:bg-slate-100 dark:border-slate-700 dark:hover:bg-slate-800"
          @click="load"
        >
          <RefreshCw class="h-3.5 w-3.5" />
        </button>
        <button
          class="inline-flex items-center gap-1.5 rounded-lg border border-red-300 px-3 py-1.5 text-sm text-red-500 hover:bg-red-50 dark:border-red-900 dark:hover:bg-red-950"
          @click="resetUsage"
        >
          <Trash2 class="h-3.5 w-3.5" /> {{ t("usage.reset") }}
        </button>
      </div>
    </div>

    <div class="grid grid-cols-2 gap-3 lg:grid-cols-5">
      <div class="rounded-xl border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
        <div class="text-xs text-slate-400">{{ t("usage.totalCost") }}</div>
        <div class="mt-1 text-xl font-semibold text-cyan-600 dark:text-cyan-300">{{ fmtMoney(overview.totalCost) }}</div>
      </div>
      <div class="rounded-xl border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
        <div class="text-xs text-slate-400">{{ t("usage.todayCost") }}</div>
        <div class="mt-1 text-xl font-semibold">{{ fmtMoney(overview.todayCost) }}</div>
      </div>
      <div class="rounded-xl border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
        <div class="text-xs text-slate-400">{{ t("usage.totalRequests") }}</div>
        <div class="mt-1 text-xl font-semibold">{{ overview.totalRequests }}</div>
      </div>
      <div class="rounded-xl border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
        <div class="text-xs text-slate-400">{{ t("usage.todayRequests") }}</div>
        <div class="mt-1 text-xl font-semibold">{{ overview.todayRequests }}</div>
      </div>
      <div class="rounded-xl border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-900">
        <div class="text-xs text-slate-400">{{ t("usage.models") }}</div>
        <div class="mt-1 text-xl font-semibold">{{ overview.models }}</div>
      </div>
    </div>

    <div class="mt-5 grid grid-cols-1 gap-5 xl:grid-cols-3">
      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <h2 class="mb-4 text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("settings.backupNow") }} · 30d</h2>
        <div v-if="bars.length === 0" class="py-10 text-center text-xs text-slate-400">{{ t("usage.none") }}</div>
        <div v-else class="flex h-40 items-end gap-1.5">
          <div v-for="b in bars" :key="b.day" class="group relative flex-1" :title="`${b.day}: ${b.cost.toFixed(4)} / ${b.requests} req`">
            <div class="rounded-t bg-cyan-500/80 group-hover:bg-cyan-600" :style="{ height: b.height + '%' }"></div>
            <div class="mt-1 hidden text-center text-[9px] text-slate-400 md:block">{{ b.day }}</div>
          </div>
        </div>
      </section>

      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <h2 class="mb-3 text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("usage.byModel") }}</h2>
        <div v-if="byModel.length === 0" class="py-6 text-center text-xs text-slate-400">{{ t("usage.none") }}</div>
        <div v-else class="space-y-2">
          <div v-for="m in byModel" :key="m.model" class="flex items-center justify-between text-sm">
            <span class="truncate font-medium">{{ m.model }}</span>
            <span class="text-xs text-slate-400">{{ m.requests }} req · {{ fmtMoney(m.cost) }}</span>
          </div>
        </div>
      </section>

      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("usage.pricing") }}</h2>
          <button class="inline-flex items-center gap-1.5 rounded-lg bg-cyan-600 px-2.5 py-1 text-xs font-medium text-white hover:bg-cyan-700" @click="savePricing">
            <Save class="h-3 w-3" /> {{ t("common.save") }}
          </button>
        </div>
        <textarea v-model="pricing" rows="10" class="w-full rounded-lg border border-slate-300 px-3 py-2 font-mono text-[11px] outline-none focus:border-cyan-500 dark:border-slate-700 dark:bg-slate-800"></textarea>
      </section>
    </div>

    <section class="mt-5 rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
      <h2 class="mb-3 text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("usage.requestLog") }}</h2>
      <div v-if="events.length === 0" class="py-6 text-center text-xs text-slate-400">{{ t("usage.none") }}</div>
      <table v-else class="w-full text-sm">
        <thead class="text-left text-xs text-slate-500 dark:text-slate-400">
          <tr>
            <th class="py-1.5 font-medium">{{ t("usage.time") }}</th>
            <th class="py-1.5 font-medium">{{ t("usage.provider") }}</th>
            <th class="py-1.5 font-medium">{{ t("usage.model") }}</th>
            <th class="py-1.5 font-medium">{{ t("usage.tokens") }}</th>
            <th class="py-1.5 font-medium">{{ t("usage.latency") }}</th>
            <th class="py-1.5 text-right font-medium">{{ t("usage.cost") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(e, i) in events" :key="i" class="border-t border-slate-100 dark:border-slate-800">
            <td class="py-1.5 text-xs text-slate-400">{{ e.ts }}</td>
            <td class="py-1.5 text-xs">{{ e.providerName }}</td>
            <td class="py-1.5 font-medium">{{ e.model }}</td>
            <td class="py-1.5 text-xs text-slate-400">{{ Number(e.inputTokens || 0) + Number(e.outputTokens || 0) }}</td>
            <td class="py-1.5 text-xs text-slate-400">{{ e.latencyMs }}ms</td>
            <td class="py-1.5 text-right">{{ fmtMoney(Number(e.costUsd) || 0) }}</td>
          </tr>
        </tbody>
      </table>
    </section>

    <section class="mt-5 rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
      <h2 class="mb-3 text-sm font-semibold text-slate-500 dark:text-slate-400">{{ t("usage.byDayTitle") }}</h2>
      <div v-if="logs.length === 0" class="py-6 text-center text-xs text-slate-400">{{ t("usage.noneAgg") }}</div>
      <table v-else class="w-full text-sm">
        <thead class="text-left text-xs text-slate-500 dark:text-slate-400">
          <tr>
            <th class="py-1.5 font-medium">{{ t("usage.day") }}</th>
            <th class="py-1.5 font-medium">{{ t("usage.model") }}</th>
            <th class="py-1.5 font-medium">{{ t("usage.provider") }}</th>
            <th class="py-1.5 font-medium">{{ t("usage.requests") }}</th>
            <th class="py-1.5 font-medium">{{ t("usage.tokens") }}</th>
            <th class="py-1.5 text-right font-medium">{{ t("usage.cost") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="l in logs" :key="l.id" class="border-t border-slate-100 dark:border-slate-800">
            <td class="py-1.5 text-xs text-slate-400">{{ l.day }}</td>
            <td class="py-1.5 font-medium">{{ l.model }}</td>
            <td class="py-1.5 text-xs text-slate-400">{{ l.providerName }}</td>
            <td class="py-1.5">{{ l.requests }}</td>
            <td class="py-1.5 text-xs text-slate-400">{{ l.inputTokens + l.outputTokens }}</td>
            <td class="py-1.5 text-right">{{ fmtMoney(l.costUsd) }}</td>
          </tr>
        </tbody>
      </table>
    </section>
  </div>
</template>