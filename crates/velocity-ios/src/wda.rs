use std::time::{Duration, Instant};

use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::debug;
use velocity_common::{Result, VelocityError};

/// A reference to an element found by WDA.
#[derive(Debug, Clone)]
pub struct WdaElement {
    pub element_id: String,
}

/// Cached hierarchy source with TTL.
struct SourceCache {
    xml: String,
    fetched_at: Instant,
    ttl: Duration,
}

impl SourceCache {
    fn is_valid(&self) -> bool {
        self.fetched_at.elapsed() < self.ttl
    }
}

/// HTTP client for communicating with WebDriverAgent.
///
/// v2 improvements:
/// - Connection pooling via reqwest (keep-alive, idle pool)
/// - Hierarchy source caching with configurable TTL
/// - Warm-up method for pre-establishing connections
pub struct WdaClient {
    base_url: String,
    session_id: Option<String>,
    client: Client,
    source_cache: Mutex<Option<SourceCache>>,
    cache_ttl: Duration,
}

impl WdaClient {
    pub fn new(base_url: &str) -> Self {
        // Configure client with connection pooling and keep-alive
        let client = Client::builder()
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(Duration::from_secs(60))
            .tcp_keepalive(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            session_id: None,
            client,
            source_cache: Mutex::new(None),
            cache_ttl: Duration::from_millis(500),
        }
    }

    /// Set the hierarchy source cache TTL.
    pub fn set_cache_ttl(&mut self, ttl: Duration) {
        self.cache_ttl = ttl;
    }

    /// Invalidate the cached hierarchy source.
    pub async fn invalidate_source_cache(&self) {
        let mut cache = self.source_cache.lock().await;
        *cache = None;
    }

    /// Pre-establish a connection to WDA and verify it's responsive.
    pub async fn warm_up(&self) -> Result<()> {
        self.health_check().await?;
        debug!("WDA connection warmed up");
        Ok(())
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn set_session_id(&mut self, id: String) {
        self.session_id = Some(id);
    }

    pub fn set_session_id_opt(&mut self, id: Option<String>) {
        self.session_id = id;
    }

    fn session_url(&self) -> Result<String> {
        let sid = self.session_id.as_deref().ok_or_else(|| {
            VelocityError::Config("No active WDA session".to_string())
        })?;
        Ok(format!("{}/session/{}", self.base_url, sid))
    }

    fn extract_error(body: &Value) -> String {
        body.get("value")
            .and_then(|v| v.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown WDA error")
            .to_string()
    }

    /// Create a new WDA session for the given bundle ID.
    /// Returns the session ID.
    pub async fn create_session(&mut self, bundle_id: &str) -> Result<String> {
        let url = format!("{}/session", self.base_url);
        let payload = json!({
            "capabilities": {
                "alwaysMatch": {
                    "bundleId": bundle_id
                }
            }
        });

        debug!(bundle_id, "creating WDA session");

        let resp = self.client.post(&url).json(&payload).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA session request failed: {e}"))
        })?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| {
            VelocityError::Config(format!("Failed to parse WDA session response: {e}"))
        })?;

        if !status.is_success() {
            return Err(VelocityError::Config(format!(
                "WDA create session failed ({}): {}",
                status,
                Self::extract_error(&body)
            )));
        }

        let session_id = body
            .get("value")
            .and_then(|v| v.get("sessionId"))
            .or_else(|| body.get("sessionId"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                VelocityError::Config(format!(
                    "WDA session response missing sessionId: {body}"
                ))
            })?
            .to_string();

