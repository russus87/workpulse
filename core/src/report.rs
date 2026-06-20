//! Utility per l'export CSV (timesheet, riepiloghi).
//!
//! Escaping conforme a RFC 4180: i campi che contengono virgola, virgolette o
//! a-capo vengono racchiusi tra virgolette doppie, raddoppiando quelle interne.

/// Racchiude/escapa un singolo campo CSV se necessario.
pub fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Compone una riga CSV (campi gia' grezzi, l'escaping lo fa questa funzione).
pub fn csv_line(fields: &[String]) -> String {
    fields
        .iter()
        .map(|f| csv_field(f))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_campi() {
        assert_eq!(csv_field("PAM"), "PAM");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("dice \"ciao\""), "\"dice \"\"ciao\"\"\"");
    }

    #[test]
    fn riga_csv() {
        let line = csv_line(&["2026-06-20".into(), "PAM, CRM".into(), "3h".into()]);
        assert_eq!(line, "2026-06-20,\"PAM, CRM\",3h");
    }
}
