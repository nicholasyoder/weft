use rmcp::transport::stdio;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = engine_mcp::WeftServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
