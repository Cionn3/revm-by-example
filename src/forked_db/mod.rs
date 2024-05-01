pub mod database_error;

pub mod global_backend;
pub use global_backend::*;

pub mod fork_db;
pub mod fork_factory;


use tiny_keccak::{Keccak, Hasher};






pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    let mut result = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut result);
    result
}

// converts from revm B256 hash to revm Address
pub fn addr_from_b256(hash: revm::primitives::B256) -> revm::primitives::Address {
    let bytes = hash.0; // Get the inner 32-byte array
    let addr_bytes = &bytes[12..]; // Take the last 20 bytes
    revm::primitives::Address::from_slice(addr_bytes)
}