// Sottile wrapper attorno a `invoke` di Tauri. Centralizza le chiamate al
// backend e, se la UI gira fuori da Tauri (es. `vite` nel browser), evita di
// crashare restituendo valori vuoti: utile per sviluppare la UI in isolamento.
import { invoke } from "@tauri-apps/api/core";

async function call(cmd, args = {}) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    console.warn(`invoke ${cmd} fallito:`, e);
    throw e;
  }
}

export const api = {
  usageBy: (dimension, period) => call("usage_by", { dimension, period }),
  productivity: (period) => call("productivity", { period }),
  aiSummary: (period) => call("ai_summary", { period }),
  journal: (period) => call("journal", { period }),
  timesheet: (period) => call("timesheet", { period }),
  exportCsv: (period) => call("export_csv", { period }),
  saveText: (path, content) => call("save_text", { path, content }),
  dailyTrend: (period) => call("daily_trend", { period }),
  comparePeriods: (period) => call("compare_periods", { period }),
  meetings: (period) => call("meetings", { period }),
  graphStartAuth: () => call("graph_start_auth"),
  graphPollAuth: (deviceCode) => call("graph_poll_auth", { deviceCode }),
  graphSync: () => call("graph_sync"),
  graphDisconnect: () => call("graph_disconnect"),
  teamsActivity: (period) => call("teams_activity", { period }),
  billing: (period) => call("billing", { period }),
  standupText: (period) => call("standup_text", { period }),
  languages: (period) => call("languages", { period }),
  codeTotals: (period) => call("code_totals", { period }),
  heat: (period) => call("heat", { period }),
  suggestions: (period) => call("suggestions", { period }),
  updateSample: (id, project, ticket, client, idle) =>
    call("update_sample", { id, project, ticket, client, idle }),
  reassignApp: (app, project, client) => call("reassign_app", { app, project, client }),
  deleteSample: (id) => call("delete_sample", { id }),
  idleBlocks: (period) => call("idle_blocks", { period }),
  focusStart: (minutes) => call("focus_start", { minutes }),
  focusStop: () => call("focus_stop"),
  focusStatus: () => call("focus_status"),
  llmInsights: (period) => call("llm_insights", { period }),
  enableDbEncryption: (passphrase) => call("enable_db_encryption", { passphrase }),
  getSettings: () => call("get_settings"),
  saveSettings: (newSettings) => call("save_settings", { newSettings }),
  setPaused: (paused) => call("set_paused", { paused }),
  syncGit: () => call("sync_git"),
  purge: (days) => call("purge", { days }),
};

// Formatta secondi come "3h 20m" / "45m" / "30s".
export function humanDuration(seconds) {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}