        debug!(session_id = %session_id, "WDA session created");
        self.session_id = Some(session_id.clone());
        Ok(session_id)
    }

    /// Delete the current WDA session.
    pub async fn delete_session(&mut self) -> Result<()> {
        let url = self.session_url()?;
        debug!("deleting WDA session");

        let resp = self.client.delete(&url).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA delete session failed: {e}"))
        })?;

        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or(json!({}));
            return Err(VelocityError::Config(format!(
                "WDA delete session failed: {}",
                Self::extract_error(&body)
            )));
        }

        self.session_id = None;
        Ok(())
    }

    /// Find a single element matching the given strategy and value.
    pub async fn find_element(&self, using: &str, value: &str) -> Result<WdaElement> {
        let url = format!("{}/element", self.session_url()?);
        let payload = json!({ "using": using, "value": value });

        debug!(using, value, "WDA find element");

        let resp = self.client.post(&url).json(&payload).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA find element request failed: {e}"))
        })?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| {
            VelocityError::Config(format!("Failed to parse WDA find element response: {e}"))
        })?;

        if !status.is_success() {
            return Err(VelocityError::ElementNotFound {
                selector: format!("{using}={value}"),
                timeout_ms: 0,
                screenshot: None,
                hierarchy_snapshot: None,
            });
        }

        let element_id = extract_element_id(&body["value"])?;
        Ok(WdaElement { element_id })
    }

    /// Find all elements matching the given strategy and value.
    pub async fn find_elements(&self, using: &str, value: &str) -> Result<Vec<WdaElement>> {
        let url = format!("{}/elements", self.session_url()?);
        let payload = json!({ "using": using, "value": value });

        debug!(using, value, "WDA find elements");

        let resp = self.client.post(&url).json(&payload).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA find elements request failed: {e}"))
        })?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| {
            VelocityError::Config(format!("Failed to parse WDA find elements response: {e}"))
        })?;

        if !status.is_success() {
            return Err(VelocityError::Config(format!(
                "WDA find elements failed ({}): {}",
                status,
                Self::extract_error(&body)
            )));
        }

        let elements = body["value"]
            .as_array()
            .ok_or_else(|| {
                VelocityError::Config("WDA find elements response is not an array".to_string())
            })?
            .iter()
            .filter_map(|v| extract_element_id(v).ok())
            .map(|element_id| WdaElement { element_id })
            .collect();

        Ok(elements)
    }

    /// Click on an element by its element ID.
    pub async fn click(&self, element_id: &str) -> Result<()> {
        let url = format!("{}/element/{}/click", self.session_url()?, element_id);
        debug!(element_id, "WDA click");

        let resp = self.client.post(&url).json(&json!({})).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA click failed: {e}"))
        })?;

        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or(json!({}));
            return Err(VelocityError::Config(format!(
                "WDA click failed: {}",
                Self::extract_error(&body)
            )));
        }

        Ok(())
    }

    /// Send keys (type text) into an element.
    pub async fn send_keys(&self, element_id: &str, text: &str) -> Result<()> {
        let url = format!("{}/element/{}/value", self.session_url()?, element_id);
        let payload = json!({ "value": text.chars().map(|c| c.to_string()).collect::<Vec<_>>() });

        debug!(element_id, text, "WDA send keys");

        let resp = self.client.post(&url).json(&payload).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA send keys failed: {e}"))
        })?;

        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or(json!({}));
            return Err(VelocityError::Config(format!(
                "WDA send keys failed: {}",
                Self::extract_error(&body)
            )));
        }

        Ok(())
    }

    /// Clear an element's text content.
    pub async fn clear(&self, element_id: &str) -> Result<()> {
        let url = format!("{}/element/{}/clear", self.session_url()?, element_id);
        debug!(element_id, "WDA clear");

        let resp = self.client.post(&url).json(&json!({})).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA clear failed: {e}"))
        })?;

        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or(json!({}));
            return Err(VelocityError::Config(format!(
                "WDA clear failed: {}",
                Self::extract_error(&body)
            )));
        }

        Ok(())
    }

    /// Get the visible text of an element.
    pub async fn get_text(&self, element_id: &str) -> Result<String> {
        let url = format!("{}/element/{}/text", self.session_url()?, element_id);
        debug!(element_id, "WDA get text");

        let resp = self.client.get(&url).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA get text failed: {e}"))
        })?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| {
            VelocityError::Config(format!("Failed to parse WDA get text response: {e}"))
        })?;

        if !status.is_success() {
            return Err(VelocityError::Config(format!(
                "WDA get text failed ({}): {}",
                status,
                Self::extract_error(&body)
            )));
        }

        Ok(body["value"].as_str().unwrap_or("").to_string())
    }

    /// Check whether an element is currently displayed.
    pub async fn is_displayed(&self, element_id: &str) -> Result<bool> {
        let url = format!("{}/element/{}/displayed", self.session_url()?, element_id);
        debug!(element_id, "WDA is displayed");

        let resp = self.client.get(&url).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA is_displayed failed: {e}"))
        })?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| {
            VelocityError::Config(format!("Failed to parse WDA displayed response: {e}"))
        })?;

        if !status.is_success() {
            return Err(VelocityError::Config(format!(
                "WDA is_displayed failed ({}): {}",
                status,
                Self::extract_error(&body)
            )));
        }

        Ok(body["value"].as_bool().unwrap_or(false))
    }

    /// Get the full XML source hierarchy from WDA (cached).
    /// Returns cached result if within TTL, otherwise fetches fresh.
    pub async fn get_source(&self) -> Result<String> {
        // Check cache first
        {
            let cache = self.source_cache.lock().await;
            if let Some(ref cached) = *cache {
                if cached.is_valid() {
                    debug!("WDA get source (cache hit)");
                    return Ok(cached.xml.clone());
                }
            }
        }

        let xml = self.get_source_fresh().await?;

        // Update cache
        {
            let mut cache = self.source_cache.lock().await;
            *cache = Some(SourceCache {
                xml: xml.clone(),
                fetched_at: Instant::now(),
                ttl: self.cache_ttl,
            });
        }

        Ok(xml)
    }

    /// Get the full XML source hierarchy from WDA, bypassing cache.
    /// Used by the sync engine which needs fresh data every cycle.
    pub async fn get_source_fresh(&self) -> Result<String> {
        let url = format!("{}/source", self.base_url);
        debug!("WDA get source (fresh)");

        let resp = self.client.get(&url).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA get source failed: {e}"))
        })?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| {
            VelocityError::Config(format!("Failed to parse WDA source response: {e}"))
        })?;

        if !status.is_success() {
            return Err(VelocityError::Config(format!(
                "WDA get source failed ({}): {}",
                status,
                Self::extract_error(&body)
            )));
        }

        Ok(body["value"].as_str().unwrap_or("").to_string())
    }

    /// Take a screenshot via WDA, returning decoded PNG bytes.
    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let url = format!("{}/screenshot", self.base_url);
        debug!("WDA screenshot");

        let resp = self.client.get(&url).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA screenshot failed: {e}"))
        })?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| {
            VelocityError::Config(format!("Failed to parse WDA screenshot response: {e}"))
        })?;

        if !status.is_success() {
            return Err(VelocityError::Config(format!(
                "WDA screenshot failed ({}): {}",
                status,
                Self::extract_error(&body)
            )));
        }

        let b64 = body["value"].as_str().ok_or_else(|| {
            VelocityError::Config("WDA screenshot response missing base64 data".to_string())
        })?;

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| {
                VelocityError::Config(format!("Failed to decode screenshot base64: {e}"))
            })?;

        Ok(bytes)
    }

    /// Get the screen (window) size as (width, height).
    pub async fn get_screen_size(&self) -> Result<(i32, i32)> {
        let url = format!("{}/window/size", self.session_url()?);
        debug!("WDA get screen size");

        let resp = self.client.get(&url).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA get screen size failed: {e}"))
        })?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| {
            VelocityError::Config(format!("Failed to parse WDA screen size response: {e}"))
        })?;

        if !status.is_success() {
            return Err(VelocityError::Config(format!(
                "WDA get screen size failed ({}): {}",
                status,
                Self::extract_error(&body)
            )));
        }

        let width = body["value"]["width"]
            .as_i64()
            .unwrap_or(390) as i32;
        let height = body["value"]["height"]
            .as_i64()
            .unwrap_or(844) as i32;

        Ok((width, height))
    }

    /// Perform a swipe (drag) gesture from one point to another over a duration.
    pub async fn swipe(
        &self,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        duration: f64,
    ) -> Result<()> {
        let url = format!("{}/wda/dragfromtoforduration", self.session_url()?);
        let payload = json!({
            "fromX": from_x,
            "fromY": from_y,
            "toX": to_x,
            "toY": to_y,
            "duration": duration
        });

        debug!(from_x, from_y, to_x, to_y, duration, "WDA swipe");

        let resp = self.client.post(&url).json(&payload).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA swipe failed: {e}"))
        })?;

        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or(json!({}));
            return Err(VelocityError::Config(format!(
                "WDA swipe failed: {}",
                Self::extract_error(&body)
            )));
        }

        Ok(())
    }

    /// Tap at screen coordinates (zero-duration drag).
    pub async fn tap_at(&self, x: f64, y: f64) -> Result<()> {
        self.swipe(x, y, x, y, 0.0).await
    }

    /// Type text into the currently focused element (session-level keys).
    pub async fn type_text(&self, text: &str) -> Result<()> {
        let url = format!("{}/wda/keys", self.session_url()?);
        let payload = json!({ "value": text.chars().map(|c| c.to_string()).collect::<Vec<_>>() });
        debug!(text, "WDA type text (session-level)");

        let resp = self.client.post(&url).json(&payload).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA type text failed: {e}"))
        })?;

        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or(json!({}));
            return Err(VelocityError::Config(format!(
                "WDA type text failed: {}",
                Self::extract_error(&body)
            )));
        }

        Ok(())
    }

    /// Press a hardware button (e.g. "home", "volumeUp").
    pub async fn press_button(&self, button: &str) -> Result<()> {
        let url = format!("{}/wda/pressButton", self.session_url()?);
        let payload = json!({ "name": button });

        debug!(button, "WDA press button");

        let resp = self.client.post(&url).json(&payload).send().await.map_err(|e| {
            VelocityError::Config(format!("WDA press button failed: {e}"))
        })?;

        if !resp.status().is_success() {
            let body: Value = resp.json().await.unwrap_or(json!({}));
            return Err(VelocityError::Config(format!(
                "WDA press button failed: {}",
                Self::extract_error(&body)
            )));
        }

        Ok(())
    }

    /// Check if WDA is healthy and responsive.
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/status", self.base_url);
        debug!("WDA health check");

        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };

        Ok(resp.status().is_success())
    }
}

