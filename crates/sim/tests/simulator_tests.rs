//! Integration tests for tv-rs-sim mock protocol servers.

use std::time::Duration;

use tv_rs_mdc::{MdcClient, MdcOptions, MdcPower};
use tv_rs_sim::mdc::MockMdcServer;
use tv_rs_sim::tizen::MockTizenServer;
use tv_rs_sim::webos::MockWebosServer;
use tv_rs_tizen::{TizenClient, TizenRemoteKey, TizenSession};
use tv_rs_webos::WebosClient;

#[tokio::test]
async fn test_mdc_simulator() {
    let server = MockMdcServer::start().await.expect("mdc server starts");
    let options = MdcOptions {
        port: server.port(),
        ..Default::default()
    };
    let client = MdcClient::with_options(server.addr().ip().to_string(), options);

    let power = client.power().await.expect("read power");
    assert_eq!(power, MdcPower::On);

    client.set_volume(45).await.expect("set volume");
    assert_eq!(server.state().volume, 45);

    client.set_brightness(75).await.expect("set brightness");
    assert_eq!(server.state().brightness, 75);
}

#[tokio::test]
async fn test_webos_simulator() {
    let server = MockWebosServer::start().await.expect("webos server starts");
    let client = WebosClient::new(server.addr().ip().to_string()).with_port(server.port());

    let mut session = client.connect().await.expect("webos connect");
    assert_eq!(session.client_key(), Some("sim-webos-client-key-12345"));

    let initial_vol = session.get_volume().await.expect("get volume");
    assert_eq!(initial_vol, 15);

    session.set_volume(42).await.expect("set volume");
    assert_eq!(server.state().volume, 42);

    session
        .launch_app("youtube.leanback.v4")
        .await
        .expect("launch app");
    assert_eq!(
        server.state().foreground_app.as_deref(),
        Some("youtube.leanback.v4")
    );
}

#[tokio::test]
async fn test_tizen_simulator() {
    let server = MockTizenServer::start().await.expect("tizen server starts");
    let client = TizenClient::new(server.addr().ip().to_string()).with_port(server.port());

    let mut session = TizenSession::connect(&client).await.expect("tizen connect");
    assert_eq!(session.token(), Some("sim-tizen-token-67890"));

    session
        .send_key(TizenRemoteKey::VolumeUp)
        .await
        .expect("send key");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.state().last_key.as_deref(), Some("KEY_VOLUP"));

    session.launch_app("1110110").await.expect("launch app");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.state().last_launched_app.as_deref(), Some("1110110"));
}
