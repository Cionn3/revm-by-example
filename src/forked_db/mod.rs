pub mod database_error;

pub mod global_backend;
pub use global_backend::*;

pub mod fork_db;
pub mod fork_factory;

use revm::primitives::{ExecutionResult, Output, Bytes};
use anyhow::anyhow;
use tiny_keccak::{Keccak, Hasher};









// matches execution result, returns the output
pub fn match_output(result: ExecutionResult) -> Result<Bytes, anyhow::Error> {
    match result {
        ExecutionResult::Success { output, .. } =>
            match output {
                Output::Call(o) => Ok(o),
                Output::Create(o, _) => Ok(o),
            }
        ExecutionResult::Revert { output, gas_used } => {
            Err(anyhow!("Call Reverted: {:?} Gas Used {}", bytes_to_string(output), gas_used))
        }
        ExecutionResult::Halt { reason,.. } => {
            Err(anyhow!("Halt Reason: {:?}", reason))
        }
    }
}

/// matches execution result, returns a bool indicating if the call was reverted
pub fn match_output_reverted(result: &ExecutionResult) -> bool {
    match result {
         ExecutionResult::Success { .. } => false,
         ExecutionResult::Revert { .. } => {
             true
         }
         ExecutionResult::Halt { .. } => true,
     }
 }


 pub fn bytes_to_string(bytes: revm::primitives::Bytes) -> String {
    if bytes.len() < 4 {
        return "EVM Returned 0x (Empty Bytes)".to_string();
    }
    let error_data = &bytes[4..];

    match String::from_utf8(error_data.to_vec()) {
        Ok(s) => s.trim_matches(char::from(0)).to_string(),
        Err(_) => "EVM Returned 0x (Empty Bytes)".to_string(),
    }
}



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