/// Extract the element ID from a WDA element response value.
/// WDA returns element IDs under a key like "ELEMENT" or "element-6066-...".
fn extract_element_id(value: &Value) -> Result<String> {
    // W3C WebDriver uses the key "element-6066-11e4-a52e-4f735466cecf"
    const W3C_ELEMENT_KEY: &str = "element-6066-11e4-a52e-4f735466cecf";

    if let Some(id) = value.get(W3C_ELEMENT_KEY).and_then(|v| v.as_str()) {
        return Ok(id.to_string());
    }

    // Legacy JSONWP uses "ELEMENT"
    if let Some(id) = value.get("ELEMENT").and_then(|v| v.as_str()) {
        return Ok(id.to_string());
    }

    // Try any string value in the object
    if let Some(obj) = value.as_object() {
        for (_, v) in obj {
            if let Some(id) = v.as_str() {
                return Ok(id.to_string());
            }
        }
    }

    Err(VelocityError::Config(format!(
        "Could not extract element ID from WDA response: {value}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_element_id_w3c() {
        let value = json!({
            "element-6066-11e4-a52e-4f735466cecf": "abc-123"
        });
        assert_eq!(extract_element_id(&value).unwrap(), "abc-123");
    }

    #[test]
    fn test_extract_element_id_legacy() {
        let value = json!({ "ELEMENT": "def-456" });
        assert_eq!(extract_element_id(&value).unwrap(), "def-456");
    }

    #[test]
    fn test_extract_element_id_missing() {
        let value = json!({});
        assert!(extract_element_id(&value).is_err());
    }

    #[test]
    fn test_wda_client_no_session() {
        let client = WdaClient::new("http://localhost:8100");
        assert!(client.session_id().is_none());
        assert!(client.session_url().is_err());
    }

    #[test]
    fn test_wda_client_session_url() {
        let mut client = WdaClient::new("http://localhost:8100");
        client.set_session_id("test-session-123".to_string());
        assert_eq!(
            client.session_url().unwrap(),
            "http://localhost:8100/session/test-session-123"
        );
    }

    #[test]
    fn test_wda_client_trailing_slash() {
        let client = WdaClient::new("http://localhost:8100/");
        let mut client = client;
        client.set_session_id("s1".to_string());
        assert_eq!(
            client.session_url().unwrap(),
            "http://localhost:8100/session/s1"
        );
    }
}
