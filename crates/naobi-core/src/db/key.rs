//! Keyed item hash for the FRECENCY table (D-014): xxh3, never `DefaultHasher`.
use xxhash_rust::xxh3::xxh3_64_with_seed;

/// 64-bit xxh3 of `identity`, seeded with the first 8 salt bytes (little endian).
pub fn item_key(salt: &[u8; 16], identity: &str) -> i64 {
    let mut seed = [0u8; 8];
    seed.copy_from_slice(&salt[..8]);
    xxh3_64_with_seed(identity.as_bytes(), u64::from_le_bytes(seed)) as i64
}
