use cached::{Cached, SizedCache};

use near_lake_framework::near_indexer_primitives;
use tokio::sync::Mutex;

use crate::metrics;

pub struct BalanceCache {
    cache: std::sync::Arc<
        Mutex<SizedCache<near_indexer_primitives::types::AccountId, crate::BalanceDetails>>,
    >,
}

impl BalanceCache {
    pub fn new(size: usize) -> Self {
        Self {
            cache: std::sync::Arc::new(Mutex::new(SizedCache::with_size(size))),
        }
    }

    pub async fn get(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
    ) -> Option<crate::BalanceDetails> {
        let mut lock = self.get_lock().await;
        lock.cache_get(account_id).copied()
    }

    pub async fn set(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
        balance: crate::BalanceDetails,
    ) {
        let mut lock = self.get_lock().await;
        lock.cache_set(account_id.clone(), balance);
    }

    async fn get_lock(
        &self,
    ) -> tokio::sync::MutexGuard<
        '_,
        cached::SizedCache<near_primitives::types::AccountId, crate::BalanceDetails>,
    > {
        let lock = self.cache.lock().await;

        metrics::CACHE_HITS.set(lock.cache_hits().unwrap_or(0) as i64);
        metrics::CACHE_MISSES.set(lock.cache_misses().unwrap_or(0) as i64);
        metrics::CACHE_SIZE.set(lock.cache_size() as i64);

        lock
    }
}
