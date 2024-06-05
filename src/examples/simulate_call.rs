use alloy::providers::Provider;
use alloy::rpc::types::eth::{BlockId, BlockNumberOrTag};
use alloy::primitives::U256;
use revm_by_example::{ forked_db::fork_factory::ForkFactory, *, utils::* };

use revm::db::{ CacheDB, EmptyDB };
use anyhow::ensure;



#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let client = get_client().await?;


    // setup the fork environment
    let latest_block = client.get_block_number().await?;
    let block_id = BlockId::Number(BlockNumberOrTag::Number(latest_block));
    let block = client.get_block(block_id.clone(), true).await?;
    let cache_db = CacheDB::new(EmptyDB::default());

    let mut fork_factory = ForkFactory::new_sandbox_factory(
        client.clone(),
        cache_db,
        Some(block_id)
    );

    // insert a dummy EOA account for easier testing with a balance of 1 ETH and 1 WETH
    let one_eth = parse_ether("1")?;
    let dummy_account = DummyAccount::new(AccountType::EOA, one_eth, one_eth);
    insert_dummy_account(&dummy_account, &mut fork_factory)?;

    // create a new fork db
    let fork_db = fork_factory.new_sandbox_fork();

    // we start from a clean state + any account information we have added, in this case when we print the fork_db
    // should only show the dummy account address and the weth address
    println!("Fork DB Accounts: {:?}", fork_db.db.accounts.keys());

    // setup a new evm instance
    let evm = new_evm(fork_db.clone(), block.clone().unwrap());
    let weth = ERC20Token::new(*WETH, client.clone()).await?;

    // ** Get the WETH balance of the dummy account


    let mut evm_params = EvmEnv {
        caller: dummy_account.address.clone(),
        transact_to: *WETH,
        call_data: weth.encode_balance_of(dummy_account.address).into(),
        value: U256::ZERO,
        evm: evm
    };

    evm_params.set_tx_env();
    let res = evm_params.evm.transact()?;
    let output = res.result.output().unwrap_or_default();

    // make sure the call is not reverted
    ensure!(res.result.is_success(), "BalanceOf call reverted, Reason: {:?}", revert_msg(output));

    // decode the output evm returned to get the balance, should be 1 WETH
    let balance: U256 = weth.decode_balance_of(output)?;
    ensure!(balance == one_eth, "Balance is not 1 WETH: {}", balance);
    println!("Account Initial WETH Balance: {}", to_readable(balance, weth.clone()));

    // ** wrap 1 ETH to WETH by interacting with the WETH contract
    // ** To do this we need to call the deposit function of the WETH contract
    // ** And send 1 ETH to it


    evm_params.set_value(one_eth);
    evm_params.set_call_data(weth.encode_deposit().into());

    // simulate the call, transact_commit will apply the state changes
    let res = evm_params.evm.transact_commit()?;
    let output = res.output().unwrap_or_default();

    ensure!(res.is_success(), "Deposit call reverted, Reason: {:?}", revert_msg(output));


    // ** get the weth balance again
    evm_params.set_value(U256::ZERO);
    evm_params.set_call_data(weth.encode_balance_of(dummy_account.address).into());

    let res = evm_params.evm.transact()?;
    let output = res.result.output().unwrap_or_default();


    let balance: U256 = weth.decode_balance_of(output)?;
    ensure!(balance == parse_ether("2").unwrap(), "Balance should be 2 WETH: {}", balance);
    
    println!("Wrapped 1 ETH, New WETH Balance: {}", to_readable(balance, weth.clone()));

    // Any changes are applied to [Evm] so if we create a new evm instance even with the same fork_db we should start from a clean state
    let evm = new_evm(fork_db, block.unwrap());

    // get the weth balance again
    evm_params.set_evm(evm);
    
    let res = evm_params.evm.transact()?;
    let output = res.result.output().unwrap_or_default();

    let balance: U256 = weth.decode_balance_of(output)?;
    ensure!(balance == one_eth, "Balance should be 1 WETH: {}", balance);
    
    println!("Account WETH Balance After New EVM: {}", to_readable(balance, weth));


    Ok(())
}