//! Commercial display integration tests against simulated hardware servers (MDC and SICP).

use std::time::Duration;
use tv_rs::{mdc_handle, probe, sicp_handle, PowerState, TvVendor};
use tv_rs_core::PowerState as CorePowerState;
use tv_rs_sicp::{DisplayId, SicpClient, SicpInput, SicpOptions, SicpPower};
use tv_rs_sim::{MockMdcServer, MockSicpServer};

#[tokio::test]
async fn test_mdc_simulated_panel_control() {
    let mdc_server = MockMdcServer::start("127.0.0.1:0").await.unwrap();
    let addr = mdc_server.local_addr().to_string();

    let handle = mdc_handle(&addr).expect("build mdc handle");
    let power = handle.as_power().expect("mdc supports power");

    let state = power.power_state().await.unwrap();
    assert_eq!(state, CorePowerState::On);

    power.sleep().await.unwrap();
    let state_after = power.power_state().await.unwrap();
    assert_eq!(state_after, CorePowerState::Off);

    mdc_server.shutdown().await;
}

#[tokio::test]
async fn test_sicp_simulated_panel_control() {
    let sicp_server = MockSicpServer::start("127.0.0.1:0").await.unwrap();
    let host = sicp_server.local_addr().ip().to_string();
    let port = sicp_server.local_addr().port();

    let client = SicpClient::with_options(
        host.clone(),
        SicpOptions {
            port,
            timeout: Duration::from_secs(2),
        },
    )
    .display(DisplayId::new(1).unwrap());

    let power = client.power().await.unwrap();
    assert_eq!(power, SicpPower::On);

    client.set_volume(75).await.unwrap();
    assert_eq!(client.volume().await.unwrap(), Some(75));

    client.set_input(SicpInput::Hdmi1).await.unwrap();
    assert_eq!(client.active_input().await.unwrap(), Some(SicpInput::Hdmi1));

    // Also verify erased SicpDevice handle
    let handle = sicp_handle(format!("{host}:{port}")).expect("build sicp handle");
    assert_eq!(handle.descriptor().address, format!("{host}:{port}"));

    sicp_server.shutdown().await;
}

#[tokio::test]
async fn test_sicp_facade_probing_and_client() {
    let sicp_server = MockSicpServer::start("127.0.0.1:0").await.unwrap();
    let addr = sicp_server.local_addr().to_string();

    let probed = probe(&addr).await;
    assert!(probed.is_some(), "probe should find sicp server");
    let tv = probed.unwrap();
    assert_eq!(tv.vendor, TvVendor::Philips);

    let client = tv.to_client().expect("build tv client for philips");
    assert_eq!(client.vendor(), TvVendor::Philips);

    let power_state = client.power_state().await.unwrap();
    assert_eq!(power_state, PowerState::On);

    client.power_off().await.unwrap();

    sicp_server.shutdown().await;
}
