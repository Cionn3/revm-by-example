use ethers::{prelude::*, utils::parse_ether};
use std::str::FromStr;
use revm_by_example::{ forked_db::fork_factory::ForkFactory, *, forked_db::bytes_to_string };

use revm::db::{CacheDB, EmptyDB};


#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    let caller = Address::from_str("0x005189A5c1dc9C75ACee8D38321d5d6f8E54B1b6")?;
    let contract_address = Address::from_str("0x0093562c7e4BcC8e4D256A27e08C9ae6Ac4F895c")?;


    let client = get_client().await?;

    let latest_block = client.get_block_number().await?;
    let block = client.get_block(latest_block).await?;
    let cache_db = CacheDB::new(EmptyDB::default());
    let block_id = BlockId::Number(BlockNumber::Number(latest_block));

    let mut fork_factory = ForkFactory::new_sandbox_factory(
        client.clone(),
        cache_db,
        Some(block_id)
    );

    insert_dummy_contract_account(&mut fork_factory)?;

    let fork_db = fork_factory.new_sandbox_fork();

    let balance_of_data = erc20_balanceof().encode("balanceOf", contract_address)?;
    let mut evm = new_evm(fork_db.clone(), block.unwrap());

    // make sure the contract is funded
    let result = sim_call(
        caller,
        *WETH,
        balance_of_data.clone(),
        U256::zero(),
        false,
        &mut evm
    )?;

    assert!(!result.is_reverted, "BalanceOf call reverted, Reason: {:?}", bytes_to_string(result.output));

    let balance: U256 = erc20_balanceof().decode_output("balanceOf", &result.output)?;
    assert!(balance > U256::zero(), "Contract is not funded");

    // ** Simulate a WETH/USDC swap on Uniswap V3
    let pool = Pool {
        address: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640")?,
        token0: *WETH,
        token1: Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")?,
        variant: PoolVariant::UniswapV3
    };

    let amount_in = parse_ether(2)?;

    let call_data = encode_swap(
        *WETH,
        pool.token1,
        amount_in,
        pool.address,
        pool.variant(),
        U256::zero()
    );

    let result = sim_call(
        caller,
        contract_address,
        call_data.into(),
        U256::zero(),
        true,
        &mut evm
    )?;

    assert!(!result.is_reverted, "Swap call reverted, Reason: {:?}", bytes_to_string(result.output));

    let amount_out: U256 = swap().decode_output("swap", &result.output)?;
    println!("Swapped {} for {}", to_readable(amount_in, *WETH), to_readable(amount_out, *USDC));

    // ** Withdraw the USDC from the contract
    let withdraw_data = encode_recover_erc20(pool.token1, amount_out);

    let result = sim_call(
        caller,
        contract_address,
        withdraw_data.into(),
        U256::zero(),
        true,
        &mut evm
    )?;

    assert!(!result.is_reverted, "Withdraw call reverted, Reason: {:?}", bytes_to_string(result.output));
    
    let balance_of_data = erc20_balanceof().encode("balanceOf", caller)?;
    let result = sim_call(
        caller,
        pool.token1,
        balance_of_data,
        U256::zero(),
        false,
        &mut evm
    )?;

    assert!(!result.is_reverted, "BalanceOf call reverted, Reason: {:?}", bytes_to_string(result.output));

    let balance: U256 = erc20_balanceof().decode_output("balanceOf", &result.output)?;
    assert!(balance == amount_out, "Caller's USDC balance != amount_out");
    println!("Withdraw success!, Caller's USDC balance: {}", to_readable(balance, *USDC));



    Ok(())
}