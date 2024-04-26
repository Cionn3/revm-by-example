use alloy::providers::Provider;
use alloy::rpc::types::eth::{BlockId, BlockNumberOrTag};
use alloy::primitives::{Address, U256};use std::str::FromStr;
use revm_by_example::{ forked_db::fork_factory::ForkFactory, *, forked_db::bytes_to_string, parse_ether };

use revm::db::{CacheDB, EmptyDB};
use anyhow::ensure;


#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let client = get_client().await?;

    let latest_block = client.get_block_number().await?;
    let block_id = BlockId::Number(BlockNumberOrTag::Number(latest_block));
    let block = client.get_block(block_id.clone(), true).await?;
    let cache_db = CacheDB::new(EmptyDB::default());

    let mut fork_factory = ForkFactory::new_sandbox_factory(
        client.clone(),
        cache_db,
        Some(block_id)
    );

    let contract_address = insert_dummy_account(AccountType::Contract, &mut fork_factory)?;
    let caller = insert_dummy_account(AccountType::EOA, &mut fork_factory)?;

    let fork_db = fork_factory.new_sandbox_fork();

    let evm = new_evm(fork_db.clone(), block.unwrap());


    // ** Simulate a WETH/USDC swap on Uniswap V3
    let pool = Pool {
        address: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640")?,
        token0: *WETH,
        token1: Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")?,
        variant: PoolVariant::UniswapV3
    };

    let amount_in = parse_ether("1")?;

    let swap_params = SwapParams {
        input_token: *WETH,
        output_token: pool.token1,
        amount_in: amount_in,
        pool: pool.address,
        pool_variant: pool.variant(),
        minimum_received: U256::ZERO // no slipage
    };

    // Approve the contract to spend the WETH
    let approve_data = encode_approve(contract_address, amount_in);

    let mut evm_params = EvmParams {
        caller: caller,
        transact_to: *WETH,
        call_data: approve_data.into(),
        value: U256::ZERO,
        apply_changes: true,
        evm: evm
    };

    let result = sim_call(&mut evm_params)?;

    ensure!(!result.is_reverted, "Approve call reverted, Reason: {:?}", bytes_to_string(result.output));

    // Do the swap
    let call_data = encode_swap(swap_params);

    evm_params.set_transact_to(contract_address);
    evm_params.set_call_data(call_data.into());
    let result = sim_call(&mut evm_params)?;

    ensure!(!result.is_reverted, "Swap call reverted, Reason: {:?}", bytes_to_string(result.output));

    let amount_out: U256 = decode_swap(result.output)?;
    ensure!(amount_out > U256::ZERO, "Amount out is zero");
    println!("Swapped {} for {}", to_readable(amount_in, *WETH), to_readable(amount_out, *USDC));

    // check callers USDC balance
    let balance_of_data = encode_balanceof(caller);
    evm_params.set_transact_to(pool.token1);
    evm_params.set_call_data(balance_of_data.clone().into());

    let result = sim_call(&mut evm_params)?;
    let caller_balance: U256 = decode_balanceof(result.output)?;

    ensure!(caller_balance >= amount_out, "Caller didn't receive the swapped amount");
    println!("Caller Received: {}", to_readable(caller_balance, pool.token1));
    
    // send the received USDC to the contract
    let transfer_data = encode_transfer(contract_address, caller_balance);
    evm_params.set_transact_to(pool.token1);
    evm_params.set_call_data(transfer_data.into());

    let result = sim_call(&mut evm_params)?;

    ensure!(!result.is_reverted, "Transfer call reverted, Reason: {:?}", bytes_to_string(result.output));
    println!("Transferred {} to contract", to_readable(caller_balance, pool.token1));

    // withdraw the USDC from the contract
    let withdraw_data = encode_recover_erc20(pool.token1, caller_balance);
    evm_params.set_transact_to(contract_address);
    evm_params.set_call_data(withdraw_data.into());

    let result = sim_call(&mut evm_params)?;

    ensure!(!result.is_reverted, "Withdraw call reverted, Reason: {:?}", bytes_to_string(result.output));

    // check the caller USDC balance again
    evm_params.set_transact_to(pool.token1);
    evm_params.set_call_data(balance_of_data.into());

    let result = sim_call(&mut evm_params)?;
    let caller_balance: U256 = decode_balanceof(result.output)?;

    ensure!(caller_balance >= amount_out, "Caller USDC balance is not zero");
    println!("Recovered {} from contract", to_readable(caller_balance, pool.token1));

    Ok(())
}