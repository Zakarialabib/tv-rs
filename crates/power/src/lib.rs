//! Skeleton crate (Phase 1). Auth/crypto insertion points = unimplemented!().

pub trait AuthProvider {
    fn authenticate(&self) -> Result<String, String> {
        unimplemented!("Phase 2 REA: extract token-gen math / SSL bypass / AES-GCM key derivation")
    }
}

pub trait TvController {
    fn discover(&self) -> Result<Vec<String>, String> { unimplemented!() }
}
