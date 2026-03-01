//! Tab-level async locking for parallel browser operations.
//!
//! Provides per-tab operation locks so that multiple tabs can be operated on
//! concurrently while operations on the same tab are serialized. This prevents
//! race conditions when two API requests target the same tab simultaneously
//! (e.g. navigate + screenshot) without blocking unrelated tabs.
//!
//! Uses `tokio::sync::Mutex` for async-safe locking with a configurable
//! acquisition timeout (default 30 seconds). If the lock cannot be acquired
//! within the timeout, a `BrowserError::TabLocked` error is returned.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use crate::error::{BrowserError, BrowserResult};

/// Default timeout for acquiring a tab-level operation lock (30 seconds).
const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-tab async operation lock for serializing concurrent operations on the same tab.
///
/// Each tab gets its own `tokio::sync::Mutex<()>` so that operations on different
/// tabs can proceed in parallel, while operations on the same tab are serialized.
/// Lock acquisition has a configurable timeout to prevent indefinite blocking.
pub struct TabLockManager {
    /// Map of tab UUID to its async operation lock.
    locks: RwLock<HashMap<Uuid, Arc<TokioMutex<()>>>>,

    /// Maximum time to wait when acquiring a tab lock before returning TabLocked error.
    lock_timeout: Duration,
}

impl Default for TabLockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TabLockManager {
    /// Creates a new TabLockManager with the default 30-second lock timeout.
    pub fn new() -> Self {
        Self {
            locks: RwLock::new(HashMap::new()),
            lock_timeout: DEFAULT_LOCK_TIMEOUT,
        }
    }

