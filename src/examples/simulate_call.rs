use alloy::providers::Provider;
use alloy::rpc::types::eth::{BlockId, BlockNumberOrTag};
use alloy::primitives::{Address, U256};
use revm_by_example::forked_db::bytes_to_string;
use revm_by_example::{ forked_db::fork_factory::ForkFactory, *, parse_ether };

use revm::db::{ CacheDB, EmptyDB };
use anyhow::ensure;



#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let client = get_client().await?;
    let one_eth = parse_ether("1")?;
    println!("1 ETH in Wei: {}", one_eth);

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
    let dummy_address = insert_dummy_account(AccountType::EOA, &mut fork_factory)?;

    // create a new fork db
    let fork_db = fork_factory.new_sandbox_fork();

    // we start from a clean state + any account information we have added, in this case when we print the fork_db
    // should only show the dummy account address and the weth address
    println!("Fork DB Accounts: {:?}", fork_db.db.accounts.keys());

    // setup a new evm instance
    let evm = new_evm(fork_db.clone(), block.clone().unwrap());

    // ** Get the WETH balance of the dummy account

    // call data
    let balance_of_data = encode_balanceof(dummy_address);

    let mut evm_params = EvmParams {
        caller: Address::ZERO, // <- caller here can be zero since doesn't matter for this call
        transact_to: *WETH, // <- the contract address we interact with
        call_data: balance_of_data.clone().into(),
        value: U256::ZERO, // <- ETH to send with the transaction
        apply_changes: false, // <- whether to apply the state changes or not
        evm: evm
    };

    let result = sim_call(&mut evm_params)?;

    // make sure the call is not reverted
    ensure!(!result.is_reverted, "BalanceOf call reverted, Reason: {:?}", bytes_to_string(result.output));

    // decode the output evm returned to get the balance, should be 1 WETH
    let balance: U256 = decode_balanceof(result.output)?;
    ensure!(balance == parse_ether("1").unwrap(), "Balance is not 1 WETH: {}", balance);
    println!("Account Initial WETH Balance: {}", to_readable(balance, *WETH));

    // ** wrap 1 ETH to WETH by interacting with the WETH contract
    // ** To do this we need to call the deposit function of the WETH contract
    // ** And send 1 ETH to it

    let value = parse_ether("1")?;
    let deposit_data = weth_deposit();

    evm_params.set_caller(dummy_address);
    evm_params.set_value(value);
    evm_params.set_call_data(deposit_data.clone().into());

    // simulate the call without applying any state changes
    let result = sim_call(&mut evm_params)?;

    ensure!(!result.is_reverted, "Deposit call reverted, Reason: {:?}", bytes_to_string(result.output));

    // ** because we didnt apply the state changes, quering the weth balance again should return 1 weth
    evm_params.set_value(U256::ZERO);
    evm_params.set_call_data(balance_of_data.clone().into());
    let result = sim_call(&mut evm_params)?;
    
    ensure!(!result.is_reverted, "BalanceOf call reverted, Reason: {:?}", bytes_to_string(result.output));

    let balance: U256 = decode_balanceof(result.output)?;
    ensure!(balance == parse_ether("1").unwrap(), "Balance is not 1 WETH: {}", balance);

    // ** sim again the deposit applying the state changes
    evm_params.set_value(value);
    evm_params.set_call_data(deposit_data.clone().into());
    evm_params.set_apply_changes(true);
    let result = sim_call(&mut evm_params)?;

    ensure!(!result.is_reverted, "Deposit call reverted, Reason: {:?}", bytes_to_string(result.output));

    // ** get the weth balance again
    evm_params.set_value(U256::ZERO);
    evm_params.set_call_data(balance_of_data.clone().into());
    let result = sim_call(&mut evm_params)?;


    let balance: U256 = decode_balanceof(result.output)?;
    

    // now the balance should be 2 WETH
    ensure!(balance == parse_ether("2").unwrap(), "Balance is not 2 WETH: {}", balance);
    println!("Wrapped 1 ETH, New WETH Balance: {}", to_readable(balance, *WETH));

    // Any changes are applied to [Evm] so if we create a new evm instance even with the same fork_db we should start from a clean state
    let evm = new_evm(fork_db, block.unwrap());

    // get the weth balance again
    evm_params.set_evm(evm);
    let result = sim_call(&mut evm_params)?;

    let balance: U256 = decode_balanceof(result.output)?;
    
    
    ensure!(balance == parse_ether("1").unwrap(), "Balance is not 1 WETH: {}", balance);
    println!("Account WETH Balance After New EVM: {}", to_readable(balance, *WETH));


    Ok(())
}