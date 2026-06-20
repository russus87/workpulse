//! Impostazioni dell'app, salvate come JSON nella cartella di configurazione
//! dell'utente. Includono le regole di classificazione, i repo Git tracciati e
//! l'identita' usata per filtrare i commit.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use workpulse_core::classify::Rules;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Secondi tra un campionamento e l'altro (minimo applicato: 5s).
    pub sample_seconds: i64,
    /// Email dell'autore usata per filtrare i commit Git propri.
    pub author_email: String,
    /// Percorsi assoluti dei repo Git da tracciare.
    pub git_repos: Vec<String>,
    /// Giorni di conservazione dei dati (retention). 0 = illimitato.
    pub retention_days: i64,
    /// Secondi di inattivita' oltre i quali un campione e' marcato idle.
    pub idle_threshold_seconds: u64,
    /// Avvio automatico di WorkPulse al login.
    pub autostart: bool,
    /// Invia una notifica di riepilogo a fine giornata.
    pub daily_summary: bool,
    /// Ora locale (0-23) a partire dalla quale inviare il riepilogo giornaliero.
    pub daily_summary_hour: u32,
    /// Connettore Microsoft Graph (Outlook/Teams) attivo.
    pub graph_enabled: bool,
    /// `client_id` dell'app Azure AD registrata dall'utente (public client).
    pub graph_client_id: String,
    /// Tenant Azure AD ("organizations" | "common" | GUID del tenant).
    pub graph_tenant: String,
    /// Refresh token salvato dopo l'autorizzazione (vuoto = non connesso).
    pub graph_refresh_token: String,
    /// Regole di classificazione (categorie app, mappa progetto->cliente).
    pub rules: Rules,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sample_seconds: 15,
            author_email: String::new(),
            git_repos: Vec::new(),
            retention_days: 365,
            idle_threshold_seconds: 120,
            autostart: false,
            daily_summary: true,
            daily_summary_hour: 18,
            graph_enabled: false,
            graph_client_id: String::new(),
            graph_tenant: "organizations".into(),
            graph_refresh_token: String::new(),
            rules: Rules::default(),
        }
    }
}

/// Percorso del file di configurazione (`<config>/WorkPulse/settings.json`).
pub fn config_path() -> PathBuf {
    let mut p = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("WorkPulse");
    let _ = std::fs::create_dir_all(&p);
    p.push("settings.json");
    p
}

/// Percorso del database SQLite (`<data>/WorkPulse/workpulse.db`).
pub fn db_path() -> PathBuf {
    let mut p = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("WorkPulse");
    let _ = std::fs::create_dir_all(&p);
    p.push("workpulse.db");
    p
}

impl Settings {
    /// Carica le impostazioni dal disco, o crea quelle di default.
    pub fn load() -> Self {
        let path = config_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Salva le impostazioni su disco.
    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(path, text)
    }
}
