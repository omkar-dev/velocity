use tracing::debug;
use velocity_common::{Result, VelocityError};

use crate::wda::WdaClient;

/// Manages the WDA client lifecycle, creating sessions on demand and reusing them
/// across tests for the same device.
pub struct WdaManager {
    wda_client: WdaClient,
    device_id: Option<String>,
}

impl WdaManager {
    /// Create a new manager targeting a WDA instance at the given base URL.
    pub fn new(wda_base_url: &str) -> Self {
        Self {
            wda_client: WdaClient::new(wda_base_url),
            device_id: None,
        }
    }

    /// Ensure a healthy WDA session exists for the given device and bundle.
    /// If a session already exists and is healthy, it is reused. Otherwise a new
    /// session is created.
    pub async fn ensure_session(&mut self, device_id: &str, bundle_id: &str) -> Result<()> {
        // If we have a session for the same device, check if it's still alive
        if self.wda_client.session_id().is_some() {
            if self.device_id.as_deref() == Some(device_id) {
                if self.health_check().await.unwrap_or(false) {
                    debug!(device_id, "reusing existing WDA session");
                    return Ok(());
                }
                debug!(device_id, "existing WDA session unhealthy, recreating");
            } else {
                debug!(
                    old_device = self.device_id.as_deref().unwrap_or("none"),
                    new_device = device_id,
                    "device changed, creating new WDA session"
                );
                // Try to clean up old session
                let _ = self.wda_client.delete_session().await;
            }
        }

        self.wda_client.create_session(bundle_id).await?;
        self.device_id = Some(device_id.to_string());
        Ok(())
    }

    /// Check if the WDA instance is responsive.
    pub async fn health_check(&self) -> Result<bool> {
        self.wda_client.health_check().await
    }

    /// Get a reference to the underlying WDA client.
    pub fn client(&self) -> &WdaClient {
        &self.wda_client
    }

    /// Get a mutable reference to the underlying WDA client.
    pub fn client_mut(&mut self) -> &mut WdaClient {
        &mut self.wda_client
    }

    /// Force-invalidate the current session so the next operation creates a fresh one.
    pub fn invalidate_session(&mut self) {
        // Clear the session ID without sending a delete request — useful when
        // the session is already lost and we want to recreate on next use.
        self.wda_client.set_session_id_opt(None);
        self.device_id = None;
    }

    /// Delete the current session if one exists. Called during cleanup.
    pub async fn teardown(&mut self) -> Result<()> {
        if self.wda_client.session_id().is_some() {
            self.wda_client.delete_session().await?;
            self.device_id = None;
        }
        Ok(())
    }
}

impl WdaManager {
    /// Attempt to recover a lost session by creating a fresh one.
    pub async fn recover_session(
        &mut self,
        device_id: &str,
        bundle_id: &str,
        test_name: &str,
        attempt: u32,
        max_attempts: u32,
    ) -> Result<()> {
        debug!(
            device_id,
            test_name, attempt, max_attempts, "attempting WDA session recovery"
        );

        // Clean up old session state
        let _ = self.wda_client.delete_session().await;

        match self.wda_client.create_session(bundle_id).await {
            Ok(_) => {
                self.device_id = Some(device_id.to_string());
                debug!(device_id, "WDA session recovered");
                Ok(())
            }
            Err(_) => Err(VelocityError::WdaSessionLost {
                test_name: test_name.to_string(),
                attempt,
                max: max_attempts,
            }),
        }
    }
}
