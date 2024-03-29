use ethers::prelude::*;
use std::str::FromStr;

use revm_by_example::{
    forked_db::{ fork_factory::ForkFactory, to_revm_u256, to_ethers_address },
    *,
};

use revm::db::{ CacheDB, EmptyDB };
use revm::primitives::{ Bytes as rBytes, TransactTo };

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let client = get_client().await?;

    let latest_block = client.get_block_number().await?;
    let block = client.get_block(latest_block).await?;
    let cache_db = CacheDB::new(EmptyDB::default());
    let block_id = BlockId::Number(BlockNumber::Number(latest_block));

    let mut mempool_stream = if let Ok(stream) = client.subscribe_full_pending_txs().await {
        stream
    } else {
        return Err(anyhow::anyhow!("Failed to subscribe to pending transactions"));
    };

    let pools = get_pools();

    // in a real application you should update the block_id to the latest block
    let fork_factory = ForkFactory::new_sandbox_factory(
        client.clone(),
        cache_db.clone(),
        Some(block_id)
    );
    let fork_db = fork_factory.new_sandbox_fork();

    while let Some(tx) = mempool_stream.next().await {
        {
            let mut evm = new_evm(fork_db.clone(), block.clone().unwrap());

            evm.tx_mut().caller = tx.from.0.into();
            evm.tx_mut().transact_to = TransactTo::Call(tx.to.unwrap_or_default().0.into());
            evm.tx_mut().data = rBytes::from(tx.input.0);
            evm.tx_mut().value = to_revm_u256(tx.value);

            let res = evm.transact()?;
            let touched_accs = res.state.keys();
            let touched_pools: Vec<Address> = touched_accs
                .clone()
                .into_iter()
                .filter(|acc| pools.contains(&to_ethers_address(**acc)))
                .map(|acc| to_ethers_address(*acc))
                .collect();
           
            if !touched_pools.is_empty() {
                let output = format!(
                    "Tx Touched pools: {:?}
                View on Etherscan https://etherscan.io/tx/{:?}",
                    touched_pools,
                    tx.hash
                );
                println!("{}", output);
            }
        }
    }

    Ok(())
}

fn get_pools() -> Vec<Address> {
    vec![
        Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
        Address::from_str("0x11b815efb8f581194ae79006d24e0d814b7697f6").unwrap(),
        Address::from_str("0x0d4a11d5eeaac28ec3f61d100daf4d40471f1852").unwrap(),
        Address::from_str("0xa43fe16908251ee70ef74718545e4fe6c5ccec9f").unwrap()
    ]
}
