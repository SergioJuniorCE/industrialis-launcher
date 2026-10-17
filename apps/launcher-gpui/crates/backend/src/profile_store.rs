//! One in-process transaction boundary for profile read/modify/write operations.
//! Never hold this across network requests or game/maintenance work. Cross-process
//! ownership belongs to the single-instance coordinator, not this mutex.

use anyhow::Context;
use std::path::Path;

static WRITES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn flush() {
    let _guard = WRITES.lock().await;
}

pub(crate) async fn mutate<T: Send + 'static>(
    work: impl std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
) -> anyhow::Result<T> {
    // The transaction owns its task. Dropping the caller cannot release the
    // mutex while a blocking atomic replacement is still completing.
    tokio::spawn(async move {
        let _lease = WRITES.lock().await;
        work.await
    })
    .await?
}

/// Missing is a new profile; unreadable/invalid is a recoverable error. Never
/// turn a damaged document into defaults and then overwrite the original.
pub(crate) async fn read_document(path: &Path) -> anyhow::Result<Option<serde_json::Value>> {
    match tokio::fs::read(path).await {
        Ok(raw) => serde_json::from_slice(&raw)
            .with_context(|| {
                format!(
                    "Invalid JSON in {}. The original file has been kept.",
                    path.display()
                )
            })
            .map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| {
            format!(
                "Could not read {}. The original file has been kept.",
                path.display()
            )
        }),
    }
}

pub(crate) async fn read_object(path: &Path) -> anyhow::Result<Option<serde_json::Value>> {
    let value = read_document(path).await?;
    if value.as_ref().is_some_and(|v| !v.is_object()) {
        anyhow::bail!(
            "Expected an object in {}. The original file has been kept.",
            path.display()
        );
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn cancelling_the_caller_does_not_drop_an_admitted_transaction() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let (started, start) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let first_order = order.clone();
        let caller = tokio::spawn(async move {
            mutate(async move {
                started.send(()).unwrap();
                wait.await.unwrap();
                first_order.lock().unwrap().push(1);
                Ok(())
            })
            .await
        });
        start.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let second_order = order.clone();
        let next = tokio::spawn(async move {
            mutate(async move {
                second_order.lock().unwrap().push(2);
                Ok(())
            })
            .await
        });
        release.send(()).unwrap();
        next.await.unwrap().unwrap();
        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }
}
