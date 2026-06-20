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
