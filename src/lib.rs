pub mod forked_db;

use ethers::{prelude::*, utils::{parse_ether, keccak256}, abi::parse_abi};
use std::sync::Arc;
use std::str::FromStr;
use forked_db::{*, fork_factory::ForkFactory, fork_db::ForkDB};

use revm::primitives::{Bytecode, Bytes as rBytes, U256 as rU256, B256, AccountInfo, TransactTo, Log};
use revm::Evm;
use bigdecimal::BigDecimal;
use lazy_static::lazy_static;


lazy_static!{
    pub static ref WETH: Address = Address::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap();
    pub static ref USDT: Address = Address::from_str("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap();
    pub static ref USDC: Address = Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap();
}

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub is_reverted: bool,
    pub logs: Vec<Log>,
    pub gas_used: u64,
    pub output: rBytes,
}


#[derive(Debug, Clone)]
pub struct Pool {
    pub address: Address,
    pub token0: Address,
    pub token1: Address,
    pub variant: PoolVariant,
}

impl Pool {
    pub fn variant(&self) -> U256 {
        match self.variant {
            PoolVariant::UniswapV2 => U256::zero(),
            PoolVariant::UniswapV3 => U256::one(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PoolVariant {
    UniswapV2,
    UniswapV3
}




pub async fn get_client() -> Result<Arc<Provider<Ws>>, anyhow::Error> {
    let url: &str = "wss://eth.merkle.io";
    let client = Provider::<Ws>::connect(url).await?;
    Ok(Arc::new(client))
}



/// Creates a new [Evm] instance with initial state from [ForkDB]
/// State changes are applied to [Evm]
pub fn new_evm(fork_db: ForkDB) -> Evm<'static, (), ForkDB> {
    let mut evm = Evm::builder().with_db(fork_db).build();

    // Disable some checks for easier testing
    evm.cfg_mut().disable_balance_check = true;
    evm.cfg_mut().disable_block_gas_limit = true;
    evm.cfg_mut().disable_base_fee = true;
    evm
}



/// Simulates a call without any inspectors
/// Returns [SimulationResult]
pub fn sim_call(
    caller: Address,
    transact_to: Address,
    call_data: Bytes,
    value: U256,
    apply_changes: bool,
    evm: &mut Evm<'static, (), ForkDB>
) -> Result<SimulationResult, anyhow::Error> {
    evm.tx_mut().caller = caller.0.into();
    evm.tx_mut().transact_to = TransactTo::Call(transact_to.0.into());
    evm.tx_mut().data = rBytes::from(call_data.0);
    evm.tx_mut().value = to_revm_u256(value);


   let result = if apply_changes {
        evm.transact_commit()?
    } else {
        evm.transact()?.result
    };

    let is_reverted = match_output_reverted(&result);
    let logs = result.logs();
    let gas_used = result.gas_used();
    let output = result.into_output().unwrap_or_default();

    let sim_result = SimulationResult {
        is_reverted,
        logs,
        gas_used,
        output,
    };

    Ok(sim_result)
}


pub fn encode_swap(
    input_token: Address,
    output_token: Address,
    amount_in: U256,
    pool_address: Address,
    pool_variant: U256,
    minimum_received: U256
) -> Vec<u8> {
    let method_id = &keccak256(b"swap(address,address,uint256,address,uint256,uint256)")[0..4];

    let encoded_args = ethabi::encode(
        &[
            ethabi::Token::Address(input_token),
            ethabi::Token::Address(output_token),
            ethabi::Token::Uint(amount_in),
            ethabi::Token::Address(pool_address),
            ethabi::Token::Uint(pool_variant),
            ethabi::Token::Uint(minimum_received),
        ]
    );

    let mut payload = vec![];
    payload.extend_from_slice(method_id);
    payload.extend_from_slice(&encoded_args);

    payload
}

pub fn encode_recover_erc20(
    token: Address,
    amount: U256
) -> Vec<u8> {
    let method_id = &keccak256(b"recover_erc20(address,uint256)")[0..4];
    
    let encoded_args = ethabi::encode(
        &[
            ethabi::Token::Address(token),
            ethabi::Token::Uint(amount),
        ]
    );

    let mut payload = vec![];
    payload.extend_from_slice(method_id);
    payload.extend_from_slice(&encoded_args);

    payload
}


pub fn encode_transfer(
    recipient: Address,
    amount: U256,
) -> Vec<u8> {
    let method_id = &keccak256(b"transfer(address,uint256)")[0..4];
    
    let encoded_args = ethabi::encode(
        &[
            ethabi::Token::Address(recipient),
            ethabi::Token::Uint(amount),
        ]
    );

    let mut payload = vec![];
    payload.extend_from_slice(method_id);
    payload.extend_from_slice(&encoded_args);

    payload
}


/// Inserts a dummy EOA account to the fork factory
pub fn insert_dummy_account(fork_factory: &mut ForkFactory) -> Result<(), anyhow::Error> {

    // you can use whatever address you want for the dummy account as long as its a valid ethereum address and ideally not in use (Doesn't have a state)
    // you could use an online tool like: https://vanity-eth.tk/ to generate a random address
    let dummy_address = Address::from_str("0x0093562c7e4BcC8e4D256A27e08C9ae6Ac4F895c")?;

    // create a new account info
    // We also set 1 ETH in balance
    let account_info = AccountInfo {
        balance: rU256::from(1000000000000000000u128),
        nonce: 0,
        code_hash: B256::default(),
        code: None, // None because its not a contract
    };

    // insert the account info into the fork factory
    fork_factory.insert_account_info(dummy_address.0.into(), account_info);

    // Now we fund the dummy account with 1 WETH
    let weth_amount = parse_ether(1).unwrap();
    let weth_address = Address::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2")?;

    // To fund any ERC20 token to an account we need the balance storage slot of the token
    // For WETH its 3
    // An amazing online tool to see the storage mapping of any contract https://evm.storage/
    let weth_slot: U256 = keccak256(abi::encode(&[
        abi::Token::Address(dummy_address.0.into()),
        abi::Token::Uint(U256::from(3)),
    ])).into();

    // insert the erc20 token balance to the dummy account
    if let Err(e) = fork_factory.insert_account_storage(
        weth_address.0.into(),
        to_revm_u256(weth_slot),
        to_revm_u256(weth_amount),
    ) {
        return Err(anyhow::anyhow!("Failed to insert account storage: {}", e));
    }

    Ok(())
}

pub fn insert_dummy_contract_account(fork_factory: &mut ForkFactory) -> Result<(), anyhow::Error> {

    let dummy_address = Address::from_str("0x0093562c7e4BcC8e4D256A27e08C9ae6Ac4F895c")?;

    let account_info = AccountInfo {
        balance: rU256::ZERO,
        nonce: 0,
        code_hash: B256::default(),
        code: Some(Bytecode::new_raw(rBytes::from(get_bytecode().0)))
    };

    fork_factory.insert_account_info(dummy_address.0.into(), account_info);

    let weth_amount = parse_ether(10).unwrap();
    let weth_address = Address::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2")?;


    let weth_slot: U256 = keccak256(abi::encode(&[
        abi::Token::Address(dummy_address.0.into()),
        abi::Token::Uint(U256::from(3)),
    ])).into();

    if let Err(e) = fork_factory.insert_account_storage(
        weth_address.0.into(),
        to_revm_u256(weth_slot),
        to_revm_u256(weth_amount),
    ) {
        return Err(anyhow::anyhow!("Failed to insert account storage: {}", e));
    }

    Ok(())
}

pub fn to_readable(amount: U256, token: Address) -> String {
    let decimals = match_decimals(token);
    let divisor_str = format!("1{:0>width$}", "", width = decimals as usize);
    let divisor = BigDecimal::from_str(&divisor_str).unwrap();
    let amount_as_decimal = BigDecimal::from_str(&amount.to_string()).unwrap();
    let amount = amount_as_decimal / divisor;
    let token = match token {
        t if t == *WETH => "WETH",
        t if t == *USDT => "USDT",
        t if t == *USDC => "USDC",
        _ => "Token"
    };
    format!("{:.4} {}", amount, token)
}

pub fn match_decimals(token: Address) -> u32 {
    match token {
       t if t == *WETH => 18,
       t if t == *USDT => 6,
       t if t == *USDC => 6,
        _ => 18
    }
}

// Deployed Bytecode of router contract
fn get_bytecode() -> Bytes {
    "0x60806040526004361015610018575b361561001657005b005b6000803560e01c806309f3d6a014610abd57806323a50d3c1461099d5780633def684d1461093f5780637129b640146101b75763fa461e331461005b575061000e565b346101b45760603660031901126101b45760443560043560243567ffffffffffffffff8084116101ac57366023850112156101ac5783600401359081116101ac5783013660248201116101ac578360609103126101b057602483013580151581036101ac576100d860646100d160448701610c2d565b9501610c2d565b6001600160a01b039485169416330361016857848313801561015f575b1561011b571561010e575061010b913390610c41565b80f35b905061010b913390610c41565b606460405162461bcd60e51b815260206004820152600c60248201527f4e6f204c697175696469747900000000000000000000000000000000000000006044820152fd5b508482136100f5565b606460405162461bcd60e51b815260206004820152600c60248201527f4e6f742074686520706f6f6c00000000000000000000000000000000000000006044820152fd5b8480fd5b8380fd5b80fd5b50346101b45760c03660031901126101b4576101d1610b7a565b6024356001600160a01b03811681036104f9576001600160a01b0360643516606435036104f957610216725189a5c1dc9c75acee8d38321d5d6f8e54b1b63314610b90565b604051916370a0823160e01b83523060048401526020836024816001600160a01b0386165afa92831561089e57849361090b575b50608435610741576001600160a01b031661026a60443560643583610c41565b6040517f0902f1ac0000000000000000000000000000000000000000000000000000000081526060816004816001600160a01b03606435165afa90811561073657859086926106da575b506dffffffffffffffffffffffffffff91821691166001600160a01b0384168310156106d557905b6040516370a0823160e01b81526001600160a01b03606435166004820152602081602481875afa80156106ca5783908890610694575b61031c9250610d05565b801561062a578215801580610621575b156105b7576103e58083029280840482036105a35784020292828404148215171561058d576103e88085029485041417156105795782018092116105655781156105515704906001600160a01b038316111561054a57835b60405190602082019282841067ffffffffffffffff8511176105345786936040528383526001600160a01b03606435163b156101b057839161040b60405194859384937f022c0d9f00000000000000000000000000000000000000000000000000000000855260048501526024840152306044840152608060648401526084830190610d12565b0381836001600160a01b03606435165af1801561052957610511575b505060206001600160a01b03916024604051809481936370a0823160e01b8352306004840152165afa9081156105065783916104cf575b50915b5080156104c95761047191610d05565b60a435811061048557602090604051908152f35b606460405162461bcd60e51b815260206004820152601e60248201527f5265616c20416d6f756e74203c204d696e696d756d20526563656976656400006044820152fd5b50610471565b90506020813d6020116104fe575b816104ea60209383610c0b565b810103126104f957513861045e565b600080fd5b3d91506104dd565b6040513d85823e3d90fd5b61051a90610bdb565b610525578238610427565b8280fd5b6040513d84823e3d90fd5b634e487b7160e01b600052604160045260246000fd5b8390610384565b602486634e487b7160e01b81526012600452fd5b602486634e487b7160e01b81526011600452fd5b602487634e487b7160e01b81526011600452fd5b634e487b7160e01b600052601160045260246000fd5b60248a634e487b7160e01b81526011600452fd5b608460405162461bcd60e51b815260206004820152602860248201527f556e697377617056324c6962726172793a20494e53554646494349454e545f4c60448201527f49515549444954590000000000000000000000000000000000000000000000006064820152fd5b5082151561032c565b608460405162461bcd60e51b815260206004820152602b60248201527f556e697377617056324c6962726172793a20494e53554646494349454e545f4960448201527f4e5055545f414d4f554e540000000000000000000000000000000000000000006064820152fd5b50506020813d6020116106c2575b816106af60209383610c0b565b810103126104f9578261031c9151610312565b3d91506106a2565b6040513d89823e3d90fd5b6102dc565b9150506060813d60601161072e575b816106f660609383610c0b565b810103126101ac5761070781610d52565b604061071560208401610d52565b92015163ffffffff81160361072a57386102b4565b8580fd5b3d91506106e9565b6040513d87823e3d90fd5b6084356001036108c75760406001600160a01b038092168284168110806000146108a9576107e86401000276ad925b84519083602083015285820152856064351660608201526060815261079481610bef565b845195869485947f128acb080000000000000000000000000000000000000000000000000000000086523060048701526024860152604435604486015216606484015260a0608484015260a4830190610d12565b0381876001600160a01b03606435165af1801561089e57610873575b5060206001600160a01b03916024604051809481936370a0823160e01b8352306004840152165afa908115610506578391610841575b5091610461565b90506020813d60201161086b575b8161085c60209383610c0b565b810103126104f957513861083a565b3d915061084f565b604090813d8311610897575b6108898183610c0b565b810103126105255738610804565b503d61087f565b6040513d86823e3d90fd5b6107e873fffd8963efd1fc6a506488495d951d5263988d2592610770565b606460405162461bcd60e51b815260206004820152601460248201527f496e76616c696420706f6f6c2076617269616e740000000000000000000000006044820152fd5b9092506020813d602011610937575b8161092760209383610c0b565b810103126104f95751913861024a565b3d915061091a565b50346101b45760203660031901126101b45780808080600435725189a5c1dc9c75acee8d38321d5d6f8e54b1b6610977813314610b90565b828215610994575bf1156109885780f35b604051903d90823e3d90fd5b506108fc61097f565b50346101b45760403660031901126101b4576109b7610b7a565b610a496000806001600160a01b03725189a5c1dc9c75acee8d38321d5d6f8e54b1b6946109e5863314610b90565b1693604051602081019163a9059cbb60e01b83526024820152602435604482015260448152610a1381610bef565b519082865af13d15610ab5573d90610a2a82610d6d565b91610a386040519384610c0b565b82523d6000602084013e5b83610d89565b8051908115159182610a91575b5050610a60575080f35b602490604051907f5274afe70000000000000000000000000000000000000000000000000000000082526004820152fd5b81925090602091810103126104f957602001518015908115036104f9573880610a56565b606090610a43565b50346101b45760203660031901126101b45760043590725189a5c1dc9c75acee8d38321d5d6f8e54b1b6610af2813314610b90565b73c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2803b15610525578280916024604051809481937f2e1a7d4d0000000000000000000000000000000000000000000000000000000083528960048401525af1801561050657610b67575b5081808094819382821561099457f1156109885780f35b610b7390929192610bdb565b9038610b50565b600435906001600160a01b03821682036104f957565b15610b9757565b606460405162461bcd60e51b815260206004820152600d60248201527f4e6f7420535741505f55534552000000000000000000000000000000000000006044820152fd5b67ffffffffffffffff811161053457604052565b6080810190811067ffffffffffffffff82111761053457604052565b90601f8019910116810190811067ffffffffffffffff82111761053457604052565b35906001600160a01b03821682036104f957565b9190610cb99060405193602085019363a9059cbb60e01b85526001600160a01b038093166024870152604486015260448552610c7c85610bef565b1692600080938192519082875af13d15610cfd573d90610c9b82610d6d565b91610ca96040519384610c0b565b82523d84602084013e5b84610d89565b908151918215159283610cd1575b505050610a605750565b819293509060209181010312610cf95760200151908115918215036101b45750388080610cc7565b5080fd5b606090610cb3565b9190820391821161058d57565b919082519283825260005b848110610d3e575050826000602080949584010152601f8019910116010190565b602081830181015184830182015201610d1d565b51906dffffffffffffffffffffffffffff821682036104f957565b67ffffffffffffffff811161053457601f01601f191660200190565b90610dc85750805115610d9e57805190602001fd5b60046040517f1425ea42000000000000000000000000000000000000000000000000000000008152fd5b81511580610e13575b610dd9575090565b6024906001600160a01b03604051917f9996b315000000000000000000000000000000000000000000000000000000008352166004820152fd5b50803b15610dd156fea2646970667358221220d913ab863f6b43774894f0bb08a4b655bc60b8047e0b0597c6befd4ced5aee2864736f6c63430008170033"
    .parse()
    .unwrap()
}


// ** ABI getters
pub fn weth_deposit() -> BaseContract {
    BaseContract::from(parse_abi(
        &["function deposit() public payable"]
    ).unwrap())
}

pub fn erc20_balanceof() -> BaseContract {
    BaseContract::from(parse_abi(
        &["function balanceOf(address) public view returns (uint256)"]
    ).unwrap())
}

pub fn swap() -> BaseContract {
    BaseContract::from(parse_abi(
        &["function swap(address,address,uint256,address,uint256,uint256) external returns (uint256)"]
    ).unwrap())
}