use akahu_mcp::AkahuServer;
use rmcp::{RoleServer, service::serve_directly, transport::stdio};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = AkahuServer::from_env().map_err(std::io::Error::other)?;

    if std::env::args().nth(1).as_deref() == Some("--health-check") {
        server.health_check().await.map_err(std::io::Error::other)?;
        return Ok(());
    }

    serve_directly::<RoleServer, _, _, _, _>(server, stdio(), None)
        .waiting()
        .await?;
    Ok(())
}
