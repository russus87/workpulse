# Roadmap

Da MVP a V2. Ogni fase e' utilizzabile da sola; nessuna funzione cloud e'
obbligatoria.

## MVP (0.1.x) — "traccia e racconta" ✅ scaffold attuale

Obiettivo: tracciamento automatico locale + reportistica di base.

- [x] Core Rust puro: modello, storage SQLite, classificazione, aggregazione,
      journal, summary (con test).
- [x] Cattura finestra attiva per OS (Linux/X11, macOS, Windows) via shell-out.
- [x] Tracker in background con pausa/ripresa.
- [x] Import commit dai repo Git locali (per autore).
- [x] Dashboard: AI summary, KPI (focus/context switch/interruzioni), tempo per
      progetto/app/cliente/ticket.
- [x] Work Journal e Timesheet (oggi/settimana/mese).
- [x] Impostazioni: regole, repo, intervallo, retention.
- [x] CI multi-piattaforma (AppImage/deb/rpm, .pkg.tar.zst, NSIS/MSI, dmg M1).
- [x] Idle detection reale (input inattivo) per OS (xprintidle / ioreg / GetLastInputInfo).
- [x] Icone e branding dedicati (logo "pulse", set completo generato con `tauri icon`).

## V1 (0.2–0.5) — "connettori e precisione"

Obiettivo: dati piu' ricchi e accurati tramite integrazioni dirette.

- [ ] **Browser**: estensione/native-messaging per URL e titolo scheda affidabili
      (oggi dedotti dal titolo finestra).
- [ ] **Jira**: connettore API (OAuth) per stato/titolo ticket, mappatura
      progetto↔cliente automatica, tempo per stato del ticket.
- [x] **Outlook Calendar / Teams**: meeting reali (durata, titolo, organizzatore,
      online sì/no) via Microsoft Graph (device code flow, `Calendars.Read`).
      Vedi [INTEGRATIONS.md](INTEGRATIONS.md). [ ] Teams presence/canali.
- [ ] **Slack**: presenza/canali via API per quantificare la comunicazione.
- [x] Idle/AFK detection nativa (vedi MVP). [ ] Unione automatica di sample contigui.
- [x] Trend di produttivita' (serie giornaliera, confronto col periodo precedente).
      [ ] Grafici avanzati e confronti settimana/mese estesi.
- [x] Export timesheet **CSV**. [ ] Export PDF e regole di arrotondamento.
- [x] Tray icon, avvio automatico, notifica di riepilogo giornaliero.

## V2 (0.6+) — "intelligenza e scala"

Obiettivo: insight piu' profondi e uso multi-dispositivo/team, sempre privacy-first.

- [ ] **AI Summary LLM** opzionale (locale o remoto), con riepiloghi settimanali
      e suggerimenti ("le tue ore di focus calano dopo le 15").
- [ ] **Sync end-to-end cifrato** multi-dispositivo (server self-hostable).
- [ ] **Team analytics** aggregati e anonimizzati (ore per progetto/cliente),
      senza esporre attivita' individuali.
- [ ] Goal & budget di tempo per cliente/progetto con alert.
- [ ] Rilevamento automatico del repo "in primo piano" (mapping finestra→repo).
- [ ] Plugin/regole community per classificazione.
- [ ] App companion mobile (sola consultazione).

## Principi trasversali

- **Privacy by design** in ogni fase (vedi [PRIVACY.md](PRIVACY.md)).
- **Core riutilizzabile**: ogni integrazione vive dietro un'interfaccia, il core
  resta puro e testabile.
- **Degradazione elegante**: se un connettore o uno strumento OS manca, l'app
  continua a funzionare con meno dati, mai con un crash.
