// credit to Foundry's SharedBackend implmenetation:
// https://github.com/foundry-rs/foundry/blob/master/evm/src/executor/fork/backend.rs

use alloy::rpc::types::eth::BlockId;
use alloy::providers::{RootProvider, Provider};

use alloy::pubsub::PubSubFrontend;
use alloy::transports::{RpcError, TransportErrorKind};
use alloy::primitives::{ Address, U256, Bytes };

use eyre::Result;
use futures::{ channel::mpsc::Receiver, task::{ Context, Poll }, Future, FutureExt, Stream };
use hashbrown::{ hash_map::Entry, HashMap };
use revm::{ db::{ CacheDB, EmptyDB }, primitives::{ AccountInfo, Bytecode, B256, KECCAK_EMPTY } };
use std::{ collections::VecDeque, pin::Pin, sync::{ mpsc::Sender as OneshotSender, Arc } };

use super::database_error::{ DatabaseError, DatabaseResult };
use super::*;

// **incoming req and outcoming req handled using revm types
// all logic internal to this module handled using ethers types (because of provider)
type AccountInfoSender = OneshotSender<DatabaseResult<AccountInfo>>;
type StorageSender = OneshotSender<DatabaseResult<U256>>;
type BlockHashSender = OneshotSender<DatabaseResult<B256>>;

type BasicFuture<Err> = Pin<
    Box<dyn Future<Output = (Result<(U256, u64, Bytes), Err>, Address)> + Send>
>;
type StorageFuture<Err> = Pin<
    Box<dyn Future<Output = (Result<U256, Err>, Address, U256)> + Send>
>;
type BlockHashFuture<Err> = Pin<Box<dyn Future<Output = (Result<B256, Err>, U256)> + Send>>;

/// Request variants that are executed by the provider
enum FetchRequestFuture<Err> {
    Basic(BasicFuture<Err>),
    Storage(StorageFuture<Err>),
    BlockHash(BlockHashFuture<Err>),
}

/// The Request type the Backend listens for
#[derive(Debug)]
pub enum BackendFetchRequest {
    /// Fetch the account info
    Basic(Address, AccountInfoSender),
    /// Fetch a storage slot
    Storage(Address, U256, StorageSender),
    /// Fetch a block hash
    BlockHash(U256, BlockHashSender),
}

/// Holds db and provdier_db to fallback on so that
/// we can make rpc calls for missing data
pub struct GlobalBackend {
    db: CacheDB<EmptyDB>,
    // used to make calls for missing data
    provider: Arc<RootProvider<PubSubFrontend>>,
    block_num: Option<BlockId>,
    /// Requests currently in progress
    pending_requests: Vec<FetchRequestFuture<RpcError<TransportErrorKind>>>,
    /// Listeners that wait for a `get_account` related response
    account_requests: HashMap<Address, Vec<AccountInfoSender>>,
    /// Listeners that wait for a `get_storage_at` response
    storage_requests: HashMap<(Address, U256), Vec<StorageSender>>,
    /// Listeners that wait for a `get_block` response
    block_requests: HashMap<U256, Vec<BlockHashSender>>,
    /// Incoming commands.
    incoming: Receiver<BackendFetchRequest>,
    /// unprocessed queued requests
    queued_requests: VecDeque<BackendFetchRequest>,
}

impl GlobalBackend {
    // not so elegeant but create sim env from state diffs
    pub fn new(
        rx: Receiver<BackendFetchRequest>,
        block_num: Option<BlockId>,
        provider: Arc<RootProvider<PubSubFrontend>>,
        initial_db: CacheDB<EmptyDB>
    ) -> Self {
        Self {
            db: initial_db,
            provider,
            block_num,
            pending_requests: Default::default(),
            account_requests: Default::default(),
            storage_requests: Default::default(),
            block_requests: Default::default(),
            incoming: rx,
            queued_requests: Default::default(),
        }
    }

