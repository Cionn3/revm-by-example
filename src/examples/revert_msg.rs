use alloy::providers::Provider;
use alloy::rpc::types::eth::{ BlockId, BlockNumberOrTag };
use alloy::primitives::{U256 , address};
use alloy::primitives::utils::{ parse_units, ParseUnits};
use revm_by_example::{ forked_db::fork_factory::ForkFactory, *, utils::* };

use revm::{ db::{ CacheDB, EmptyDB }, primitives::TransactTo };

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let client = get_client().await?;

    // setup the fork environment
    let latest_block = client.get_block_number().await?;
    let block_id = BlockId::Number(BlockNumberOrTag::Number(latest_block));
    let block = client.get_block(block_id.clone(), true.into()).await?;
    let cache_db = CacheDB::new(EmptyDB::default());

    let mut fork_factory = ForkFactory::new_sandbox_factory(
        client.clone(),
        cache_db,
        Some(block_id)
    );

    // create a dummy account with no funds

    let bob = DummyAccount::new(AccountType::EOA, U256::ZERO, U256::ZERO);

    // insert Bob & Alice to the local fork db
    insert_dummy_account(&bob, &mut fork_factory)?;

    let fork_db = fork_factory.new_sandbox_fork();

    let vitalik = address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045");

    let mut evm = new_evm(fork_db.clone(), block.clone().unwrap());

    // try to send 1000 usdc to vitalik
    let usdc = ERC20Token::new(*USDC, client.clone()).await?;
    let amount = parse_units("1000", 6)?;

    let amount = match amount {
        ParseUnits::U256(amount) => amount,
        _ => panic!("Should be U256"),
    };

    let call_data = usdc.encode_transfer(vitalik, amount);

    evm.tx_mut().caller = bob.address;
    evm.tx_mut().value = U256::ZERO;
    evm.tx_mut().transact_to = TransactTo::Call(usdc.address);
    evm.tx_mut().data = call_data.into();

    let res = evm.transact_commit()?;
    let output = res.output().unwrap_or_default();



    // And as expected, the call should revert
    if !res.is_success() {
        println!("Call Reverted, Reason: {:?}", revert_msg(output));
    } else {
        println!("Call Successful, This should not happen");}


    Ok(())
}
