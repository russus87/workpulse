//! WorkPulse core — logica di tracciamento automatico del lavoro, pura e
//! riutilizzabile (nessuna dipendenza da Tauri o dall'OS).
//!
//! Pipeline:
//!   1. `classify`  — da finestra attiva ad attivita' classificata (progetto/ticket/cliente);
//!   2. `storage`   — persistenza locale su SQLite (tutto resta sul dispositivo);
//!   3. `aggregate` — tempi per app/progetto/ticket/cliente e metriche di produttivita';
//!   4. `git`       — lettura di branch e commit dai repo locali;
//!   5. `journal`   — Work Journal giornaliero per progetto;
//!   6. `summary`   — riepilogo in linguaggio naturale (template locale).

pub mod aggregate;
pub mod classify;
pub mod error;
pub mod git;
pub mod journal;
pub mod model;
pub mod report;
pub mod storage;
pub mod summary;
pub mod trends;

pub use error::{Error, Result};
