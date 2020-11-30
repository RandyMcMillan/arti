//! Types to describe information about other downloaded directory
//! documents, without necessarily having the full document.

use tor_llcrypto as ll;
use tor_netdoc::doc::netstatus::{Lifetime, MDConsensus, UnvalidatedMDConsensus};

use digest::Digest;

/// Information about a consensus that we have in storage.
///
/// This information is ordinarily derived from the consensus, but doesn't
/// have to be.
#[derive(Debug, Clone)]
pub struct ConsensusMeta {
    /// The time over which the consensus is valid.
    lifetime: Lifetime,
    /// A sha3-256 digest of the signed portion of the consensus: used for
    /// fetching diffs.
    sha3_256_of_signed: [u8; 32],
    /// A sha3-256 digest of the entirety of the consensus: used for
    /// naming the file.
    sha3_256_of_whole: [u8; 32],
}

impl ConsensusMeta {
    /// Create a new ConsensusMeta
    pub fn new(
        lifetime: Lifetime,
        sha3_256_of_signed: [u8; 32],
        sha3_256_of_whole: [u8; 32],
    ) -> Self {
        ConsensusMeta {
            lifetime,
            sha3_256_of_signed,
            sha3_256_of_whole,
        }
    }
    /// Derive a new ConsensusMeta from an UnvalidatedMDConsensus and the
    /// text of its signed portino.
    pub fn from_unvalidated(
        signed_part: &str,
        remainder: &str,
        con: &UnvalidatedMDConsensus,
    ) -> Self {
        let lifetime = con.peek_lifetime().clone();
        let (sd, wd) = sha3_dual(signed_part, remainder);
        ConsensusMeta::new(lifetime, sd, wd)
    }
    /// Derive a new ConsensusMeta from a MDConsensus and the text of its
    /// signed portion.
    #[allow(unused)]
    pub fn from_consensus(signed_part: &str, remainder: &str, con: &MDConsensus) -> Self {
        let lifetime = con.lifetime().clone();
        let (sd, wd) = sha3_dual(signed_part, remainder);
        ConsensusMeta::new(lifetime, sd, wd)
    }
    /// Return the lifetime of this ConsensusMeta
    pub fn lifetime(&self) -> &Lifetime {
        &self.lifetime
    }
    /// Return the sha3-256 of the signed portion of this consensus.
    pub fn sha3_256_of_signed(&self) -> &[u8; 32] {
        &self.sha3_256_of_signed
    }
    /// Return the sha3-256 of the entirety of this consensus.
    pub fn sha3_256_of_whole(&self) -> &[u8; 32] {
        &self.sha3_256_of_whole
    }
}

/// Compute the sha3-256 digests of signed_part on its own, and of
/// signed_part concatenated with remainder.
fn sha3_dual(signed_part: impl AsRef<[u8]>, remainder: impl AsRef<[u8]>) -> ([u8; 32], [u8; 32]) {
    let mut d = ll::d::Sha3_256::new();
    d.update(signed_part.as_ref());
    let sha3_of_signed = d.clone().finalize().into();
    d.update(remainder.as_ref());
    let sha3_of_whole = d.finalize().into();
    (sha3_of_signed, sha3_of_whole)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn t_sha3_dual() {
        let s = b"Loarax ipsum gruvvulus thneed amet, snergelly once-ler lerkim, sed do barbaloot tempor gluppitus ut labore et truffula magna aliqua. Ut enim ad grickle-grass veniam, quis miff-muffered ga-zumpco laboris nisi ut cruffulus ex ea schloppity consequat. Duis aute snarggle in swomeeswans in voluptate axe-hacker esse rippulus crummii eu moof nulla snuvv.";

        let sha3_of_whole: [u8; 32] = ll::d::Sha3_256::digest(s).into();

        for idx in 0..s.len() {
            let sha3_of_part: [u8; 32] = ll::d::Sha3_256::digest(&s[..idx]).into();
            let (a, b) = sha3_dual(&s[..idx], &s[idx..]);
            assert_eq!(a, sha3_of_part);
            assert_eq!(b, sha3_of_whole);
        }
    }
}
