//! Persistenza locale su SQLite (file unico nella cartella dati dell'utente).
//!
//! Scelte di privacy: tutto resta sul dispositivo. Nessuna telemetria, nessun
//! invio remoto. Il file DB e' l'unica fonte di verita' e l'utente puo'
//! cancellarlo in qualsiasi momento (vedi `purge_before`).

use crate::error::Result;
use crate::model::{ActivitySample, Category, GitCommit, Meeting, UsageRow};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

/// Handle del database di WorkPulse.
pub struct Store {
    conn: Connection,
}

/// Dimensione su cui raggruppare l'utilizzo nelle query `usage_by`.
#[derive(Debug, Clone, Copy)]
pub enum Dimension {
    App,
    Project,
    Ticket,
    Client,
    Category,
}

impl Dimension {
    fn column(self) -> &'static str {
        match self {
            Dimension::App => "app",
            Dimension::Project => "project",
            Dimension::Ticket => "ticket",
            Dimension::Client => "client",
            Dimension::Category => "category",
        }
    }
}

impl Store {
    /// Apre (o crea) il DB al percorso indicato e applica lo schema.
    pub fn open(path: &str) -> Result<Self> {
        Self::open_with_key(path, None)
    }

    /// Apre il DB con cifratura a riposo opzionale (SQLCipher). Se `key` e' `Some`,
    /// il file e' cifrato AES-256 con la passphrase data (deve essere quella usata
    /// alla creazione). Con `None` il comportamento e' quello di uno SQLite normale.
    pub fn open_with_key(path: &str, key: Option<&str>) -> Result<Self> {
        let conn = Connection::open(path)?;
        if let Some(k) = key {
            // La PRAGMA key va impostata PRIMA di qualsiasi altra operazione.
            conn.pragma_update(None, "key", k)?;
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    /// DB in memoria, utile nei test.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Migra un DB in chiaro verso un nuovo file cifrato (SQLCipher export).
    /// Usata una volta quando l'utente attiva la cifratura su dati esistenti.
    pub fn export_encrypted(src: &str, dst: &str, key: &str) -> Result<()> {
        let conn = Connection::open(src)?;
        conn.execute("ATTACH DATABASE ?1 AS enc KEY ?2", params![dst, key])?;
        conn.query_row("SELECT sqlcipher_export('enc')", [], |_| Ok(()))?;
        conn.execute("DETACH DATABASE enc", [])?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS samples (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                app         TEXT NOT NULL,
                title       TEXT NOT NULL,
                url         TEXT,
                category    TEXT NOT NULL,
                project     TEXT,
                ticket      TEXT,
                client      TEXT,
                git_branch  TEXT,
                start       TEXT NOT NULL,
                seconds     INTEGER NOT NULL,
                idle        INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_samples_start   ON samples(start);
            CREATE INDEX IF NOT EXISTS idx_samples_project ON samples(project);

            CREATE TABLE IF NOT EXISTS commits (
                hash      TEXT PRIMARY KEY,
                repo      TEXT NOT NULL,
                author    TEXT NOT NULL,
                message   TEXT NOT NULL,
                branch    TEXT NOT NULL,
                project   TEXT,
                at        TEXT NOT NULL,
                additions INTEGER NOT NULL DEFAULT 0,
                deletions INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_commits_at ON commits(at);

            CREATE TABLE IF NOT EXISTS meetings (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                ext_id           TEXT NOT NULL UNIQUE,
                subject          TEXT NOT NULL,
                start            TEXT NOT NULL,
                duration_seconds INTEGER NOT NULL,
                is_online        INTEGER NOT NULL DEFAULT 0,
                organizer        TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_meetings_start ON meetings(start);

            CREATE TABLE IF NOT EXISTS presence (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                availability TEXT NOT NULL,
                activity     TEXT NOT NULL,
                start        TEXT NOT NULL,
                seconds      INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_presence_start ON presence(start);
            "#,
        )?;
        Ok(())
    }

    /// Registra un campione di presence Teams (stato osservato per `seconds`).
    pub fn insert_presence(
        &self,
        availability: &str,
        activity: &str,
        at: DateTime<Utc>,
        seconds: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO presence (availability,activity,start,seconds) VALUES (?1,?2,?3,?4)",
            params![availability, activity, at.to_rfc3339(), seconds],
        )?;
        Ok(())
    }

    /// Tempo per stato Teams (`activity`) in un intervallo, dal piu' lungo.
    pub fn presence_by_activity(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<UsageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT activity, SUM(seconds) FROM presence
             WHERE start >= ?1 AND start < ?2 GROUP BY activity ORDER BY 2 DESC",
        )?;
        let rows = stmt
            .query_map(params![from.to_rfc3339(), to.to_rfc3339()], |r| {
                Ok(UsageRow { key: r.get(0)?, seconds: r.get(1)? })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Registra/aggiorna un meeting (idempotente per `ext_id`).
    pub fn upsert_meeting(&self, m: &Meeting) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meetings (ext_id,subject,start,duration_seconds,is_online,organizer)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(ext_id) DO UPDATE SET
                subject=excluded.subject,
                start=excluded.start,
                duration_seconds=excluded.duration_seconds,
                is_online=excluded.is_online,
                organizer=excluded.organizer",
            params![
                m.ext_id, m.subject, m.start.to_rfc3339(),
                m.duration_seconds, m.is_online as i64, m.organizer,
            ],
        )?;
        Ok(())
    }

    /// Meeting in un intervallo, ordinati per inizio.
    pub fn meetings_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Meeting>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,ext_id,subject,start,duration_seconds,is_online,organizer
             FROM meetings WHERE start >= ?1 AND start < ?2 ORDER BY start ASC",
        )?;
        let rows = stmt
            .query_map(params![from.to_rfc3339(), to.to_rfc3339()], |r| {
                let start: String = r.get(3)?;
                Ok(Meeting {
                    id: Some(r.get(0)?),
                    ext_id: r.get(1)?,
                    subject: r.get(2)?,
                    start: DateTime::parse_from_rfc3339(&start)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    duration_seconds: r.get(4)?,
                    is_online: r.get::<_, i64>(5)? != 0,
                    organizer: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Inserisce un sample e ne restituisce l'id.
    pub fn insert_sample(&self, s: &ActivitySample) -> Result<i64> {
        let cat = serde_json::to_value(s.category)?
            .as_str()
            .unwrap_or("other")
            .to_string();
        self.conn.execute(
            "INSERT INTO samples
                (app,title,url,category,project,ticket,client,git_branch,start,seconds,idle)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                s.app, s.title, s.url, cat, s.project, s.ticket, s.client,
                s.git_branch, s.start.to_rfc3339(), s.seconds, s.idle as i64,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Registra un commit (idempotente per hash).
    pub fn upsert_commit(&self, c: &GitCommit) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO commits
                (hash,repo,author,message,branch,project,at,additions,deletions)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                c.hash, c.repo, c.author, c.message, c.branch, c.project,
                c.at.to_rfc3339(), c.additions, c.deletions,
            ],
        )?;
        Ok(())
    }

    /// Tempo (secondi, escluso idle) raggruppato per dimensione, in un intervallo.
    /// Ordinato dal piu' usato al meno usato.
    pub fn usage_by(
        &self,
        dim: Dimension,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<UsageRow>> {
        let col = dim.column();
        let sql = format!(
            "SELECT COALESCE({col},'(non assegnato)') AS k, SUM(seconds) AS s
             FROM samples
             WHERE idle = 0 AND start >= ?1 AND start < ?2
             GROUP BY k ORDER BY s DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![from.to_rfc3339(), to.to_rfc3339()], |r| {
                Ok(UsageRow {
                    key: r.get(0)?,
                    seconds: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Tutti i sample (non idle) di un intervallo, ordinati per inizio.
    pub fn samples_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<ActivitySample>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,app,title,url,category,project,ticket,client,git_branch,start,seconds,idle
             FROM samples WHERE start >= ?1 AND start < ?2 ORDER BY start ASC",
        )?;
        let rows = stmt
            .query_map(params![from.to_rfc3339(), to.to_rfc3339()], |r| {
                let cat: String = r.get(4)?;
                let category: Category =
                    serde_json::from_value(serde_json::Value::String(cat))
                        .unwrap_or(Category::Other);
                let start: String = r.get(9)?;
                Ok(ActivitySample {
                    id: Some(r.get(0)?),
                    app: r.get(1)?,
                    title: r.get(2)?,
                    url: r.get(3)?,
                    category,
                    project: r.get(5)?,
                    ticket: r.get(6)?,
                    client: r.get(7)?,
                    git_branch: r.get(8)?,
                    start: DateTime::parse_from_rfc3339(&start)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    seconds: r.get(10)?,
                    idle: r.get::<_, i64>(11)? != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Commit registrati in un intervallo, dal piu' recente.
    pub fn commits_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<GitCommit>> {
        let mut stmt = self.conn.prepare(
            "SELECT repo,hash,author,message,branch,project,at,additions,deletions
             FROM commits WHERE at >= ?1 AND at < ?2 ORDER BY at DESC",
        )?;
        let rows = stmt
            .query_map(params![from.to_rfc3339(), to.to_rfc3339()], |r| {
                let at: String = r.get(6)?;
                Ok(GitCommit {
                    repo: r.get(0)?,
                    hash: r.get(1)?,
                    author: r.get(2)?,
                    message: r.get(3)?,
                    branch: r.get(4)?,
                    project: r.get(5)?,
                    at: DateTime::parse_from_rfc3339(&at)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    additions: r.get(7)?,
                    deletions: r.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Aggiorna i campi modificabili di un sample (correzione manuale).
    /// I `None` lasciano il campo invariato; per azzerare un campo passare `Some("")`.
    pub fn update_sample(
        &self,
        id: i64,
        project: Option<&str>,
        ticket: Option<&str>,
        client: Option<&str>,
        idle: Option<bool>,
    ) -> Result<()> {
        let norm = |v: Option<&str>| v.map(|s| if s.is_empty() { None } else { Some(s.to_string()) });
        if let Some(p) = norm(project) {
            self.conn.execute("UPDATE samples SET project=?1 WHERE id=?2", params![p, id])?;
        }
        if let Some(t) = norm(ticket) {
            self.conn.execute("UPDATE samples SET ticket=?1 WHERE id=?2", params![t, id])?;
        }
        if let Some(c) = norm(client) {
            self.conn.execute("UPDATE samples SET client=?1 WHERE id=?2", params![c, id])?;
        }
        if let Some(b) = idle {
            self.conn.execute("UPDATE samples SET idle=?1 WHERE id=?2", params![b as i64, id])?;
        }
        Ok(())
    }

    /// Riassegna in blocco tutti i sample di un'app a un progetto/cliente
    /// (utile per correggere classificazioni sistematiche).
    pub fn reassign_app(&self, app: &str, project: &str, client: Option<&str>) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE samples SET project=?1, client=?2 WHERE app=?3",
            params![project, client, app],
        )?;
        Ok(n)
    }

    /// Elimina un sample (correzione manuale).
    pub fn delete_sample(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM samples WHERE id=?1", params![id])?;
        Ok(())
    }

    /// Blocchi di inattivita' (idle) di durata significativa, da riconciliare.
    pub fn idle_blocks(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        min_seconds: i64,
    ) -> Result<Vec<ActivitySample>> {
        let all = self.samples_between(from, to)?;
        Ok(all
            .into_iter()
            .filter(|s| s.idle && s.seconds >= min_seconds)
            .collect())
    }

    /// Cancella tutti i dati precedenti a una data (diritto all'oblio / retention).
    pub fn purge_before(&self, cutoff: DateTime<Utc>) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM samples WHERE start < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        self.conn.execute(
            "DELETE FROM commits WHERE at < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        self.conn.execute(
            "DELETE FROM meetings WHERE start < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        self.conn.execute(
            "DELETE FROM presence WHERE start < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::{Classifier, Rules};
    use crate::model::WindowSnapshot;
    use chrono::Duration;

    #[test]
    fn salva_e_aggrega_per_progetto() {
        let store = Store::open_in_memory().unwrap();
        let c = Classifier::new(Rules::default());
        let now = Utc::now();

        let snap = WindowSnapshot {
            app: "Code".into(),
            title: "PAM-1 work".into(),
            url: None,
            git_branch: None,
            idle: false,
            at: now,
        };
        store.insert_sample(&c.classify(&snap, 120)).unwrap();
        store.insert_sample(&c.classify(&snap, 60)).unwrap();

        let rows = store
            .usage_by(Dimension::Project, now - Duration::hours(1), now + Duration::hours(1))
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "PAM");
        assert_eq!(rows[0].seconds, 180);
    }
}
