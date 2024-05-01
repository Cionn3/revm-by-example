use alloy::providers::Provider;
use alloy::rpc::types::eth::{BlockId, BlockNumberOrTag};
use alloy::primitives::{Address, U256};use std::str::FromStr;
use revm_by_example::{ forked_db::fork_factory::ForkFactory, *, utils::* };

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

    let one_eth = parse_ether("1")?;
    let dummy_contract = DummyAccount::new(AccountType::Contract(swap_router_bytecode()), U256::ZERO, U256::ZERO);
    let dummy_account = DummyAccount::new(AccountType::EOA, one_eth, one_eth);
    insert_dummy_account(&dummy_contract, &mut fork_factory)?;
    insert_dummy_account(&dummy_account, &mut fork_factory)?;

    let fork_db = fork_factory.new_sandbox_fork();

    let evm = new_evm(fork_db, block.unwrap());
    let weth = ERC20Token::new(*WETH, client.clone()).await?;
    let usdc = ERC20Token::new(*USDC, client.clone()).await?;


    let pool = Pool {
        address: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640")?,
        token0: weth.address,
        token1: usdc.address,
        variant: PoolVariant::UniswapV3
    };

    let swap_params = SwapParams {
        input_token: weth.address,
        output_token: usdc.address,
        amount_in: one_eth,
        pool: pool.address,
        pool_variant: pool.variant(),
        minimum_received: U256::ZERO // no slipage
    };

    // Approve the contract to spend 1 WETH
    let call_data = weth.encode_approve(dummy_contract.address, one_eth);

    let mut evm_params = EvmParams {
        caller: dummy_account.address,
        transact_to: weth.address,
        call_data: call_data.into(),
        value: U256::ZERO,
        evm: evm
    };

    evm_params.set_tx_env();
    let res = evm_params.evm.transact_commit()?;
    let output = res.output().unwrap_or_default();

    ensure!(res.is_success(), "Approve call reverted, Reason: {:?}", revert_msg(&output));

    // ** Simulate a WETH/USDC swap on Uniswap V3
    let call_data = encode_swap(swap_params);

    evm_params.set_transact_to(dummy_contract.address);
    evm_params.set_call_data(call_data.into());
    evm_params.set_tx_env();

    let res = evm_params.evm.transact_commit()?;
    let output = res.output().unwrap_or_default();

    ensure!(res.is_success(), "Swap call reverted, Reason: {:?}", revert_msg(&output));
    

    let amount_out: U256 = decode_swap(output)?;
    ensure!(amount_out > U256::ZERO, "Amount out is zero");
    println!("Swapped {} for {}", to_readable(one_eth, weth), to_readable(amount_out, usdc));


    Ok(())
}