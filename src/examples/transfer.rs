use alloy::providers::Provider;
use alloy::rpc::types::eth::{ BlockId, BlockNumberOrTag };
use alloy::primitives::U256;
use alloy::primitives::utils::{ parse_ether, format_ether };
use revm_by_example::{ forked_db::fork_factory::ForkFactory, *, utils::* };

use revm::{ db::{ CacheDB, EmptyDB }, interpreter::Host, primitives::{ TransactTo, Bytes } };
use anyhow::{ensure, anyhow };

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

    // create 2 dummy accounts Bob and Alice

    // give Alice 100 ETH and 100 WETH
    let alice = DummyAccount::new(AccountType::EOA, parse_ether("100")?, parse_ether("100")?);

    let bob = DummyAccount::new(AccountType::EOA, U256::ZERO, U256::ZERO);

    // insert Bob & Alice to the local fork db
    insert_dummy_account(&alice, &mut fork_factory)?;

    insert_dummy_account(&bob, &mut fork_factory)?;

    let fork_db = fork_factory.new_sandbox_fork();

    let mut evm = new_evm(fork_db.clone(), block.clone().unwrap());
    let weth = ERC20Token::new(*WETH, client.clone()).await?;

    // query the balance of Alice

    let (balance, _) = evm.context.balance(alice.address).unwrap();
    println!("Alice balance: {} ETH", format_ether(balance));

    // transfer 10 ETH from Alice to Bob
    let amount = parse_ether("10")?;

    // setup the minimum tx fields needed
    evm.tx_mut().caller = alice.address;
    evm.tx_mut().value = amount;
    evm.tx_mut().transact_to = TransactTo::Call(bob.address);
    evm.tx_mut().data = Bytes::default();

    // transact and commit changes
    evm.transact_commit()?;

    // query the balance of Bob
    let (balance, _) = evm.context.balance(bob.address).unwrap();
    ensure!(balance >= amount, "Bob's balance is less than 10 ETH, Bob not happy!");

    let received = format!("Bob just received: {:.4} ETH!", format_ether(amount));
    let bob_balance = format!("Bob's balance: {:.4} ETH", format_ether(balance));
    let bob_happy = "Bob is happy!";
    let msg = format!("\n{}\n{}\n{}", received, bob_balance, bob_happy);

    println!("{}", msg);

    // Now transfer 10 WETH from Alice to Bob

    // encode the erc20 transfer call data
    let call_data = weth.encode_transfer(bob.address, amount);

    evm.tx_mut().data = call_data.into();
    evm.tx_mut().value = U256::ZERO;
    evm.tx_mut().transact_to = TransactTo::Call(weth.address);

    let res = evm.transact_commit()?;
    let output = res.output().unwrap_or_default();

    if !res.is_success() {
        let err = revert_msg(output);
        return Err(anyhow!("ERC20 Transfer Failed, Reason: {}", err));
    }
    
    // query Bob's WETH balance
    let call_data = weth.encode_balance_of(bob.address);

    evm.tx_mut().data = call_data.into();

    let res = evm.transact()?.result;
    let output = res.output().unwrap_or_default();

    let balance = weth.decode_balance_of(&output)?;
    ensure!(balance >= amount, "Bob's WETH balance is less than 10 WETH");

    let received = format!("Bob just received: {:.4} WETH!", format_ether(balance));
    let bob_balance = format!("Bob's WETH balance: {:.4} WETH", format_ether(balance));

    let msg = format!("\n{}\n{}", received, bob_balance);

    println!("{}", msg);

  Ok(())
}
