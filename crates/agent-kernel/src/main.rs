//! NIAT Agent Kernel
//! Core system daemon: intent parser, planner, tool router, and permission manager.

mod model;
mod planner;
mod ipc;

use anyhow::Result;
use niat_common::config::NiatConfig;
use niat_common::types::McpConfig;
use model::ModelClient;
use planner::Planner;
use tool_runtime::ToolExecutor;
use mcp_client::McpClient;
use system_manager::SystemManager;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tracing::{info, error};

const SOCKET_PATH: &str = "/var/run/niat/agent.sock";

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();

    info!("NIAT Agent Kernel v{} starting", niat_common::types::NIAT_VERSION);

    // Load configuration (user config first, system/state/legacy fallbacks)
    let (config, _) = NiatConfig::load_persistent();
    info!(hostname = %config.system.hostname, "Configuration loaded");

    // Initialize system manager
    let sys_mgr = SystemManager::new();
    let status = sys_mgr.status();
    info!(
        kernel = %status.kernel_version,
        mem = format!("{}MB/{}MB", status.mem_used_mb, status.mem_total_mb),
        "System status collected"
    );

    // Initialize tool executor
    let tool_executor = ToolExecutor::new(config.security.require_confirmation);
    info!("Tool executor ready ({} tools registered)", tool_executor.list_tools().len());

    // Initialize MCP client
    let mut mcp = McpClient::new();
    let mcp_config = McpConfig::load(std::path::Path::new("/etc/niat/mcp.json"))?;
    mcp.init_from_config(&mcp_config).await?;
    info!("MCP client initialized");

    // Initialize model client
    let model_client = ModelClient::new(&config.model);
    let model_online = model_client.health_check().await;
    info!(online = model_online, model = %config.model.model, "Model provider status");

    // Create planner
    let planner = Planner::new(config.clone(), model_client, tool_executor);

    // Ensure socket directory exists
    let socket_dir = std::path::Path::new(SOCKET_PATH).parent().unwrap();
    std::fs::create_dir_all(socket_dir)?;

    // Remove stale socket
    let _ = std::fs::remove_file(SOCKET_PATH);

    // Start Unix domain socket listener for TUI communication
    let listener = UnixListener::bind(SOCKET_PATH)?;
    info!(path = SOCKET_PATH, "Agent kernel listening");

    // Accept connections from TUI
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                info!("TUI client connected");
                let planner_clone = planner.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, planner_clone).await {
                        error!(error = %e, "Client handler error");
                    }
                });
            }
            Err(e) => {
                error!(error = %e, "Accept error");
            }
        }
    }
}

async fn handle_client(
    stream: tokio::net::UnixStream,
    planner: Planner,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            info!("TUI client disconnected");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse incoming message from TUI
        match serde_json::from_str::<ipc::TuiRequest>(trimmed) {
            Ok(request) => {
                let response = planner.handle_request(request).await;
                let resp_json = serde_json::to_string(&response)? + "\n";
                writer.write_all(resp_json.as_bytes()).await?;
                writer.flush().await?;
            }
            Err(e) => {
                let err_resp = ipc::AgentResponse::Error {
                    message: format!("Invalid request: {}", e),
                };
                let resp_json = serde_json::to_string(&err_resp)? + "\n";
                writer.write_all(resp_json.as_bytes()).await?;
                writer.flush().await?;
            }
        }
    }
    Ok(())
}
