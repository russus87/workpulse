<script>
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
  let settings = $state(null);
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
      [summary, metrics, byProject, byApp, byClient, byTicket, journalEntries, sheet] =
        await Promise.all([
          api.aiSummary(period),
          api.productivity(period),
          api.usageBy("project", period),
          api.usageBy("app", period),
          api.usageBy("client", period),
          api.usageBy("ticket", period),
          api.journal(period),
          api.timesheet(period),
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

    {#if view === "timesheet"}
      <div class="card">
        <h3>Timesheet automatico</h3>
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
            <button class="btn" onclick={saveSettings}>💾 Salva</button>
            <button class="btn ghost" onclick={() => api.purge(settings.retention_days).then(refresh)}>
              🗑️ Applica retention ora
            </button>
          </div>
          <p class="muted" style="margin-top:14px">
            🔒 Privacy by design: tutti i dati restano in locale sul tuo dispositivo.
            Nessuna telemetria, nessun invio remoto.
          </p>
        {:else}
          <p class="muted">Caricamento…</p>
        {/if}
      </div>
    {/if}
  </main>
</div>
