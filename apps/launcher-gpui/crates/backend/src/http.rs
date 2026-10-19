//! Port of `apps/launcher/electron/backend/http.ts`.

use std::time::Duration;

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct JsonResult {
    pub status: u16,
    pub body: serde_json::Value,
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(DEFAULT_REQUEST_TIMEOUT)
        .user_agent("IndustrialisLauncher/0.1")
        .build()
        .expect("reqwest client builds")
}

/// Mirrors `requestJson`: transport failures throw, HTTP error statuses are
/// returned for the caller to handle (never retried here).
pub async fn request_json(request: reqwest::RequestBuilder) -> Result<JsonResult, anyhow::Error> {
    let url = request
        .try_clone()
        .and_then(|r| r.build().ok())
        .map(|r| r.url().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    let response = request.timeout(DEFAULT_REQUEST_TIMEOUT).send().await.map_err(|e| {
        if e.is_timeout() {
            anyhow::anyhow!("request to {url} timed out after 30s. Check your internet connection and try again.")
        } else {
            anyhow::anyhow!("request to {url} failed ({e}). Check your internet connection and try again.")
        }
    })?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let body: serde_json::Value = if text.is_empty() {
        serde_json::Value::Object(Default::default())
    } else {
        serde_json::from_str(&text).unwrap_or(serde_json::json!({ "raw": text }))
    };
    Ok(JsonResult { status, body })
}

pub fn format_http_result(result: &JsonResult) -> String {
    format!("HTTP {}: {}", result.status, result.body)
}

/// HTTP statuses worth retrying or falling back on (rate-limit / transient).
pub fn is_transient_http_status(status: u16) -> bool {
    status == 408 || status == 429 || status >= 500
}

pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// Mirrors `retryWithBackoff`.
pub async fn retry_with_backoff<T, F, Fut>(
    mut operation: F,
    max_attempts: u32,
    base_delay: Duration,
) -> Result<T, anyhow::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
{
    assert!(max_attempts >= 1, "maxAttempts must be a positive integer");
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempt >= max_attempts {
                    return Err(error);
                }
                let delay = base_delay
                    .saturating_mul(2u32.saturating_pow(attempt - 1))
                    .min(Duration::from_secs(30));
                sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_transient_statuses() {
        assert!(is_transient_http_status(429));
        assert!(is_transient_http_status(503));
        assert!(!is_transient_http_status(200));
        assert!(!is_transient_http_status(404));
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let worker = attempts.clone();
        let result = retry_with_backoff(
            || {
                let worker = worker.clone();
                async move {
                    let n = worker.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n < 2 {
                        anyhow::bail!("boom");
                    }
                    Ok::<_, anyhow::Error>(n)
                }
            },
            3,
            Duration::from_millis(1),
        )
        .await
        .unwrap();
        assert_eq!(result, 2);
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    }
}
