use sha2::{Digest, Sha256};

#[must_use]
pub fn hash_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[must_use]
pub fn generate_token() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let a = hash_token("test-token");
        let b = hash_token("test-token");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn generate_is_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
    }
}
