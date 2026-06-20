//! Insight con LLM **locale** (es. Ollama): generazione di riepiloghi/analisi
//! piu' ricchi senza che i dati lascino il dispositivo.
//!
//! Si parla con un endpoint locale stile Ollama (`/api/generate`). Il prompt e'
//! costruito a partire da dati gia' aggregati. Se l'LLM non risponde, il chiamante
//! ricade sul riepilogo a template. Parsing della risposta testato a parte.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GenerateResp {
    response: String,
}

/// Estrae il testo generato dalla risposta JSON di Ollama (`/api/generate`).
pub fn parse_generate(json: &str) -> Result<String, String> {
    let r: GenerateResp = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(r.response.trim().to_string())
}

/// Chiede all'LLM locale di elaborare `prompt`. `endpoint` e' la base (es.
/// `http://localhost:11434`), `model` il nome del modello.
pub fn generate(endpoint: &str, model: &str, prompt: &str) -> Result<String, String> {
    let url = format!("{}/api/generate", endpoint.trim_end_matches('/'));
    let body = ureq::post(&url)
        .send_json(ureq::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
        }))
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                format!("HTTP {code}: {}", resp.into_string().unwrap_or_default())
            }
            ureq::Error::Transport(t) => format!("LLM non raggiungibile: {t}"),
        })?
        .into_string()
        .map_err(|e| e.to_string())?;
    parse_generate(&body)
}

/// Costruisce il prompt di insight settimanale da una base testuale di dati.
pub fn weekly_prompt(data_summary: &str) -> String {
    format!(
        "Sei un assistente di produttivita'. In italiano, in massimo 5 frasi, \
         analizza questi dati di lavoro della settimana e dai 1-2 suggerimenti \
         concreti. Non inventare numeri non presenti.\n\nDATI:\n{data_summary}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_risposta_ollama() {
        let json = r#"{"model":"llama3.2","response":"  Hai lavorato bene.  ","done":true}"#;
        assert_eq!(parse_generate(json).unwrap(), "Hai lavorato bene.");
    }
}