    /// handle the request in queue in the future.
    ///
    /// We always check:
    ///  1. if the requested value is already stored in the cache, then answer the sender
    ///  2. otherwise, fetch it via the provider but check if a request for that value is already in
    /// progress (e.g. another Sender just requested the same account)
    fn on_request(&mut self, req: BackendFetchRequest) {
        match req {
            BackendFetchRequest::Basic(addr, sender) => {
                let acc = self.db.accounts.get(&addr);
                if let Some(acc) = acc {
                    let _ = sender.send(Ok(acc.info.clone()));
                } else {
                    self.request_account(addr, sender);
                }
            }
            BackendFetchRequest::Storage(addr, idx, sender) => {
                let value = self.db.accounts.get(&addr).and_then(|acc| acc.storage.get(&idx));
                if let Some(value) = value {
                    let _ = sender.send(Ok(*value));
                } else {
                    // account present but not storage -> fetch storage
                    self.request_account_storage(addr.0.into(), idx, sender)
                }
            }
            BackendFetchRequest::BlockHash(number, sender) => {
                let hash = self.db.block_hashes.get(&number);
                if let Some(hash) = hash {
                    let _ = sender.send(Ok(hash.0.into()));
                } else {
                    self.request_hash(number, sender);
                }
            }
        }
    }

    /// process a request for an account
    fn request_account(&mut self, address: Address, listener: AccountInfoSender) {
        match self.account_requests.entry(address) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().push(listener);
            }
            Entry::Vacant(entry) => {
                entry.insert(vec![listener]);
                let provider = self.provider.clone();
                let block_num = self.block_num.unwrap();
                let fut = Box::pin(async move {

                    let balance = provider.get_balance(address, block_num);
                    let nonce = provider.get_transaction_count(address, block_num);
                    let code = provider.get_code_at(address, block_num);
                    let resp = tokio::try_join!(balance, nonce, code);

                   
                    (resp, address)
                });
                self.pending_requests.push(FetchRequestFuture::Basic(fut));
            }
        }
    }

    // Process a request for account's storage
    fn request_account_storage(&mut self, address: Address, idx: U256, listener: StorageSender) {
        match self.storage_requests.entry((address, idx)) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().push(listener);
            }
            Entry::Vacant(entry) => {
                entry.insert(vec![listener]);
                let provider = self.provider.clone();
                let block_num = self.block_num;
                let fut = Box::pin(async move {

                    let storage = provider.get_storage_at(
                        address,
                        idx,
                        block_num.unwrap()
                    ).await;


                    (storage, address, idx)
                });
                self.pending_requests.push(FetchRequestFuture::Storage(fut));
            }
        }
    }

    // Process a request for a block hash
    fn request_hash(&mut self, number: U256, listener: BlockHashSender) {
        match self.block_requests.entry(number) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().push(listener);
            }
            Entry::Vacant(entry) => {
                entry.insert(vec![listener]);
                let provider = self.provider.clone();
                let block_id = self.block_num.unwrap();

                let fut = Box::pin(async move {
                    let block = provider.get_block(block_id, true).await;

                    let block_hash = match block {
                        Ok(Some(block)) =>
                            Ok(
                                block.header.hash.expect(
                                    "empty block hash on mined block, this should never happen"
                                )
                            ),
                        Ok(None) => {
                            // if no block was returned then the block does not exist, in which case
                            // we return empty hash
                            Ok(KECCAK_EMPTY.0.into())
                        }
                        Err(err) => Err(err),
                    };

                    (block_hash, number)
                });
                self.pending_requests.push(FetchRequestFuture::BlockHash(fut));
            }
        }
    }
}

