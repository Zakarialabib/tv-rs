//! Simulator runner for tv-rs mock hardware servers (Samsung MDC :1515, Philips SICP :5000).

use tv_rs_sim::{MockMdcServer, MockSicpServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mdc_addr = std::env::var("MDC_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:1515".to_string());
    let sicp_addr = std::env::var("SICP_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:5000".to_string());

    let mdc_server = MockMdcServer::start(&mdc_addr).await?;
    println!(
        "Mock Samsung MDC Server listening on {}",
        mdc_server.local_addr()
    );

    let sicp_server = MockSicpServer::start(&sicp_addr).await?;
    println!(
        "Mock Philips SICP Server listening on {}",
        sicp_server.local_addr()
    );

    println!("Hardware simulation servers active. Press Ctrl+C to terminate.");
    tokio::signal::ctrl_c().await?;

    println!("Shutting down mock hardware servers...");
    mdc_server.shutdown().await;
    sicp_server.shutdown().await;

    Ok(())
}
