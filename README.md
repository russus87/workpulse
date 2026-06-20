# WorkPulse

**Tracciamento automatico del lavoro** — senza inserimenti manuali. WorkPulse
osserva in background applicazioni, finestre attive, repository Git e contesto
di lavoro, poi genera **tempo per progetto/ticket/cliente**, **Work Journal**,
**timesheet automatici**, **analytics di produttivita'** e **riepiloghi in
linguaggio naturale**.

> _"Oggi hai lavorato 3h 10m sul progetto PAM, corretto 4 bug, effettuato 6
> commit e partecipato a 2 meeting."_

Desktop app in **Rust + Tauri + Svelte**. Stessa struttura degli altri progetti
([glyphbox](https://github.com/russus87/glyphbox), oxiterm, oops): core Rust puro
+ shell Tauri sottile.

🔒 **Privacy by design**: tutti i dati restano in locale sul dispositivo. Nessuna
telemetria, nessun invio remoto.

---

## Funzionalita'

| Area | Cosa fa |
|------|---------|
| **Tracking automatico** | Campiona la finestra attiva, classifica per categoria (coding / browsing / comunicazione / documenti), deduce progetto, ticket e cliente. |
| **Tempo** | Per applicazione, per progetto, per ticket, per cliente. |
| **Work Journal** | Genera attivita' svolte, ticket toccati, commit effettuati, tempo investito — per giorno e per progetto. |
| **Timesheet** | Generazione giornaliera / settimanale / mensile, ripartita per progetto. |
| **Productivity Analytics** | Focus time, context switching, interruzioni, trend. |
| **AI Summary** | Riepilogo testuale della giornata (template locale, opzione LLM disattivata di default). |
| **Dashboard** | KPI, grafici a barre, analisi storica, confronti temporali. |

## Sorgenti dati

- **Sistema operativo**: applicazione e titolo della finestra attiva (cattura
  best-effort via strumenti di sistema — vedi [`capture.rs`](src-tauri/src/capture.rs)).
- **Git**: branch corrente e commit dei repo locali configurati (filtrati per
  autore) — vedi [`git.rs`](core/src/git.rs).
- **Browser / Jira / Outlook / Teams / Slack**: dedotti dal titolo finestra e
  dagli URL nella v0; connettori dedicati (API) previsti in **V1/V2** — vedi la
  [Roadmap](docs/ROADMAP.md).

## Architettura

```
┌───────────────────────────── Desktop app (Tauri) ─────────────────────────────┐
│  Svelte UI  ──invoke──▶  comandi Tauri (src-tauri/src/lib.rs)                   │
│                              │                                                  │
│   tracker (thread) ◀─────────┘   capture (OS)   git (commit)                    │
│        │                                                                        │
│        ▼                                                                        │
│  ┌──────────────────────── workpulse-core (Rust puro) ──────────────────────┐  │
│  │ classify → storage(SQLite) → aggregate → journal → summary               │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────────┘
```

Il **core** non dipende da Tauri: e' riutilizzabile da una CLI, un servizio
headless o un futuro server di sync. Dettagli in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

- **Modello dati** → [docs/DATA-MODEL.md](docs/DATA-MODEL.md)
- **API (comandi Tauri)** → [docs/API.md](docs/API.md)
- **Privacy by design** → [docs/PRIVACY.md](docs/PRIVACY.md)
- **Roadmap MVP → V1 → V2** → [docs/ROADMAP.md](docs/ROADMAP.md)

## Sviluppo

```bash
npm install
npm run tauri dev      # avvia app + UI in hot-reload
cargo test -p workpulse-core   # test della logica core
```

### Cattura finestra attiva (runtime)
- **Linux/X11**: richiede `xdotool` e `xprop` (`pacman -S xdotool xorg-xprop`).
- **macOS**: usa `osascript` (di serie). Va concesso il permesso *Accessibilita'*.
- **Windows**: usa PowerShell (di serie).

Se gli strumenti non sono disponibili, il tracker degrada con eleganza (salta il
campione) senza crashare.

## Build & Release

Tutto via GitHub Actions ([`.github/workflows/release.yml`](.github/workflows/release.yml)),
attivato dal push di un tag `v*`:

- **Linux**: `.AppImage`, `.deb`, `.rpm`
- **Arch Linux**: `.pkg.tar.zst` (job `arch` con `makepkg`)
- **Windows**: `.exe` (NSIS) + `.msi`
- **macOS**: `.dmg`/`.app` per **Apple Silicon (M1+)** (`aarch64-apple-darwin`)

```bash
git tag v0.1.0 && git push origin v0.1.0
```

## Licenza

MIT © Antonio Russo
