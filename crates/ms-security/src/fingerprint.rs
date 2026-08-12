use sha2::{Digest, Sha256};

/// Hex-encoded SHA-256 digest of a certificate's DER bytes, colon-separated
/// in groups of 2 for human comparison during pairing (e.g.
/// `AB:CD:EF:...`), matching the conventional display format for TLS/SSH
/// fingerprints.
pub fn sha256_hex(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    digest.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_always_produces_the_same_fingerprint() {
        let data = b"certificate-bytes";
        assert_eq!(sha256_hex(data), sha256_hex(data));
    }

    #[test]
    fn fingerprint_is_colon_separated_hex() {
        let fp = sha256_hex(b"x");
        assert_eq!(fp.len(), 32 * 3 - 1); // 32 bytes -> "XX:" * 31 + "XX"
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit() || c == ':'));
    }
}
