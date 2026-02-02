//! Runtime-agnostic time utilities
//!
//! This module provides time-related functions that work with any async runtime.

use std::time::Duration;

/// Runtime-agnostic sleep function
#[cfg(feature = "runtime-tokio")]
pub async fn sleep(duration: Duration) {
    ::tokio::time::sleep(duration).await
}

#[cfg(feature = "runtime-smol")]
pub async fn sleep(duration: Duration) {
    ::smol::Timer::after(duration).await;
}

/// Runtime-agnostic timeout helper
///
/// Returns Ok(result) if the future completes within the duration,
/// or Err(()) if the timeout expires.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, ()>
where
    F: std::future::Future<Output = T>,
{
    #[cfg(feature = "runtime-tokio")]
    {
        ::tokio::time::timeout(duration, future)
            .await
            .map_err(|_| ())
    }

    #[cfg(feature = "runtime-smol")]
    {
        ::smol::future::or(
            async { Ok(future.await) },
            async {
                sleep(duration).await;
                Err(())
            },
        )
        .await
    }
}
