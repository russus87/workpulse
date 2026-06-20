# Modello dati

Tutto e' persistito **localmente** in un singolo file SQLite
(`<data>/WorkPulse/workpulse.db`). Due tabelle, indicizzate per le query temporali.

## Pipeline dei tipi

```
WindowSnapshot ──classify──▶ ActivitySample ──store──▶ samples (tabella)
git log ──────────────────▶ GitCommit ──────store──▶ commits (tabella)

samples ──aggregate──▶ UsageRow[] / ProductivityMetrics
samples + commits ──▶ JournalEntry[] / AI Summary
```

## Tabella `samples`

Unita' base: un intervallo di attivita' classificato.

| Campo | Tipo | Note |
|-------|------|------|
| `id` | INTEGER PK | autoincrement |
| `app` | TEXT | nome applicazione (es. `Code`, `firefox`, `Teams`) |
| `title` | TEXT | titolo finestra attiva |
| `url` | TEXT? | URL scheda browser, se noto |
| `category` | TEXT | `coding` \| `browsing` \| `communication` \| `documents` \| `other` |
| `project` | TEXT? | dedotto (es. `PAM`) |
| `ticket` | TEXT? | dedotto (es. `PAM-1423`) |
| `client` | TEXT? | mappato da `project` via regole |
| `git_branch` | TEXT? | branch rilevato |
| `start` | TEXT | RFC3339 (UTC) |
| `seconds` | INTEGER | durata attribuita |
| `idle` | INTEGER | 0/1; i sample idle non contano nelle metriche |

Indici: `idx_samples_start`, `idx_samples_project`.

## Tabella `commits`

| Campo | Tipo | Note |
|-------|------|------|
| `hash` | TEXT PK | idempotente (INSERT OR IGNORE) |
| `repo` | TEXT | nome breve del repo |
| `author` | TEXT | autore del commit |
| `message` | TEXT | messaggio (subject) |
| `branch` | TEXT | branch corrente del repo |
| `project` | TEXT? | dedotto dal primo ticket nel messaggio |
| `at` | TEXT | RFC3339 (UTC) |

Indice: `idx_commits_at`.

## Classificazione (`Rules`)

Le regole sono **dati configurabili**, non codice:

- `coding_apps`, `communication_apps`, `document_apps`, `browser_apps`: liste di
  nomi-app (match case-insensitive su sottostringa) → determinano la `category`.
- `project_to_client`: mappa `"PAM" -> "Acme S.p.A."` → popola `client`.
- Riconoscimento ticket/progetto: regex `\b([A-Z][A-Z0-9]{1,9})-(\d{1,6})\b`
  applicata a titolo finestra, branch e URL (in quest'ordine).

## Aggregazioni derivate

- **UsageRow** `{ key, seconds }`: tempo per dimensione
  (`app`/`project`/`ticket`/`client`/`category`) in un intervallo, ordinato.
- **ProductivityMetrics**: `active_seconds`, `focus_seconds`, `context_switches`,
  `interruptions`, `avg_focus_block_seconds`.
- **JournalEntry** `{ day, project, seconds, tickets[], commits[] }`.

## Retention / diritto all'oblio

`purge_before(cutoff)` elimina sample e commit precedenti alla data. Applicata
automaticamente all'avvio in base a `retention_days` (0 = illimitato) e
invocabile manualmente dalla UI.
