pub mod database_error;

pub mod global_backend;
pub use global_backend::*;

pub mod fork_db;
pub mod fork_factory;

use revm::primitives::{ExecutionResult, Output, Bytes};
use anyhow::anyhow;










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



// ** Convert Types from Ethers To Revm Primitive Types **

// Converts from Ethers U256 to revm::primitives::U256
pub fn to_revm_u256(u: ethers::types::U256) -> revm::primitives::U256 {
    let mut bytes = [0u8; 32];
    u.to_little_endian(&mut bytes);
    revm::primitives::U256::from_le_bytes(bytes)
}

// Converts from revm primitive U256 to ethers U256
pub fn to_ethers_u256(u: revm::primitives::U256) -> ethers::types::U256 {
    let bytes: [u8; 32] = u.to_be_bytes(); // Explicitly specifying the size of the byte array
    ethers::types::U256::from_big_endian(&bytes)
}

// converts from revm primitive Address to ethers Address
pub fn to_ethers_address(address: revm::primitives::Address) -> ethers::types::Address {
    let bytes: [u8; 20] = address.0.into();
    ethers::types::H160::from(bytes)
}

// converts from ethers U256 to ethers H256
pub fn h256_from_u256(u: ethers::types::U256) -> ethers::types::H256 {
    let mut bytes = [0u8; 32];
    u.to_little_endian(&mut bytes);
    ethers::types::H256::from_slice(&bytes)
}

// converts from Ethers Address to revm primitive Address
pub fn to_revm_address(address: ethers::types::Address) -> revm::primitives::Address {
    let bytes: [u8; 20] = address.0;
    revm::primitives::Address::from(bytes)
}

// converts from revm B256 hash to revm Address
pub fn addr_from_b256(hash: revm::primitives::B256) -> revm::primitives::Address {
    let bytes = hash.0; // Get the inner 32-byte array
    let addr_bytes = &bytes[12..]; // Take the last 20 bytes
    revm::primitives::Address::from_slice(addr_bytes)
}