<script>
  import { save } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, humanDuration } from "./lib/api.js";
  import Bars from "./components/Bars.svelte";

  // Stato della UI: vista attiva e periodo selezionato.
  let view = $state("dashboard");
  let period = $state("today");

  // Dati caricati dal backend.
  let summary = $state("");
  let metrics = $state(null);
  let byProject = $state([]);
  let byApp = $state([]);
  let byClient = $state([]);
  let byTicket = $state([]);
  let journalEntries = $state([]);
  let sheet = $state([]);
  let trend = $state([]);
  let comparison = $state(null);
  let meetingList = $state([]);
  let teams = $state(null);
  let bill = $state(null);
  let langs = $state([]);
  let codeStats = $state(null);
  let heatCells = $state([]);
  let suggList = $state([]);
  let idleList = $state([]);
  let standupTxt = $state("");
  let insightsTxt = $state("");
  let focusSecs = $state(0);
  let settings = $state(null);
  // Stato del flusso di connessione Microsoft Graph.
  let graphDevice = $state(null);
  let graphMsg = $state("");
  let graphPoll = null;
  let paused = $state(false);
  let error = $state("");

  const periods = [
    { id: "today", label: "Oggi" },
    { id: "week", label: "Settimana" },
    { id: "month", label: "Mese" },
  ];

  // Ricarica tutti i dati del periodo corrente.
  async function refresh() {
    error = "";
    try {
      [summary, metrics, byProject, byApp, byClient, byTicket, journalEntries, sheet, trend, comparison] =
        await Promise.all([
          api.aiSummary(period),
          api.productivity(period),
          api.usageBy("project", period),
          api.usageBy("app", period),
          api.usageBy("client", period),
          api.usageBy("ticket", period),
          api.journal(period),
          api.timesheet(period),
          api.dailyTrend(period),
          api.comparePeriods(period),
        ]);
      meetingList = await api.meetings(period).catch(() => []);
      teams = await api.teamsActivity(period).catch(() => null);
      [bill, langs, codeStats, heatCells, suggList, idleList] = await Promise.all([
        api.billing(period).catch(() => null),
        api.languages(period).catch(() => []),
        api.codeTotals(period).catch(() => null),
        api.heat(period).catch(() => []),
        api.suggestions(period).catch(() => []),
        api.idleBlocks(period).catch(() => []),
      ]);
    } catch (e) {
      error = String(e);
    }
  }

  async function loadSettings() {
    try {
      settings = await api.getSettings();
    } catch (e) {
      error = String(e);
    }
  }

  function setPeriod(p) {
    period = p;
    refresh();
  }

  async function togglePause() {
    paused = await api.setPaused(!paused);
  }

  async function saveSettings() {
    await api.saveSettings(settings);
    await api.syncGit();
    refresh();
  }

  // Esporta il timesheet del periodo in un file CSV scelto dall'utente.
  async function exportCsv() {
    try {
      const csv = await api.exportCsv(period);
      const path = await save({
        defaultPath: `workpulse-${period}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (path) await api.saveText(path, csv);
    } catch (e) {
      error = String(e);
    }
  }

  // Formatta il delta percentuale di un confronto come "+12%" / "-5%" / "—".
  function deltaLabel(c) {
    if (!c || c.delta_pct === null || c.delta_pct === undefined) return "—";
    return (c.delta_pct >= 0 ? "+" : "") + c.delta_pct + "%";
  }

  // --- Connessione Microsoft 365 (Graph) tramite device code flow ---
  async function connectGraph() {
    error = "";
    try {
      await api.saveSettings(settings); // assicura client_id/tenant salvati
      const dc = await api.graphStartAuth();
      graphDevice = dc;
      graphMsg = dc.message;
      try { await openUrl(dc.verification_uri); } catch {}
      // Polling fino ad autorizzazione o errore.
      if (graphPoll) clearInterval(graphPoll);
      graphPoll = setInterval(async () => {
        try {
          const r = await api.graphPollAuth(dc.device_code);
          if (r === "ok") {
            clearInterval(graphPoll);
            graphDevice = null;
            graphMsg = "Connesso ✓ — sincronizzo i meeting…";
            await loadSettings();
            await syncGraph();
          }
        } catch (e) {
          clearInterval(graphPoll);
          graphDevice = null;
          graphMsg = "Errore: " + e;
        }
      }, (dc.interval || 5) * 1000);
    } catch (e) {
      graphMsg = "Errore: " + e;
    }
  }

  async function syncGraph() {
    try {
      const n = await api.graphSync();
      graphMsg = `Sincronizzati ${n} meeting.`;
      refresh();
    } catch (e) {
      graphMsg = "Errore sync: " + e;
    }
  }

  async function disconnectGraph() {
    if (graphPoll) clearInterval(graphPoll);
    graphDevice = null;
    await api.graphDisconnect();
    await loadSettings();
    graphMsg = "Disconnesso.";
  }

  async function loadStandup() {
    standupTxt = await api.standupText(period).catch((e) => "Errore: " + e);
  }
  async function copyStandup() {
    try { await navigator.clipboard.writeText(standupTxt); } catch {}
  }
  async function loadInsights() {
    insightsTxt = "Generazione in corso…";
    insightsTxt = await api.llmInsights(period).catch((e) => "Errore: " + e);
  }

  // Focus / Pomodoro.
  async function startFocus() {
    await api.focusStart(0);
    pollFocus();
  }
  async function stopFocus() {
    await api.focusStop();
    focusSecs = 0;
  }
  async function pollFocus() {
    focusSecs = await api.focusStatus().catch(() => 0);
  }

  // Correzione: assegna progetto a un blocco (idle o sample).
  async function assignProject(sample, project) {
    if (!project) return;
    await api.updateSample(sample.id, project, null, null, sample.idle ? false : null);
    refresh();
  }
  async function dropSample(id) {
    await api.deleteSample(id);
    refresh();
  }
  async function applySuggestion(s, project) {
    if (!project) return;
    await api.reassignApp(s.key, project, null);
    refresh();
  }

  // Cifratura DB.
  let encPass = $state("");
  async function enableEncryption() {
    try {
      const msg = await api.enableDbEncryption(encPass);
      encPass = "";
      await loadSettings();
      alert(msg);
    } catch (e) {
      error = String(e);
    }
  }

  // Onboarding.
  async function finishOnboarding() {
    settings.onboarded = true;
    await api.saveSettings(settings);
    await api.syncGit();
    refresh();
  }

  // Aggiorna lo stato del focus ogni 10s.
  $effect(() => {
    const t = setInterval(pollFocus, 10000);
    return () => clearInterval(t);
  });

  // Intensità heatmap (0..1) per il colore della cella.
  let heatMax = $derived(Math.max(1, ...heatCells.map((c) => c.seconds)));
  function heatAt(wd, h) {
    const c = heatCells.find((x) => x.weekday === wd && x.hour === h);
    return c ? c.seconds / heatMax : 0;
  }
  const weekdays = ["Lun", "Mar", "Mer", "Gio", "Ven", "Sab", "Dom"];

  // Carica i dati all'avvio.
  $effect(() => {
    refresh();
    loadSettings();
  });

  // Aggiorna automaticamente ogni 60s.
  $effect(() => {
    const t = setInterval(refresh, 60000);
    return () => clearInterval(t);
  });

  function reposText() {
    return settings ? settings.git_repos.join("\n") : "";
  }
  function setReposText(v) {
    if (settings) settings.git_repos = v.split("\n").map((s) => s.trim()).filter(Boolean);
  }
</script>

{#if settings && !settings.onboarded}
  <div class="onboard-overlay">
    <div class="card" style="max-width:520px; width:100%">
      <div class="brand" style="padding-top:0">Benvenuto in Work<span>Pulse</span></div>
      <p class="muted">Configura l'essenziale per iniziare a tracciare. Tutto resta in locale.</p>
      <label class="field">
        <span>La tua email (per riconoscere i tuoi commit Git)</span>
        <input bind:value={settings.author_email} placeholder="tu@azienda.it" />
      </label>
      <label class="field">
        <span>Repository Git da tracciare (uno per riga)</span>
        <textarea rows="3" value={settings.git_repos.join("\n")} oninput={(e) => settings.git_repos = e.target.value.split("\n").map(s=>s.trim()).filter(Boolean)}></textarea>
      </label>
      <label class="field">
        <span>Tariffa oraria di default (€) — opzionale</span>
        <input type="number" min="0" step="5" bind:value={settings.rates.default_hourly} />
      </label>
      <p class="muted">Potrai collegare Microsoft 365, attivare la cifratura e altro nelle Impostazioni.</p>
      <button class="btn" onclick={finishOnboarding}>Inizia →</button>
    </div>
  </div>
{/if}

<div class="app">
  <aside class="sidebar">
    <div class="brand">Work<span>Pulse</span></div>
    <button class="nav-item" class:active={view === "dashboard"} onclick={() => (view = "dashboard")}>📊 Dashboard</button>
    <button class="nav-item" class:active={view === "journal"} onclick={() => (view = "journal")}>📓 Work Journal</button>
    <button class="nav-item" class:active={view === "standup"} onclick={() => { view = "standup"; loadStandup(); }}>🗣️ Standup</button>
    <button class="nav-item" class:active={view === "billing"} onclick={() => (view = "billing")}>💶 Fatturazione</button>
    <button class="nav-item" class:active={view === "analytics"} onclick={() => (view = "analytics")}>📈 Analisi storica</button>
    <button class="nav-item" class:active={view === "heatmap"} onclick={() => (view = "heatmap")}>🔥 Heatmap</button>
    <button class="nav-item" class:active={view === "reconcile"} onclick={() => (view = "reconcile")}>🧩 Riconcilia</button>
    <button class="nav-item" class:active={view === "insights"} onclick={() => (view = "insights")}>🤖 Insight AI</button>
    <button class="nav-item" class:active={view === "timesheet"} onclick={() => (view = "timesheet")}>🗓️ Timesheet</button>
    <button class="nav-item" class:active={view === "settings"} onclick={() => (view = "settings")}>⚙️ Impostazioni</button>
    <div class="spacer"></div>
    {#if focusSecs > 0}
      <button class="nav-item" style="background:var(--accent-2);color:white" onclick={stopFocus}>
        🎯 Focus {Math.ceil(focusSecs / 60)}m — stop
      </button>
    {:else}
      <button class="nav-item" onclick={startFocus}>🎯 Avvia focus</button>
    {/if}
    <button class="nav-item" onclick={togglePause}>
      {paused ? "▶️ Riprendi tracking" : "⏸️ Pausa tracking"}
    </button>
  </aside>

  <main class="main">
    <div class="toolbar">
      <div class="periods">
        {#each periods as p}
          <button class:active={period === p.id} onclick={() => setPeriod(p.id)}>{p.label}</button>
        {/each}
      </div>
      <button class="btn ghost" onclick={refresh}>↻ Aggiorna</button>
    </div>

    {#if error}
      <div class="summary" style="border-color:var(--warn)">
        <span class="label" style="color:var(--warn)">Avviso</span>
        <div>{error}</div>
      </div>
    {/if}

    {#if view === "dashboard"}
      <div class="summary">
        <span class="label">AI Summary</span>
        <div>{summary || "—"}</div>
      </div>

      {#if metrics}
        <div class="kpis">
          <div class="kpi"><div class="v">{humanDuration(metrics.active_seconds)}</div><div class="k">Tempo attivo</div></div>
          <div class="kpi"><div class="v">{humanDuration(metrics.focus_seconds)}</div><div class="k">Focus time</div></div>
          <div class="kpi"><div class="v">{metrics.context_switches}</div><div class="k">Context switch</div></div>
          <div class="kpi"><div class="v">{metrics.interruptions}</div><div class="k">Interruzioni</div></div>
        </div>
      {/if}

      <div class="grid2">
        <div class="card"><h3>Tempo per progetto</h3><Bars rows={byProject} /></div>
        <div class="card"><h3>Tempo per applicazione</h3><Bars rows={byApp} green /></div>
        <div class="card"><h3>Tempo per cliente</h3><Bars rows={byClient} /></div>
        <div class="card"><h3>Tempo per ticket</h3><Bars rows={byTicket} green /></div>
      </div>

      {#if meetingList.length}
        <div class="card" style="margin-top:16px">
          <h3>Meeting da calendario ({meetingList.length})</h3>
          <table>
            <thead><tr><th>Orario</th><th>Titolo</th><th>Durata</th><th>Tipo</th></tr></thead>
            <tbody>
              {#each meetingList as m}
                <tr>
                  <td>{new Date(m.start).toLocaleString()}</td>
                  <td>{m.subject}</td>
                  <td>{humanDuration(m.duration_seconds)}</td>
                  <td>{m.is_online ? "🟢 online" : "🏢 presenza"}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}

      {#if teams && teams.connected}
        <div class="card" style="margin-top:16px">
          <div class="row" style="justify-content:space-between">
            <h3 style="margin:0">Attività Teams</h3>
            <span class="tag" style="border-color:var(--accent)">In call/meeting: {humanDuration(teams.in_call_seconds)}</span>
          </div>
          {#if teams.rows.length}
            <div style="margin-top:12px"><Bars rows={teams.rows} /></div>
          {:else}
            <p class="muted">Nessun dato di presence ancora. Resta connesso: WorkPulse campiona Teams ~ogni minuto.</p>
          {/if}
        </div>
      {/if}

      <div class="grid2" style="margin-top:16px">
        <div class="card">
          <h3>Tempo per linguaggio</h3>
          <Bars rows={langs} />
        </div>
        <div class="card">
          <h3>Codice (commit)</h3>
          {#if codeStats}
            <div class="kpis" style="grid-template-columns:repeat(3,1fr)">
              <div class="kpi"><div class="v">{codeStats.commits}</div><div class="k">Commit</div></div>
              <div class="kpi"><div class="v" style="color:var(--accent-2)">+{codeStats.additions}</div><div class="k">Righe agg.</div></div>
              <div class="kpi"><div class="v" style="color:var(--warn)">-{codeStats.deletions}</div><div class="k">Righe rim.</div></div>
            </div>
          {:else}
            <p class="muted">Nessun commit nel periodo.</p>
          {/if}
        </div>
      </div>
    {/if}

    {#if view === "journal"}
      <div class="card">
        <h3>Work Journal — {periods.find((p) => p.id === period)?.label}</h3>
        {#if journalEntries.length === 0}
          <p class="muted">Nessuna attivita' registrata.</p>
        {:else}
          {#each journalEntries as e}
            <div style="margin-bottom:16px">
              <div class="row" style="justify-content:space-between">
                <strong>{e.project ?? "(non assegnato)"}</strong>
                <span class="muted">{humanDuration(e.seconds)}</span>
              </div>
              {#if e.tickets.length}
                <div class="tags">{#each e.tickets as t}<span class="tag">{t}</span>{/each}</div>
              {/if}
              {#if e.commits.length}
                <ul class="muted" style="margin:6px 0 0; padding-left:18px">
                  {#each e.commits as c}<li>{c}</li>{/each}
                </ul>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    {/if}

    {#if view === "analytics"}
      {#if comparison}
        <div class="kpis" style="grid-template-columns:repeat(2,1fr)">
          <div class="kpi">
            <div class="v">{humanDuration(comparison.active.current)} <span class="muted" style="font-size:14px">({deltaLabel(comparison.active)})</span></div>
            <div class="k">Tempo attivo vs periodo precedente ({humanDuration(comparison.active.previous)})</div>
          </div>
          <div class="kpi">
            <div class="v">{humanDuration(comparison.focus.current)} <span class="muted" style="font-size:14px">({deltaLabel(comparison.focus)})</span></div>
            <div class="k">Focus vs periodo precedente ({humanDuration(comparison.focus.previous)})</div>
          </div>
        </div>
      {/if}
      <div class="card">
        <h3>Trend giornaliero — attivo</h3>
        <Bars rows={trend.map((d) => ({ key: d.day, seconds: d.active_seconds }))} />
      </div>
      <div class="card" style="margin-top:16px">
        <h3>Trend giornaliero — focus</h3>
        <Bars rows={trend.map((d) => ({ key: d.day, seconds: d.focus_seconds }))} green />
      </div>
    {/if}

    {#if view === "standup"}
      <div class="card">
        <div class="row" style="justify-content:space-between">
          <h3 style="margin:0">Standup — recap copiabile</h3>
          <div class="row">
            <button class="btn ghost" onclick={loadStandup}>↻ Rigenera</button>
            <button class="btn" onclick={copyStandup}>📋 Copia</button>
          </div>
        </div>
        <pre style="white-space:pre-wrap; font-family:inherit; margin-top:12px">{standupTxt || "—"}</pre>
      </div>
    {/if}

    {#if view === "billing"}
      <div class="card">
        <h3>Fatturazione per cliente</h3>
        {#if bill && bill.items.length}
          <table>
            <thead><tr><th>Cliente</th><th>Tracciato</th><th>Fatturabile</th><th>Tariffa</th><th>Importo</th></tr></thead>
            <tbody>
              {#each bill.items as it}
                <tr>
                  <td>{it.key}</td>
                  <td>{humanDuration(it.seconds)}</td>
                  <td>{humanDuration(it.billable_seconds)}</td>
                  <td>{bill.currency_hint}{it.hourly_rate}/h</td>
                  <td><strong>{bill.currency_hint}{it.amount.toFixed(2)}</strong></td>
                </tr>
              {/each}
            </tbody>
            <tfoot>
              <tr><td colspan="4" style="text-align:right">Totale</td><td><strong>{bill.currency_hint}{bill.total.toFixed(2)}</strong></td></tr>
            </tfoot>
          </table>
          <p class="muted" style="margin-top:10px">Imposta tariffe e arrotondamento nelle Impostazioni. Export grezzo via Timesheet → CSV.</p>
        {:else}
          <p class="muted">Nessun dato fatturabile nel periodo.</p>
        {/if}
      </div>
    {/if}

    {#if view === "heatmap"}
      <div class="card">
        <h3>Heatmap produttività (giorno × ora)</h3>
        <div style="overflow-x:auto">
          <table style="border-collapse:separate; border-spacing:2px">
            <tbody>
              {#each weekdays as wd, wi}
                <tr>
                  <td class="muted" style="border:none; padding:2px 6px">{wd}</td>
                  {#each Array(24) as _, h}
                    <td title="{wd} {h}:00" style="border:none; width:14px; height:14px; padding:0; border-radius:3px; background:rgba(47,129,247,{heatAt(wi, h)})"></td>
                  {/each}
                </tr>
              {/each}
              <tr><td style="border:none"></td>{#each Array(24) as _, h}<td class="muted" style="border:none; font-size:9px; text-align:center">{h % 3 === 0 ? h : ""}</td>{/each}</tr>
            </tbody>
          </table>
        </div>
      </div>
    {/if}

    {#if view === "reconcile"}
      <div class="card">
        <h3>Riconcilia blocchi inattivi ({idleList.length})</h3>
        <p class="muted" style="margin-top:0">Assegna i periodi di inattività: erano lavoro, una call offline o una pausa?</p>
        {#if idleList.length === 0}
          <p class="muted">Nessun blocco idle da riconciliare.</p>
        {:else}
          {#each idleList as s}
            <div class="row" style="justify-content:space-between; border-bottom:1px solid var(--border); padding:8px 0">
              <span>{new Date(s.start).toLocaleString()} · {humanDuration(s.seconds)}</span>
              <div class="row">
                <input placeholder="progetto" style="width:120px" onchange={(e) => assignProject(s, e.target.value)} />
                <button class="btn ghost" onclick={() => dropSample(s.id)}>🗑️</button>
              </div>
            </div>
          {/each}
        {/if}
      </div>

      {#if suggList.length}
        <div class="card" style="margin-top:16px">
          <h3>Suggerimenti regole</h3>
          {#each suggList as s}
            <div class="row" style="justify-content:space-between; padding:6px 0">
              <span>{s.message} <span class="muted">({humanDuration(s.seconds)})</span></span>
              <input placeholder="→ progetto" style="width:120px" onchange={(e) => applySuggestion(s, e.target.value)} />
            </div>
          {/each}
        </div>
      {/if}
    {/if}

    {#if view === "insights"}
      <div class="card">
        <div class="row" style="justify-content:space-between">
          <h3 style="margin:0">Insight AI (LLM locale)</h3>
          <button class="btn" onclick={loadInsights}>✨ Genera</button>
        </div>
        {#if settings && !settings.llm_enabled}
          <p class="muted">Abilita l'LLM locale (es. Ollama) nelle Impostazioni.</p>
        {/if}
        <pre style="white-space:pre-wrap; font-family:inherit; margin-top:12px">{insightsTxt || "Premi Genera per un'analisi del periodo."}</pre>
      </div>
    {/if}

    {#if view === "timesheet"}
      <div class="card">
        <div class="row" style="justify-content:space-between">
          <h3 style="margin:0">Timesheet automatico</h3>
          <button class="btn ghost" onclick={exportCsv}>⬇️ Esporta CSV</button>
        </div>
        {#if sheet.length === 0}
          <p class="muted">Nessun dato nel periodo.</p>
        {:else}
          <table>
            <thead><tr><th>Giorno</th><th>Ripartizione progetti</th><th>Totale</th></tr></thead>
            <tbody>
              {#each sheet as d}
                <tr>
                  <td>{d.day}</td>
                  <td>{d.rows.map((r) => `${r.key} ${humanDuration(r.seconds)}`).join(" · ")}</td>
                  <td>{humanDuration(d.total_seconds)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {/if}

    {#if view === "settings"}
      <div class="card" style="max-width:640px">
        <h3>Impostazioni</h3>
        {#if settings}
          <label class="field">
            <span>Email autore (per filtrare i tuoi commit Git)</span>
            <input bind:value={settings.author_email} placeholder="tu@azienda.it" />
          </label>
          <label class="field">
            <span>Repository Git da tracciare (uno per riga, percorsi assoluti)</span>
            <textarea rows="4" value={reposText()} oninput={(e) => setReposText(e.target.value)}></textarea>
          </label>
          <div class="row">
            <label class="field" style="flex:1">
              <span>Intervallo campionamento (secondi)</span>
              <input type="number" min="5" bind:value={settings.sample_seconds} />
            </label>
            <label class="field" style="flex:1">
              <span>Conservazione dati (giorni, 0 = illimitato)</span>
              <input type="number" min="0" bind:value={settings.retention_days} />
            </label>
          </div>
          <div class="row">
            <label class="field" style="flex:1">
              <span>Soglia inattivita' (secondi)</span>
              <input type="number" min="30" bind:value={settings.idle_threshold_seconds} />
            </label>
            <label class="field" style="flex:1">
              <span>Ora riepilogo giornaliero (0-23)</span>
              <input type="number" min="0" max="23" bind:value={settings.daily_summary_hour} />
            </label>
          </div>
          <label class="field row" style="gap:8px; align-items:center">
            <input type="checkbox" style="width:auto" bind:checked={settings.autostart} />
            <span style="margin:0">Avvia WorkPulse automaticamente al login</span>
          </label>
          <label class="field row" style="gap:8px; align-items:center">
            <input type="checkbox" style="width:auto" bind:checked={settings.daily_summary} />
            <span style="margin:0">Notifica di riepilogo a fine giornata</span>
          </label>

          <hr style="border-color:var(--border); margin:18px 0" />
          <h3>💶 Fatturazione</h3>
          <div class="row">
            <label class="field" style="flex:1">
              <span>Tariffa oraria di default (€)</span>
              <input type="number" min="0" step="5" bind:value={settings.rates.default_hourly} />
            </label>
            <label class="field" style="flex:1">
              <span>Arrotondamento (minuti)</span>
              <input type="number" min="0" step="5" bind:value={settings.billing_round_minutes} />
            </label>
          </div>

          <hr style="border-color:var(--border); margin:18px 0" />
          <h3>🎯 Focus & nudge</h3>
          <div class="row">
            <label class="field" style="flex:1">
              <span>Durata Pomodoro (min)</span>
              <input type="number" min="5" bind:value={settings.pomodoro_minutes} />
            </label>
            <label class="field" style="flex:1">
              <span>Nudge pausa dopo (min, 0=off)</span>
              <input type="number" min="0" bind:value={settings.nudge_no_break_minutes} />
            </label>
            <label class="field" style="flex:1">
              <span>Nudge comunicazione (min, 0=off)</span>
              <input type="number" min="0" bind:value={settings.nudge_comm_minutes} />
            </label>
          </div>

          <hr style="border-color:var(--border); margin:18px 0" />
          <h3>🔒 Privacy & cifratura</h3>
          <label class="field">
            <span>App personali (auto-pausa quando attive, una per riga)</span>
            <textarea rows="2" value={settings.personal_apps.join("\n")} oninput={(e) => settings.personal_apps = e.target.value.split("\n").map(s=>s.trim()).filter(Boolean)}></textarea>
          </label>
          <label class="field row" style="gap:8px; align-items:center">
            <input type="checkbox" style="width:auto" bind:checked={settings.private_autopause} />
            <span style="margin:0">Auto-pausa su finestre in incognito/privato</span>
          </label>
          {#if settings.db_encrypted}
            <p class="tag" style="border-color:var(--accent-2)">✓ Database cifrato a riposo</p>
          {:else}
            <div class="row">
              <input type="password" placeholder="passphrase (min 8)" bind:value={encPass} style="flex:1" />
              <button class="btn ghost" onclick={enableEncryption}>🔐 Cifra il database</button>
            </div>
          {/if}

          <hr style="border-color:var(--border); margin:18px 0" />
          <h3>🤖 LLM locale (Insight AI)</h3>
          <label class="field row" style="gap:8px; align-items:center">
            <input type="checkbox" style="width:auto" bind:checked={settings.llm_enabled} />
            <span style="margin:0">Abilita insight con LLM locale</span>
          </label>
          <div class="row">
            <label class="field" style="flex:2">
              <span>Endpoint (es. Ollama)</span>
              <input bind:value={settings.llm_endpoint} />
            </label>
            <label class="field" style="flex:1">
              <span>Modello</span>
              <input bind:value={settings.llm_model} />
            </label>
          </div>

          <div class="row">
            <button class="btn" onclick={saveSettings}>💾 Salva</button>
            <button class="btn ghost" onclick={() => api.purge(settings.retention_days).then(refresh)}>
              🗑️ Applica retention ora
            </button>
          </div>
          <hr style="border-color:var(--border); margin:18px 0" />
          <h3>🔗 Microsoft 365 (Outlook / Teams)</h3>
          <p class="muted" style="margin-top:0">
            Importa i meeting reali dal calendario. Serve un'app Azure AD (public
            client) con permesso <code>Calendars.Read</code>: incolla qui il suo
            <em>client_id</em>. L'autorizzazione avviene una sola volta dal browser.
          </p>
          <label class="field">
            <span>Client ID (app Azure AD)</span>
            <input bind:value={settings.graph_client_id} placeholder="00000000-0000-0000-0000-000000000000" />
          </label>
          <label class="field">
            <span>Tenant (organizations | common | GUID)</span>
            <input bind:value={settings.graph_tenant} />
          </label>
          <div class="row">
            {#if settings.graph_refresh_token}
              <span class="tag" style="border-color:var(--accent-2)">✓ Connesso</span>
              <button class="btn ghost" onclick={syncGraph}>🔄 Sincronizza meeting</button>
              <button class="btn ghost" onclick={disconnectGraph}>Disconnetti</button>
            {:else}
              <button class="btn" onclick={connectGraph}>🔗 Connetti Microsoft 365</button>
            {/if}
          </div>
          <label class="field row" style="gap:8px; align-items:center; margin-top:10px">
            <input type="checkbox" style="width:auto" bind:checked={settings.track_presence} />
            <span style="margin:0">Traccia attività Teams (presence: tempo in call/meeting/presenting)</span>
          </label>
          <p class="muted" style="margin-top:0">
            Richiede il permesso <code>Presence.Read</code> sull'app Azure AD. Se ti
            sei connesso prima di questa funzione, premi <em>Connetti</em> di nuovo
            per concedere il nuovo permesso.
          </p>
          {#if graphDevice}
            <div class="summary" style="margin-top:12px">
              <span class="label">Autorizzazione</span>
              <div>{graphDevice.message}</div>
              <div class="row" style="margin-top:8px">
                <span class="tag" style="font-size:16px; letter-spacing:2px">{graphDevice.user_code}</span>
                <button class="btn ghost" onclick={() => openUrl(graphDevice.verification_uri)}>Apri pagina di login</button>
              </div>
            </div>
          {:else if graphMsg}
            <p class="muted">{graphMsg}</p>
          {/if}

          <p class="muted" style="margin-top:14px">
            🔒 Privacy by design: tutti i dati restano in locale sul tuo dispositivo.
            Nessuna telemetria, nessun invio remoto. Il connettore Microsoft usa
            il device code flow (nessun client secret) e salva solo il refresh token.
          </p>
        {:else}
          <p class="muted">Caricamento…</p>
        {/if}
      </div>
    {/if}
  </main>
</div>
