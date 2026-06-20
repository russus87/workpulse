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

<div class="app">
  <aside class="sidebar">
    <div class="brand">Work<span>Pulse</span></div>
    <button class="nav-item" class:active={view === "dashboard"} onclick={() => (view = "dashboard")}>📊 Dashboard</button>
    <button class="nav-item" class:active={view === "journal"} onclick={() => (view = "journal")}>📓 Work Journal</button>
    <button class="nav-item" class:active={view === "analytics"} onclick={() => (view = "analytics")}>📈 Analisi storica</button>
    <button class="nav-item" class:active={view === "timesheet"} onclick={() => (view = "timesheet")}>🗓️ Timesheet</button>
    <button class="nav-item" class:active={view === "settings"} onclick={() => (view = "settings")}>⚙️ Impostazioni</button>
    <div class="spacer"></div>
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
