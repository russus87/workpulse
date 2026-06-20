# Architettura

WorkPulse e' un'app desktop **Rust + Tauri + Svelte** con la stessa filosofia
degli altri progetti del repo (glyphbox, oxiterm): **un core Rust puro** che
contiene tutta la logica, e **una shell Tauri sottile** che lo espone alla UI e
lo collega al sistema operativo.

## Componenti

### 1. `workpulse-core` (crate Rust, nessuna dipendenza da Tauri/OS)

| Modulo | Responsabilita' |
|--------|-----------------|
| `model` | Tipi del dominio: `WindowSnapshot`, `ActivitySample`, `GitCommit`, `ProductivityMetrics`, `JournalEntry`. |
| `classify` | Da snapshot grezzo ad attivita' classificata (categoria, progetto, ticket, cliente) tramite **regole configurabili** (`Rules`). |
| `storage` | Persistenza su **SQLite** (rusqlite "bundled", zero dipendenze di sistema). Query di aggregazione per dimensione e finestra temporale. |
| `aggregate` | Metriche pure: tempo attivo, focus, context switch, interruzioni, durata media dei blocchi di focus. |
| `git` | Lettura di branch e commit dai repo locali via shell-out a `git` (nessun linking di libgit2). |
| `journal` | Costruisce il Work Journal giornaliero per progetto. |
| `summary` | Genera il riepilogo in linguaggio naturale (template locale; contratto `SummaryInput` pronto per un generatore LLM opzionale). |

Essendo puro e testabile, il core gira su qualunque piattaforma e potra' essere
riusato da una CLI, un daemon headless o un server di sync (V2).

### 2. `workpulse` (src-tauri — shell desktop)

| Modulo | Responsabilita' |
|--------|-----------------|
| `capture` | Cattura della finestra attiva **specifica per OS**, via shell-out (`xdotool`/`xprop` su Linux, `osascript` su macOS, PowerShell su Windows). Zero dipendenze native → build semplice su tutte le piattaforme. |
| `tracker` | Thread in background: campiona a intervalli regolari, classifica, persiste; importa periodicamente i commit Git. Gestisce pausa/ripresa. |
| `settings` | Impostazioni persistite come JSON (regole, repo, retention, intervallo); percorsi di config e DB nelle cartelle standard dell'utente. |
| `lib` | Comandi Tauri (`#[tauri::command]`) che la UI invoca: vedi [API.md](API.md). |

### 3. UI Svelte (`src/`)

Single-page con sidebar e 4 viste: **Dashboard** (AI summary, KPI, grafici),
**Work Journal**, **Timesheet**, **Impostazioni**. La UI non fa polling pesante:
i dati sono gia' aggregati dal backend; aggiornamento automatico ogni 60s.

## Flusso dati

```
OS ─▶ capture::snapshot ─▶ classify::classify ─▶ storage.insert_sample
git log ───────────────────────────────────────▶ storage.upsert_commit
                                                        │
UI invoke ─▶ comando Tauri ─▶ storage.query ─▶ aggregate / journal / summary ─▶ UI
```

## Perche' queste scelte

- **Core senza Tauri**: testabile in CI senza GUI (i 6 test del core girano in
  ~13s), riutilizzabile, e separa nettamente la logica dalla piattaforma.
- **SQLite bundled**: nessuna dipendenza di sistema, file unico, facile da
  pacchettizzare e da cancellare (privacy).
- **Cattura via shell-out**: niente librerie native da linkare → la CI compila
  identica su Linux/Windows/macOS senza toolchain extra, e il fallimento di uno
  strumento a runtime degrada senza crash.
- **Git via comando `git`**: nessun libgit2, usa la config dell'utente.

## Componenti server (futuri, V2)

Opt-in, mai obbligatori: un servizio di **sync cifrato end-to-end** per
consolidare piu' dispositivi e un'API di team analytics aggregata/anonimizzata.
Vedi [ROADMAP.md](ROADMAP.md) e [PRIVACY.md](PRIVACY.md).
