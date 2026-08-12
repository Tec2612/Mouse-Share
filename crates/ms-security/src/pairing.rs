use sha2::{Digest, Sha256};

/// Derives a 6-digit short authentication string (SAS) from TLS 1.3
/// exporter keying material and both peers' observed certificate
/// fingerprints, in the spirit of ZRTP/Bluetooth "numbers match" pairing.
///
/// Security property: the exporter keying material is derived from the
/// (EC)DHE shared secret negotiated *for this specific connection*, which
/// an active man-in-the-middle cannot reproduce without being a party to
/// both legs of the handshake — and if they are, the two legs use
/// different key material, so the SAS each victim computes will differ. A
/// human confirming the two on-screen codes match is therefore actually
/// verifying there is no MITM, not just re-typing a number the app made
/// up. This is why pairing does not rely on the PIN/SAS alone for
/// long-term authentication: it is single-use, to bootstrap the
/// fingerprint pinning that `PinnedVerifier` enforces afterward.
///
/// Fingerprints are sorted before hashing so both participants — who each
/// see themselves as "local" and the other as "remote" — compute the
/// identical digest regardless of TLS client/server role.
pub fn derive_sas(exporter_keying_material: &[u8], fingerprint_a: &str, fingerprint_b: &str) -> String {
    let (first, second) = if fingerprint_a <= fingerprint_b {
        (fingerprint_a, fingerprint_b)
    } else {
        (fingerprint_b, fingerprint_a)
    };

    let mut hasher = Sha256::new();
    hasher.update(exporter_keying_material);
    hasher.update(first.as_bytes());
    hasher.update(second.as_bytes());
    let digest = hasher.finalize();

    // Take the first 4 bytes as a u32 and reduce mod 1_000_000 for a
    // human-typeable 6-digit code, matching common SAS/OTP conventions.
    let n = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:06}", n % 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_symmetric_regardless_of_fingerprint_argument_order() {
        let a = derive_sas(b"shared-secret", "AA:BB", "CC:DD");
        let b = derive_sas(b"shared-secret", "CC:DD", "AA:BB");
        assert_eq!(a, b);
    }

    #[test]
    fn different_exporter_material_yields_different_codes() {
        // Models the MITM-detection property: two "legs" of a
        // man-in-the-middled connection get independent DHE secrets and
        // thus independent exporter material, so victims see mismatched
        // codes even though the reported fingerprints could be made to
        // look plausible.
        let a = derive_sas(b"leg-one-secret", "AA:BB", "CC:DD");
        let b = derive_sas(b"leg-two-secret", "AA:BB", "CC:DD");
        assert_ne!(a, b);
    }

    #[test]
    fn output_is_always_six_digits() {
        let code = derive_sas(b"x", "AA", "BB");
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
