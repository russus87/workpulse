//! Modello dati di WorkPulse.
//!
//! Il flusso e' a strati:
//!   WindowSnapshot  -> (classificazione) ->  ActivitySample  -> (storage)
//!   ActivitySample  -> (aggregazione)    ->  UsageReport / Journal / Summary
//!
//! Lo `snapshot` e' cio' che l'OS ci dice in un istante (app + titolo + url).
//! Il `sample` e' uno snapshot arricchito (categoria, progetto, ticket, cliente)
//! e con una durata: e' l'unita' base che salviamo e su cui calcoliamo tutto.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Categoria d'uso di un'attivita', derivata dall'app e dal contesto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    /// Editor/IDE, terminale, strumenti di sviluppo.
    Coding,
    /// Browser su documentazione, ticket, ricerca.
    Browsing,
    /// Riunioni e comunicazione (Teams, Slack, Outlook, Meet).
    Communication,
    /// Documenti, fogli, scrittura.
    Documents,
    /// Tutto cio' che non rientra altrove.
    Other,
}

impl Category {
    /// Vero se la categoria conta come "lavoro di concentrazione" (focus).
    pub fn is_focus(self) -> bool {
        matches!(self, Category::Coding | Category::Documents)
    }
}

/// Cio' che l'OS riporta in un istante: l'app in primo piano e cosa mostra.
///
/// `url` e `git_branch` sono opzionali e arrivano solo se disponibili
/// (es. integrazione browser o rilevamento del repo Git attivo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSnapshot {
    /// Nome dell'applicazione (es. "Code", "firefox", "Teams").
    pub app: String,
    /// Titolo della finestra attiva.
    pub title: String,
    /// URL della scheda attiva del browser, se noto.
    pub url: Option<String>,
    /// Branch Git del progetto in primo piano, se rilevato.
    pub git_branch: Option<String>,
    /// L'utente e' considerato attivo (input mouse/tastiera recente)?
    pub idle: bool,
    /// Istante della rilevazione.
    pub at: DateTime<Utc>,
}

/// Un'attivita' classificata e con durata: l'unita' base persistita.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySample {
    pub id: Option<i64>,
    pub app: String,
    pub title: String,
    pub url: Option<String>,
    pub category: Category,
    /// Progetto dedotto (es. "PAM"), se riconosciuto.
    pub project: Option<String>,
    /// Ticket dedotto (es. "PAM-1423"), se riconosciuto.
    pub ticket: Option<String>,
    /// Cliente dedotto dal progetto/regole, se noto.
    pub client: Option<String>,
    /// Branch Git, se rilevato.
    pub git_branch: Option<String>,
    /// Inizio dell'intervallo.
    pub start: DateTime<Utc>,
    /// Durata in secondi attribuita a questo sample.
    pub seconds: i64,
    /// L'utente era inattivo durante l'intervallo (idle non conta come focus).
    pub idle: bool,
}

/// Un commit Git osservato in un repo locale tracciato.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    pub repo: String,
    pub hash: String,
    pub author: String,
    pub message: String,
    pub branch: String,
    pub project: Option<String>,
    pub at: DateTime<Utc>,
}

/// Un meeting importato da un calendario esterno (es. Outlook/Teams via Graph).
/// Tenuto separato dai `samples` per non raddoppiare il tempo gia' tracciato
/// dalla finestra attiva: serve a contare e contestualizzare le riunioni.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: Option<i64>,
    /// Id esterno dell'evento (per l'idempotenza degli import).
    pub ext_id: String,
    pub subject: String,
    pub start: DateTime<Utc>,
    pub duration_seconds: i64,
    /// Riunione online (Teams/Meet) vs in presenza.
    pub is_online: bool,
    pub organizer: Option<String>,
}

/// Riga di un report di utilizzo: "X secondi su questa chiave".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRow {
    /// Etichetta (nome app, progetto, ticket o cliente a seconda della query).
    pub key: String,
    pub seconds: i64,
}

/// Metriche di produttivita' calcolate su un intervallo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductivityMetrics {
    /// Secondi totali tracciati (esclusi i periodi idle).
    pub active_seconds: i64,
    /// Secondi in attivita' di focus (coding/documenti).
    pub focus_seconds: i64,
    /// Numero di cambi di contesto (passaggi tra progetti/app diversi).
    pub context_switches: i64,
    /// Numero di interruzioni (passaggi verso comunicazione che spezzano il focus).
    pub interruptions: i64,
    /// Durata media di un blocco di focus, in secondi.
    pub avg_focus_block_seconds: i64,
}

/// Una voce del Work Journal generato automaticamente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Giorno di riferimento (YYYY-MM-DD).
    pub day: String,
    /// Progetto a cui si riferisce la voce.
    pub project: Option<String>,
    /// Tempo investito sul progetto, in secondi.
    pub seconds: i64,
    /// Ticket toccati nella giornata per quel progetto.
    pub tickets: Vec<String>,
    /// Commit effettuati (messaggi sintetici).
    pub commits: Vec<String>,
}
