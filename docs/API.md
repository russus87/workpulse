# API — comandi Tauri

La UI dialoga con il backend tramite `invoke(<comando>, <args>)`. Tutti i comandi
sono definiti in [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs) e wrappati in
[`src/lib/api.js`](../src/lib/api.js).

Il parametro `period` accetta `"today"` | `"week"` | `"month"` e viene risolto
sui confini del giorno **locale** dell'utente.

| Comando | Argomenti | Ritorna | Descrizione |
|---------|-----------|---------|-------------|
| `usage_by` | `{ dimension, period }` | `UsageRow[]` | Tempo per dimensione: `app`/`project`/`ticket`/`client`/`category`. |
| `productivity` | `{ period }` | `ProductivityMetrics` | Focus, context switch, interruzioni, blocco medio. |
| `ai_summary` | `{ period }` | `string` | Riepilogo in linguaggio naturale. |
| `journal` | `{ period }` | `JournalEntry[]` | Work Journal per progetto. |
| `timesheet` | `{ period }` | `TimesheetDay[]` | Timesheet giorno-per-giorno per progetto. |
| `export_csv` | `{ period }` | `string` | Timesheet del periodo in CSV (RFC 4180). |
| `save_text` | `{ path, content }` | — | Salva testo (es. CSV) su un percorso scelto. |
| `daily_trend` | `{ period }` | `DayTotal[]` | Serie giornaliera attivo/focus per i grafici storici. |
| `compare_periods` | `{ period }` | `PeriodComparison` | Confronto col periodo precedente equivalente (delta %). |
| `get_settings` | — | `Settings` | Impostazioni correnti. |
| `save_settings` | `{ newSettings }` | — | Persiste e applica nuove impostazioni. |
| `set_paused` | `{ paused }` | `bool` | Pausa/ripresa del tracciamento. |
| `sync_git` | — | — | Importa subito i commit dai repo configurati. |
| `purge` | `{ days }` | `number` | Cancella i dati piu' vecchi di `days` giorni. |

## Esempi di payload

`usage_by("project", "week")`:
```json
[
  { "key": "PAM", "seconds": 11400 },
  { "key": "CRM", "seconds": 4800 },
  { "key": "(non assegnato)", "seconds": 1200 }
]
```

`productivity("today")`:
```json
{
  "active_seconds": 18000,
  "focus_seconds": 12600,
  "context_switches": 23,
  "interruptions": 5,
  "avg_focus_block_seconds": 1800
}
```

`ai_summary("today")`:
```
"Oggi hai lavorato 3h 30m principalmente sul progetto PAM, corretto 4 bug,
 effettuato 6 commit e partecipato a 2 meeting. Focus: 3h 30m su 5h attivo
 (23 cambi di contesto, 5 interruzioni)."
```

## Note di design

- I comandi sono **adattatori sottili**: aprono la finestra temporale, interrogano
  lo `Store`, convertono gli errori del core in `String` per la UI.
- Nessun comando espone scrittura diretta dei sample: la registrazione e'
  esclusiva del `tracker` in background.
- Un eventuale generatore **LLM** rimpiazzerebbe solo `ai_summary`, ricevendo lo
  stesso `SummaryInput` del template locale (vedi [PRIVACY.md](PRIVACY.md)).
