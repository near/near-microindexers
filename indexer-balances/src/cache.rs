use cached::{Cached, SizedCache};

use near_lake_framework::near_indexer_primitives;
use tokio::sync::Mutex;

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
        let mut lock = self.cache.lock().await;
        lock.cache_get(account_id).copied()
    }

    pub async fn set(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
        balance: crate::BalanceDetails,
    ) {
        let mut lock = self.cache.lock().await;
        lock.cache_set(account_id.clone(), balance);
    }
}
