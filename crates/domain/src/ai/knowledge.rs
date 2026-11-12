//! Basis pengetahuan (P5-7, `PRD.md §8.2`): validasi dokumen & chunking deterministik.

pub const TITLE_MAX: usize = 200;
pub const BODY_MAX: usize = 50_000;
pub const CHUNK_MAX: usize = 1_000;
pub const CHUNK_OVERLAP: usize = 150;
/// Batas per tenant (sementara sampai D-10 plan limits).
pub const MAX_DOCUMENTS: i64 = 100;
pub const MAX_CHUNKS: i64 = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KnowledgeError {
    #[error("judul tidak valid")]
    Title,
    #[error("isi tidak valid")]
    Body,
}

pub fn validate(title: &str, body: &str) -> Result<(), KnowledgeError> {
    let t = title.trim();
    if t.is_empty() || t.chars().count() > TITLE_MAX || t.chars().any(char::is_control) {
        return Err(KnowledgeError::Title);
    }
    let b = body.trim();
    if b.is_empty() || b.chars().count() > BODY_MAX {
        return Err(KnowledgeError::Body);
    }
    Ok(())
}

/// Potong per paragraf → kalimat → karakter, lalu gabungkan hingga `max` dengan overlap ekor.
/// Setiap chunk diawali judul agar konteks tetap bermakna saat diambil sendiri.
pub fn chunk(title: &str, body: &str, max: usize, overlap: usize) -> Vec<String> {
    let head = format!("{}\n", title.trim());
    let budget = max.saturating_sub(head.chars().count()).max(100);
    let mut pieces: Vec<String> = vec![];
    for para in body.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        if para.chars().count() <= budget {
            pieces.push(para.to_owned());
            continue;
        }
        let mut sentence = String::new();
        for ch in para.chars() {
            sentence.push(ch);
            let end = matches!(ch, '.' | '!' | '?' | '\n') || sentence.chars().count() >= budget;
            if end {
                pieces.push(sentence.trim().to_owned());
                sentence.clear();
            }
        }
        if !sentence.trim().is_empty() {
            pieces.push(sentence.trim().to_owned());
        }
    }
    let mut out: Vec<String> = vec![];
    let mut cur = String::new();
    for p in pieces.into_iter().filter(|p| !p.is_empty()) {
        let would = cur.chars().count() + p.chars().count() + 1;
        if !cur.is_empty() && would > budget {
            out.push(cur.clone());
            let tail: String = cur
                .chars()
                .rev()
                .take(overlap)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            cur = tail;
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&p);
        if cur.chars().count() > budget {
            cur = cur.chars().take(budget).collect();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out.into_iter()
        .map(|c| format!("{head}{}", c.trim()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert!(validate("FAQ Ongkir", "Gratis ongkir di atas 100rb.").is_ok());
        assert_eq!(validate(" ", "x"), Err(KnowledgeError::Title));
        assert_eq!(validate("t", " "), Err(KnowledgeError::Body));
        assert_eq!(
            validate("t", &"a".repeat(50_001)),
            Err(KnowledgeError::Body)
        );
    }

    #[test]
    fn chunks_are_bounded_ordered_and_titled() {
        let body = (0..60)
            .map(|i| format!("Kalimat nomor {i} tentang kebijakan retur toko."))
            .collect::<Vec<_>>()
            .join(" ");
        let body = format!("Pembuka singkat.\n\n{body}\n\n{}", "x".repeat(3000));
        let c = chunk("Kebijakan Retur", &body, CHUNK_MAX, CHUNK_OVERLAP);
        assert!(c.len() > 3);
        assert!(
            c.iter()
                .all(|x| x.starts_with("Kebijakan Retur\n") && x.chars().count() <= CHUNK_MAX),
            "{c:?}"
        );
        assert!(c[0].contains("Pembuka singkat."));
        // Deterministik & tidak ada teks hilang di awal kalimat yang dipotong.
        assert_eq!(c, chunk("Kebijakan Retur", &body, CHUNK_MAX, CHUNK_OVERLAP));
        assert!(c.iter().any(|x| x.contains("Kalimat nomor 59")));
        assert_eq!(
            chunk("T", "Pendek.", CHUNK_MAX, CHUNK_OVERLAP),
            vec!["T\nPendek.".to_owned()]
        );
    }
}
