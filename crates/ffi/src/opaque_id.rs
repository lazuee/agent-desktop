use std::sync::atomic::{AtomicUsize, Ordering};

use agent_desktop_core::{AdapterError, ErrorCode};

/// The atomic `try_update` is still unstable on the declared MSRV, so the deprecated
/// `fetch_update` stays until it stabilises.
#[allow(deprecated)]
pub(crate) fn allocate(
    counter: &AtomicUsize,
    resource: &'static str,
) -> Result<usize, AdapterError> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .map_err(|_| {
            AdapterError::new(
                ErrorCode::Internal,
                format!("{resource} identifier space exhausted"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_never_wraps_or_reuses_zero() {
        let counter = AtomicUsize::new(usize::MAX);
        assert!(allocate(&counter, "Test").is_err());
        assert_eq!(counter.load(Ordering::Relaxed), usize::MAX);
    }
}
