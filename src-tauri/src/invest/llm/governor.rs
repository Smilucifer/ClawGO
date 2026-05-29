use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::types::ProviderId;

/// Per-provider concurrency governor (RFC D8).
/// Each provider gets an independent Semaphore with 8 permits.
/// 5 assets x 3 roles = 15 concurrent requests capped to 8 per provider.
pub struct LlmGovernor {
    semaphores: HashMap<ProviderId, Arc<Semaphore>>,
}

impl LlmGovernor {
    pub fn new() -> Self {
        let mut semaphores = HashMap::new();
        for provider in ProviderId::all() {
            semaphores.insert(*provider, Arc::new(Semaphore::new(8)));
        }
        Self { semaphores }
    }

    /// Acquire a permit for the given provider. Blocks if all 8 permits are taken.
    pub async fn acquire(&self, provider: ProviderId) -> OwnedSemaphorePermit {
        self.semaphores[&provider]
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed unexpectedly")
    }

    /// Try to acquire without blocking. Returns None if no permits available.
    pub fn try_acquire(&self, provider: ProviderId) -> Option<OwnedSemaphorePermit> {
        self.semaphores[&provider]
            .clone()
            .try_acquire_owned()
            .ok()
    }
}

impl Default for LlmGovernor {
    fn default() -> Self {
        Self::new()
    }
}

// Global singleton -- created once, shared across all committee runs.
use std::sync::OnceLock;

static GOVERNOR: OnceLock<LlmGovernor> = OnceLock::new();

pub fn global_governor() -> &'static LlmGovernor {
    GOVERNOR.get_or_init(LlmGovernor::new)
}
