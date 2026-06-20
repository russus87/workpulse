# Connettori esterni

## Microsoft 365 — Outlook Calendar / Teams (Graph)

Importa i **meeting reali** dal calendario (inclusi i Teams) per contarli e
contestualizzarli con precisione, al posto della sola euristica sui titoli finestra.

### Come funziona

- **Auth**: OAuth 2.0 **Device Code Flow** (public client, *nessun client secret*).
  Autorizzi una volta dal browser con un codice; WorkPulse salva solo il
  `refresh_token` (in locale) per i sync successivi.
- **Permesso minimo**: `Calendars.Read` (+ `offline_access` per il refresh).
- **Dati letti**: oggetto, inizio/fine, se la riunione e' online, organizzatore.
  I meeting stanno in una tabella separata (`meetings`) per **non raddoppiare**
  il tempo gia' tracciato dalla finestra attiva.

### Setup (una tantum) dell'app Azure AD

1. [portal.azure.com](https://portal.azure.com) → *Microsoft Entra ID* →
   *App registrations* → **New registration**.
2. Nome a piacere; *Supported account types*: a seconda dell'organizzazione.
3. In **Authentication** → *Advanced settings* → **Allow public client flows** = **Sì**
   (necessario per il device code flow).
4. In **API permissions** → *Add a permission* → *Microsoft Graph* →
   *Delegated* → **Calendars.Read**. (Aggiungi consenso se richiesto.)
5. Copia il **Application (client) ID**.

### In WorkPulse

Impostazioni → *Microsoft 365*:
- incolla il **Client ID** (e il **tenant**: `organizations`, `common` o il GUID);
- **Connetti** → si apre la pagina di login Microsoft con un codice → autorizza;
- al termine i meeting vengono sincronizzati (pulsante **Sincronizza** per i successivi).

I comandi backend sono: `graph_start_auth`, `graph_poll_auth`, `graph_sync`,
`graph_disconnect` (vedi [API.md](API.md)). Il parsing della risposta calendario
e' coperto da test (`graph::tests::parsing_calendar_view`).

### Privacy

Il connettore e' **opt-in** e disattivato di default. Nessun client secret;
viene salvato solo il `refresh_token` in locale. Disconnettendo, il token viene
rimosso. Vedi [PRIVACY.md](PRIVACY.md).

---

## In arrivo (vedi [ROADMAP.md](ROADMAP.md))

- **Teams presence** e canali (oltre ai meeting da calendario).
- **Jira** (REST, token): stato/titolo ticket, tempo per ticket.
- **Slack** (API): presenza/canali.
- **Estensione browser**: URL/titolo scheda affidabili.

Tutti seguiranno lo stesso schema: connettore isolato, parsing testato, dati
local-first, attivazione esplicita.
