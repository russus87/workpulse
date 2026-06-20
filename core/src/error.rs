//! Tipo d'errore unico del core, cosi' i comandi Tauri possono fare `?`
//! e poi convertire in stringa per la UI.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("errore database: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("errore (de)serializzazione: {0}")]
    Json(#[from] serde_json::Error),

    #[error("errore I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
