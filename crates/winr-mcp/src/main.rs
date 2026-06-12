#[tokio::main]
async fn main() -> anyhow::Result<()> {
    winr_mcp::serve_stdio().await
}
