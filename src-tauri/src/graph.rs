//! Connettore Microsoft Graph: importa i meeting da Outlook Calendar (inclusi i
//! Teams) per contare e contestualizzare le riunioni con precisione.
//!
//! Autenticazione: **OAuth 2.0 Device Code Flow** (public client, nessun secret).
//! L'utente registra un'app in Azure AD, inserisce il `client_id` nelle
//! impostazioni e autorizza una volta dal browser con un codice; salviamo il
//! `refresh_token` per i sync successivi. Permesso minimo richiesto: `Calendars.Read`.
//!
//! Tutte le chiamate usano `ureq` (TLS rustls). Le funzioni di rete restituiscono
//! `Result<_, String>`; il parsing della risposta calendario e' una funzione pura
//! e testata (`parse_calendar_view`).

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use workpulse_core::model::Meeting;

const SCOPE: &str = "offline_access Calendars.Read Presence.Read";

fn authority(tenant: &str) -> String {
    let t = if tenant.is_empty() { "organizations" } else { tenant };
    format!("https://login.microsoftonline.com/{t}/oauth2/v2.0")
}

/// Risposta dell'endpoint device code: cosa mostrare all'utente per autorizzare.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct TokenResp {
    access_token: String,
    refresh_token: Option<String>,
}

/// Esito di un giro di polling del token durante il device code flow.
pub enum Poll {
    /// In attesa che l'utente autorizzi; riprovare dopo `interval`.
    Pending,
    /// Autorizzato: (access_token, refresh_token).
    Done(String, Option<String>),
    /// Flusso terminato con errore (scaduto/negato/altro).
    Failed(String),
}

/// Avvia il device code flow: ritorna il codice da mostrare all'utente.
pub fn start_device_code(client_id: &str, tenant: &str) -> Result<DeviceCode, String> {
    let url = format!("{}/devicecode", authority(tenant));
    ureq::post(&url)
        .send_form(&[("client_id", client_id), ("scope", SCOPE)])
        .map_err(stringify_err)?
        .into_json::<DeviceCode>()
        .map_err(|e| e.to_string())
}

/// Esegue un singolo giro di polling del token.
pub fn poll_token(client_id: &str, tenant: &str, device_code: &str) -> Poll {
    let url = format!("{}/token", authority(tenant));
    let res = ureq::post(&url).send_form(&[
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:device_code",
        ),
        ("client_id", client_id),
        ("device_code", device_code),
    ]);
    match res {
        Ok(resp) => match resp.into_json::<TokenResp>() {
            Ok(t) => Poll::Done(t.access_token, t.refresh_token),
            Err(e) => Poll::Failed(e.to_string()),
        },
        Err(ureq::Error::Status(_, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            if body.contains("authorization_pending") || body.contains("slow_down") {
                Poll::Pending
            } else if body.contains("expired_token") {
                Poll::Failed("codice scaduto, riprova".into())
            } else if body.contains("authorization_declined") {
                Poll::Failed("autorizzazione negata".into())
            } else {
                Poll::Failed(body)
            }
        }
        Err(e) => Poll::Failed(e.to_string()),
    }
}

/// Scambia il refresh token per un nuovo access token.
pub fn refresh_access_token(
    client_id: &str,
    tenant: &str,
    refresh_token: &str,
) -> Result<(String, Option<String>), String> {
    let url = format!("{}/token", authority(tenant));
    let t = ureq::post(&url)
        .send_form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .map_err(stringify_err)?
        .into_json::<TokenResp>()
        .map_err(|e| e.to_string())?;
    Ok((t.access_token, t.refresh_token))
}

/// Scarica i meeting dell'intervallo dal calendario dell'utente.
pub fn fetch_meetings(
    access_token: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Meeting>, String> {
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/calendarView?startDateTime={}&endDateTime={}&$select=subject,start,end,isOnlineMeeting,organizer&$top=100&$orderby=start/dateTime",
        from.format("%Y-%m-%dT%H:%M:%S"),
        to.format("%Y-%m-%dT%H:%M:%S"),
    );
    let body = ureq::get(&url)
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("Prefer", "outlook.timezone=\"UTC\"")
        .call()
        .map_err(stringify_err)?
        .into_string()
        .map_err(|e| e.to_string())?;
    parse_calendar_view(&body)
}

// ----- Teams presence -----

#[derive(Debug, Deserialize)]
struct PresenceResp {
    availability: Option<String>,
    activity: Option<String>,
}

/// Stato Teams: (availability, activity). Es. ("Busy", "InACall").
pub fn parse_presence(json: &str) -> Result<(String, String), String> {
    let p: PresenceResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok((
        p.availability.unwrap_or_else(|| "Unknown".into()),
        p.activity.unwrap_or_else(|| "Unknown".into()),
    ))
}