    /// Creates a new TabLockManager with a custom lock acquisition timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            locks: RwLock::new(HashMap::new()),
            lock_timeout: timeout,
        }
    }

    /// Returns the configured lock acquisition timeout.
    pub fn lock_timeout(&self) -> Duration {
        self.lock_timeout
    }

    /// Retrieves or creates the async lock for a given tab.
    ///
    /// Uses a read lock on the HashMap first (fast path), falling back to a
    /// write lock only when the entry needs to be created.
    fn get_or_create_lock(&self, tab_id: Uuid) -> Arc<TokioMutex<()>> {
        // Fast path: lock already exists
        {
            let locks = self.locks.read();
            if let Some(lock) = locks.get(&tab_id) {
                return Arc::clone(lock);
            }
        }

        // Slow path: create new lock
        let mut locks = self.locks.write();
        // Double-check after acquiring write lock
        locks
            .entry(tab_id)
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    /// Executes an async closure while holding the tab-level operation lock.
    ///
    /// Ensures that only one operation runs on a given tab at a time. Operations
    /// on different tabs proceed in parallel without blocking each other.
    ///
    /// Returns `BrowserError::TabLocked` if the lock cannot be acquired within
    /// the configured timeout period.
    ///
    /// # Arguments
    ///
    /// * `tab_id` - The UUID of the tab to lock
    /// * `f` - The async closure to execute while holding the lock
    pub async fn with_tab_lock<F, Fut, R>(
        &self,
        tab_id: Uuid,
        f: F,
    ) -> BrowserResult<R>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = BrowserResult<R>>,
    {
        let lock = self.get_or_create_lock(tab_id);

        let guard = tokio::time::timeout(self.lock_timeout, lock.lock())
            .await
            .map_err(|_| BrowserError::TabLocked(tab_id))?;

        let result = f().await;
        drop(guard);
        result
    }

    /// Removes the lock entry for a closed tab to prevent memory leaks.
    ///
    /// Should be called when a tab is closed to clean up the lock HashMap.
    pub fn remove_tab(&self, tab_id: Uuid) {
        let mut locks = self.locks.write();
        locks.remove(&tab_id);
    }

    /// Returns the number of currently tracked tab locks.
    pub fn lock_count(&self) -> usize {
        self.locks.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::time::Instant;

    #[tokio::test]
    async fn test_parallel_tab_operations() {
        let manager = Arc::new(TabLockManager::new());
        let tab_a = Uuid::new_v4();
        let tab_b = Uuid::new_v4();

        let counter = Arc::new(AtomicU32::new(0));

        let m1 = Arc::clone(&manager);
        let c1 = Arc::clone(&counter);
        let h1 = tokio::spawn(async move {
            m1.with_tab_lock(tab_a, || async {
                // Simulate work
                tokio::time::sleep(Duration::from_millis(50)).await;
                c1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();
        });

        let m2 = Arc::clone(&manager);
        let c2 = Arc::clone(&counter);
        let h2 = tokio::spawn(async move {
            m2.with_tab_lock(tab_b, || async {
                // Simulate work
                tokio::time::sleep(Duration::from_millis(50)).await;
                c2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();
        });

        let start = Instant::now();
        h1.await.unwrap();
        h2.await.unwrap();
        let elapsed = start.elapsed();

        assert_eq!(counter.load(Ordering::SeqCst), 2);
        // Both should run in parallel, so total time should be ~50ms, not ~100ms
        assert!(
            elapsed < Duration::from_millis(90),
            "Parallel tabs took {:?}, expected < 90ms",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_same_tab_sequential() {
        let manager = Arc::new(TabLockManager::new());
        let tab_id = Uuid::new_v4();
        let order = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        let m1 = Arc::clone(&manager);
        let o1 = Arc::clone(&order);
        let h1 = tokio::spawn(async move {
            m1.with_tab_lock(tab_id, || {
                let o1 = Arc::clone(&o1);
                async move {
                    o1.lock().await.push(1);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    o1.lock().await.push(2);
                    Ok(())
                }
            })
            .await
            .unwrap();
        });

        // Small delay to ensure h1 acquires the lock first
        tokio::time::sleep(Duration::from_millis(5)).await;

        let m2 = Arc::clone(&manager);
        let o2 = Arc::clone(&order);
        let h2 = tokio::spawn(async move {
            m2.with_tab_lock(tab_id, || {
                let o2 = Arc::clone(&o2);
                async move {
                    o2.lock().await.push(3);
                    Ok(())
                }
            })
            .await
            .unwrap();
        });

        h1.await.unwrap();
        h2.await.unwrap();

        let final_order = order.lock().await;
        // Operation 1 must complete (push 1, 2) before operation 2 starts (push 3)
        assert_eq!(*final_order, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_tab_lock_timeout() {
        let manager = Arc::new(TabLockManager::with_timeout(Duration::from_millis(50)));
        let tab_id = Uuid::new_v4();

        let m1 = Arc::clone(&manager);
        let _hold = tokio::spawn(async move {
            m1.with_tab_lock(tab_id, || async {
                // Hold the lock for longer than the timeout
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(())
            })
            .await
        });

        // Wait for the first task to acquire the lock
        tokio::time::sleep(Duration::from_millis(10)).await;

        let result: BrowserResult<()> = manager
            .with_tab_lock(tab_id, || async { Ok(()) })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, BrowserError::TabLocked(id) if id == tab_id),
            "Expected TabLocked error, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_remove_tab_cleanup() {
        let manager = TabLockManager::new();
        let tab_id = Uuid::new_v4();

        // Trigger lock creation
        manager
            .with_tab_lock(tab_id, || async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(manager.lock_count(), 1);

        manager.remove_tab(tab_id);
        assert_eq!(manager.lock_count(), 0);
    }

    #[tokio::test]
    async fn test_lock_reuse_across_operations() {
        let manager = TabLockManager::new();
        let tab_id = Uuid::new_v4();

        for i in 0..5 {
            let result: BrowserResult<u32> = manager
                .with_tab_lock(tab_id, || async move { Ok(i) })
                .await;
            assert_eq!(result.unwrap(), i);
        }

        // Only one lock should have been created
        assert_eq!(manager.lock_count(), 1);
    }
}