impl Future for GlobalBackend {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let pin = self.get_mut();
        loop {
            // Drain queued requests first.
            while let Some(req) = pin.queued_requests.pop_front() {
                pin.on_request(req);
            }

            // receive new requests to delegate to the underlying provider
            loop {
                match Pin::new(&mut pin.incoming).poll_next(cx) {
                    Poll::Ready(Some(req)) => {
                        pin.queued_requests.push_back(req);
                    }
                    Poll::Ready(None) => {
                        return Poll::Ready(());
                    }
                    Poll::Pending => {
                        break;
                    }
                }
            }

            // poll all requests in progress
            for n in (0..pin.pending_requests.len()).rev() {
                let mut request = pin.pending_requests.swap_remove(n);
                match &mut request {
                    FetchRequestFuture::Basic(fut) => {
                        if let Poll::Ready((resp, addr)) = fut.poll_unpin(cx) {
                            // get the response
                            let (balance, nonce, code) = match resp {
                                Ok(res) => res,
                                Err(err) => {
                                    let err = Arc::new(eyre::Error::new(err));
                                    if let Some(listeners) = pin.account_requests.remove(&addr) {
                                        listeners.into_iter().for_each(|l| {
                                            let _ = l.send(
                                                Err(
                                                    DatabaseError::GetAccount(
                                                        addr,
                                                        Arc::clone(&err)
                                                    )
                                                )
                                            );
                                        });
                                    }
                                    continue;
                                }
                            };

                            // convert it to revm-style types
                            let (code, code_hash) = if !code.is_empty() {
                                (Some(code.clone()), keccak256(&code).into())
                            } else {
                                (Some(revm::primitives::Bytes::default()), KECCAK_EMPTY)
                            };

                            // update the cache
                            let acc = AccountInfo {
                                nonce,
                                balance,
                                code: code.map(|bytes| Bytecode::new_raw(bytes)),
                                code_hash,
                            };
                            pin.db.insert_account_info(addr, acc.clone());

                            // notify all listeners
                            if let Some(listeners) = pin.account_requests.remove(&addr) {
                                listeners.into_iter().for_each(|l| {
                                    let _ = l.send(Ok(acc.clone()));
                                });
                            }
                            continue;
                        }
                    }
                    FetchRequestFuture::Storage(fut) => {
                        if let Poll::Ready((resp, addr, idx)) = fut.poll_unpin(cx) {
                            let value = match resp {
                                Ok(value) => value,
                                Err(err) => {
                                    // notify all listeners
                                    let err = Arc::new(eyre::Error::new(err));
                                    if
                                        let Some(listeners) = pin.storage_requests.remove(
                                            &(addr, idx)
                                        )
                                    {
                                        listeners.into_iter().for_each(|l| {
                                            let _ = l.send(
                                                Err(
                                                    DatabaseError::GetStorage(
                                                        addr,
                                                        idx,
                                                        Arc::clone(&err)
                                                    )
                                                )
                                            );
                                        });
                                    }
                                    continue;
                                }
                            };

                            // update the cache
                            pin.db.insert_account_storage(addr, idx, value).unwrap();

                            // notify all listeners
                            if let Some(listeners) = pin.storage_requests.remove(&(addr, idx)) {
                                listeners.into_iter().for_each(|l| {
                                    let _ = l.send(Ok(value));
                                });
                            }
                            continue;
                        }
                    }
                    FetchRequestFuture::BlockHash(fut) => {
                        if let Poll::Ready((block_hash, number)) = fut.poll_unpin(cx) {
                            let value = match block_hash {
                                Ok(value) => value,
                                Err(err) => {
                                    let err = Arc::new(eyre::Error::new(err));
                                    // notify all listeners
                                    if let Some(listeners) = pin.block_requests.remove(&number) {
                                        listeners.into_iter().for_each(|l| {
                                            let _ = l.send(
                                                Err(
                                                    DatabaseError::GetBlockHash(
                                                        number,
                                                        Arc::clone(&err)
                                                    )
                                                )
                                            );
                                        });
                                    }
                                    continue;
                                }
                            };

                            // update the cache
                            pin.db.block_hashes.insert(number, value);

                            // notify all listeners
                            if let Some(listeners) = pin.block_requests.remove(&number) {
                                listeners.into_iter().for_each(|l| {
                                    let _ = l.send(Ok(value.0.into()));
                                });
                            }
                            continue;
                        }
                    }
                }
                // not ready, insert and poll again
                pin.pending_requests.push(request);
            }

            // If no new requests have been queued, break to
            // be polled again later.
            if pin.queued_requests.is_empty() {
                return Poll::Pending;
            }
        }
    }
}
