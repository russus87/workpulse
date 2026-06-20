# Privacy by design

WorkPulse osserva attivita' potenzialmente sensibili (titoli di finestre, URL,
messaggi di commit). La privacy non e' un'opzione: e' un vincolo architetturale.

## Principi

1. **Local-first, sempre.** Tutti i dati vivono in un unico file SQLite sul
   dispositivo. Nessuna telemetria, nessun account obbligatorio, nessun invio
   remoto nella versione base.
2. **Nessuna cattura di contenuto.** Si registrano metadati (app, titolo, durata,
   branch, hash commit), **mai** keystroke, screenshot o contenuto dei documenti.
3. **Controllo dell'utente.**
   - **Pausa tracciamento** con un clic (es. attivita' personali).
   - **Retention** configurabile (`retention_days`, 0 = illimitato); purge
     automatico all'avvio e manuale dalla UI (**diritto all'oblio**).
   - I dati sono in chiaro e ispezionabili: e' "solo" un file SQLite.
4. **Regole trasparenti.** La classificazione e' guidata da regole leggibili e
   modificabili (`Rules`), non da modelli opachi.
5. **AI opzionale e off-by-default.** Il riepilogo e' generato da un **template
   locale** deterministico. Un eventuale LLM e' opt-in esplicito; se attivato,
   l'utente sceglie tra modello locale o endpoint remoto, e l'`SummaryInput`
   contiene solo dati gia' aggregati (no titoli grezzi) se l'utente lo richiede.

## Cosa NON viene raccolto

- Battiture, clipboard, screenshot, audio/video dei meeting.
- Contenuto di email, chat, documenti.
- Posizione, rubrica, identita' di terzi.

## Dove finiscono i dati

| Dato | Posizione |
|------|-----------|
| Database attivita' | `<data_dir>/WorkPulse/workpulse.db` |
| Impostazioni | `<config_dir>/WorkPulse/settings.json` |
| Log | gestiti da `tauri-plugin-log` (locali) |

`<data_dir>`/`<config_dir>` seguono le convenzioni della piattaforma
(XDG su Linux, `Library/Application Support` su macOS, `%APPDATA%` su Windows).

## Sync e team analytics (V2, opt-in)

Qualsiasi funzione cloud sara' **disattivata di default** e, se abilitata:
- **cifratura end-to-end** lato client prima dell'invio;
- per le metriche di team, **solo aggregati anonimizzati** (es. ore per progetto),
  mai titoli o attivita' individuali identificabili;
- possibilita' di self-hosting del server di sync.

## Conformita'

L'approccio local-first e data-minimization e' allineato ai principi GDPR
(minimizzazione, limitazione della conservazione, controllo dell'interessato).
WorkPulse e' uno strumento personale di produttivita': l'uso per monitoraggio
dei dipendenti richiede basi giuridiche e trasparenza verso i lavoratori ed e'
**esplicitamente fuori dallo scopo** del prodotto base.
