pub mod embedded;
pub mod routes;
pub mod selector_gen;
pub mod server;
pub mod state;
pub mod ws;
pub mod yaml_gen;

use std::sync::Arc;

use tokio::net::TcpListener;
use velocity_common::PlatformDriver;

use state::AppState;

pub struct InspectorServer {
    state: Arc<AppState>,
}

impl InspectorServer {
    pub fn new(driver: Arc<dyn PlatformDriver>, device_id: Option<String>) -> Self {
        Self {
            state: Arc::new(AppState::new(driver, device_id)),
        }
    }

    pub async fn start(self, port: u16) -> anyhow::Result<()> {
        let router = server::build_router(self.state);
        let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;

        tracing::info!("Inspector server listening on http://localhost:{port}");

        axum::serve(listener, router).await?;

        Ok(())
    }
}