/// Legge la presence corrente dell'utente (richiede scope `Presence.Read`).
pub fn fetch_presence(access_token: &str) -> Result<(String, String), String> {
    let body = ureq::get("https://graph.microsoft.com/v1.0/me/presence")
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
        .map_err(stringify_err)?
        .into_string()
        .map_err(|e| e.to_string())?;
    parse_presence(&body)
}

/// Vero se l'`activity` Teams indica una chiamata/riunione attiva.
pub fn is_in_call(activity: &str) -> bool {
    matches!(
        activity,
        "InACall" | "InAConferenceCall" | "InAMeeting" | "Presenting" | "OnThePhone"
    )
}

// ----- Parsing puro (testabile senza rete) -----

#[derive(Debug, Deserialize)]
struct CalendarView {
    value: Vec<GraphEvent>,
}

#[derive(Debug, Deserialize)]
struct GraphEvent {
    id: String,
    subject: Option<String>,
    start: GraphDateTime,
    end: GraphDateTime,
    #[serde(rename = "isOnlineMeeting")]
    is_online_meeting: Option<bool>,
    organizer: Option<Organizer>,
}

#[derive(Debug, Deserialize)]
struct GraphDateTime {
    #[serde(rename = "dateTime")]
    date_time: String,
}

#[derive(Debug, Deserialize)]
struct Organizer {
    #[serde(rename = "emailAddress")]
    email_address: Option<EmailAddress>,
}

#[derive(Debug, Deserialize)]
struct EmailAddress {
    name: Option<String>,
}

/// Converte il JSON di `calendarView` in `Meeting`.
pub fn parse_calendar_view(json: &str) -> Result<Vec<Meeting>, String> {
    let cv: CalendarView = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for e in cv.value {
        let start = parse_graph_dt(&e.start.date_time);
        let end = parse_graph_dt(&e.end.date_time);
        let duration_seconds = (end - start).num_seconds().max(0);
        out.push(Meeting {
            id: None,
            ext_id: e.id,
            subject: e.subject.unwrap_or_else(|| "(senza titolo)".into()),
            start,
            duration_seconds,
            is_online: e.is_online_meeting.unwrap_or(false),
            organizer: e.organizer.and_then(|o| o.email_address).and_then(|a| a.name),
        });
    }
    Ok(out)
}

/// Graph (con `Prefer: outlook.timezone="UTC"`) restituisce orari senza offset,
/// es. `2026-06-20T09:00:00.0000000`: li interpretiamo come UTC.
fn parse_graph_dt(s: &str) -> DateTime<Utc> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return dt.with_timezone(&Utc);
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return DateTime::from_naive_utc_and_offset(naive, Utc);
        }
    }
    Utc::now()
}

/// Estrae un messaggio leggibile da un errore ureq (incluso il corpo HTTP).
fn stringify_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            format!("HTTP {code}: {body}")
        }
        ureq::Error::Transport(t) => t.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_calendar_view() {
        let json = r#"{
          "value": [
            {
              "id": "AAA111",
              "subject": "Daily PAM",
              "start": { "dateTime": "2026-06-20T09:00:00.0000000", "timeZone": "UTC" },
              "end":   { "dateTime": "2026-06-20T09:30:00.0000000", "timeZone": "UTC" },
              "isOnlineMeeting": true,
              "organizer": { "emailAddress": { "name": "Mario Rossi", "address": "m@x.it" } }
            },
            {
              "id": "BBB222",
              "subject": "Review",
              "start": { "dateTime": "2026-06-20T14:00:00.0000000", "timeZone": "UTC" },
              "end":   { "dateTime": "2026-06-20T15:00:00.0000000", "timeZone": "UTC" },
              "isOnlineMeeting": false,
              "organizer": { "emailAddress": { "name": null } }
            }
          ]
        }"#;
        let meetings = parse_calendar_view(json).unwrap();
        assert_eq!(meetings.len(), 2);
        assert_eq!(meetings[0].ext_id, "AAA111");
        assert_eq!(meetings[0].subject, "Daily PAM");
        assert_eq!(meetings[0].duration_seconds, 1800);
        assert!(meetings[0].is_online);
        assert_eq!(meetings[0].organizer.as_deref(), Some("Mario Rossi"));
        assert_eq!(meetings[1].duration_seconds, 3600);
        assert!(!meetings[1].is_online);
        assert!(meetings[1].organizer.is_none());
    }

    #[test]
    fn parsing_presence() {
        let (avail, activity) =
            parse_presence(r#"{"availability":"Busy","activity":"InACall"}"#).unwrap();
        assert_eq!(avail, "Busy");
        assert_eq!(activity, "InACall");
        assert!(is_in_call(&activity));
        assert!(!is_in_call("Available"));
    }
}
