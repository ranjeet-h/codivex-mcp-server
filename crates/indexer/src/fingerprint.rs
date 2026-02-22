use sha2::{Digest, Sha256};

pub fn fingerprint_content(content: &str) -> String {
    let normalized = content
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::fingerprint_content;

    #[test]
    fn fingerprint_ignores_spacing_noise() {
        let a = "fn x() {\n  1 + 1\n}\n";
        let b = "fn   x(){ 1 + 1 }";
        assert_eq!(fingerprint_content(a), fingerprint_content(b));
    }
}
