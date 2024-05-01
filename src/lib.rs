pub mod forked_db;
pub mod utils;

use alloy::{primitives::{ Address, Bytes, U256 }, providers::{ ProviderBuilder, RootProvider } };
use alloy::transports::ws::WsConnect;
use alloy::pubsub::PubSubFrontend;

use alloy::signers::wallet::LocalWallet;


use alloy::rpc::types::eth::Block;

use std::sync::Arc;
use std::str::FromStr;

use forked_db::{ *, fork_factory::ForkFactory, fork_db::ForkDB };

use revm::primitives::{
    Bytecode,
    B256,
    AccountInfo,
    TransactTo,
};
use revm::Evm;

use lazy_static::lazy_static;



lazy_static! {
    pub static ref WETH: Address = Address::from_str(
        "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
    ).unwrap();

    pub static ref USDC: Address = Address::from_str(
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
    ).unwrap();
}


/// Represents a dummy account we want to insert into the fork enviroment
pub struct DummyAccount {
    pub account_type: AccountType,

    /// ETH balance to fund with
    pub balance: U256,

    /// WETH balance to fund with
    pub weth_balance: U256,

    pub address: Address
}

impl DummyAccount {
    pub fn new(account_type: AccountType, balance: U256, weth_balance: U256) -> Self {
        Self {
            account_type,
            balance,
            weth_balance,
            address: LocalWallet::random().address()
        }
    }
}

pub enum AccountType {
    /// Externally Owned Account
    EOA,

    /// An Ethereum Smart Contract
    Contract(Bytecode),
}



/// Struct that holds the Parameters used for a call simulation
#[derive(Debug)]
pub struct EvmParams {
    /// The address of the caller who initiates the transaction
    pub caller: Address,

    /// The address to interact with (eg. contract address or wallet address if you are just sending ETH to someone)
    pub transact_to: Address,

    /// The call data to send to `transact_to`
    /// If you are just sending ETH to a wallet `call_data` should be [Bytes::default()]
    pub call_data: Bytes,

    /// The amount of ETH to send with the transaction
    /// Only set a value if you are suppose to send ETH with the transaction
    /// If you are not sending ETH or the function you are calling is `non payable`, set this to [U256::ZERO]
    pub value: U256,

    /// The [Evm] instance to use
    pub evm: Evm<'static, (), ForkDB>,
}

impl EvmParams {
    /// Sets the transaction environment for the [Evm] instance
    pub fn set_tx_env(&mut self) {
        self.evm.tx_mut().caller = self.caller;
        self.evm.tx_mut().transact_to = TransactTo::Call(self.transact_to);
        self.evm.tx_mut().data = self.call_data.clone();
        self.evm.tx_mut().value = self.value;
    }

    /// Sets the `caller` of the transaction
    pub fn set_caller(&mut self, caller: Address) {
        self.caller = caller;
    }

    /// Sets the `transact_to` address
    pub fn set_transact_to(&mut self, transact_to: Address) {
        self.transact_to = transact_to;
    }

    /// Sets the `call_data`
    pub fn set_call_data(&mut self, call_data: Bytes) {
        self.call_data = call_data;
    }

    /// Sets the `value` of the transaction
    pub fn set_value(&mut self, value: U256) {
        self.value = value;
    }

    /// Sets the [Evm] instance
    pub fn set_evm(&mut self, evm: Evm<'static, (), ForkDB>) {
        self.evm = evm;
    }
}




pub async fn get_client() -> Result<Arc<RootProvider<PubSubFrontend>>, anyhow::Error> {
    let url: &str = "wss://eth.merkle.io";
    let client = ProviderBuilder::new().on_ws(WsConnect::new(url)).await?;
    Ok(Arc::new(client))
}

/// Creates a new [Evm] instance with initial state from [ForkDB]
///
/// State changes are applied to [Evm]
pub fn new_evm(fork_db: ForkDB, block: Block) -> Evm<'static, (), ForkDB> {
    let mut evm = Evm::builder().with_db(fork_db).build();

    evm.block_mut().number = U256::from(block.header.number.unwrap());
    evm.block_mut().timestamp = U256::from(block.header.timestamp);
    evm.block_mut().coinbase = Address
        ::from_str("0xDecafC0FFEe15BAD000000000000000000000000")
        .unwrap();

    // Disable some checks for easier testing
    evm.cfg_mut().disable_balance_check = true;
    evm.cfg_mut().disable_block_gas_limit = true;
    evm.cfg_mut().disable_base_fee = true;
    evm
}







/// Inserts a dummy account to the local fork enviroment
pub fn insert_dummy_account(
    account: &DummyAccount,
    fork_factory: &mut ForkFactory
) -> Result<(), anyhow::Error> {

    let code = match &account.account_type {
        AccountType::EOA => Bytecode::default(),
        AccountType::Contract(code) => code.clone(),
    };

    let account_info = AccountInfo {
        balance: account.balance,
        nonce: 0,
        code_hash: B256::default(),
        code: Some(code),
    };

    // insert the account info into the fork enviroment
    fork_factory.insert_account_info(account.address, account_info);


    // To fund any ERC20 token to an account we need the balance storage slot of the token
    // For WETH its 3
    // An amazing online tool to see the storage mapping of any contract https://evm.storage/
    let slot_num = U256::from(3);
    let addr_padded = pad_left(account.address.to_vec(), 32);
    let slot = slot_num.to_be_bytes_vec();
    
    let data = [&addr_padded, &slot].iter().flat_map(|x| x.iter().copied()).collect::<Vec<u8>>();
    let slot_hash = keccak256(&data);
    let slot: U256 = U256::from_be_bytes(slot_hash.try_into().expect("Hash must be 32 bytes"));
    

    // insert the erc20 token balance to the dummy account
    if let Err(e) = fork_factory.insert_account_storage(*WETH, slot, account.weth_balance) {
        return Err(anyhow::anyhow!("Failed to insert account storage: {}", e));
    }

    Ok(())
}

fn pad_left(vec: Vec<u8>, full_len: usize) -> Vec<u8> {
    let mut padded = vec![0u8; full_len - vec.len()];
    padded.extend(vec);
    padded
}