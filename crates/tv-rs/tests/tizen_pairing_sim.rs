//! Tizen remote-channel pairing and token exit, against the mock server.
//!
//! The same contract the webOS slice proves for SSAP, on the Samsung wire:
//! first pairing goes through the prompt and issues a token, a stored token
//! pairs with no new prompt, a forged token is a denial rather than a hang,
//! key presses ride the channel, and the Tizen ≤4 app-enumeration event
//! answers with the playbook's recorded shape. The TLS variant additionally
//! proves the `TrustOnFirstUse` path against the mock's self-signed
//! certificate — the modern channel's own trust model.
//!
//! These tests drive the **real** `tv_rs_tizen` client over real sockets; the
//! only simulated part is the television, in `crates/sim/src/tizen_server.rs`.

use std::time::Duration;

use tv_rs_sim::MockTizenServer;
use tv_rs_tizen::{TizenClient, TizenError, TizenRemoteKey, TizenSession};
use tv_rs_transport::{cert_sink, CertFingerprint, TlsPolicy};

/// A client pointed at the mock's plain-WS port.
fn plain_client(addr: &std::net::SocketAddr) -> TizenClient {
    TizenClient::new(addr.ip().to_string())
        .pinned_port(addr.port())
        .with_handshake_timeout(Duration::from_secs(5))
}

#[tokio::test]
async fn first_pairing_prompts_and_issues_a_token() {
    let server = MockTizenServer::start_plain("127.0.0.1:0")
        .await
        .expect("mock binds");
    let client = plain_client(&server.local_addr());

    let pairing = client.pair().await.expect("pairing succeeds");
    assert_eq!(pairing.token.as_deref(), Some("mock-tizen-token-1"));
    assert_eq!(pairing.tv_name.as_deref(), Some("Mock Tizen TV"));

    assert_eq!(server.prompts_shown(), 1, "the first pairing prompts");
    assert_eq!(server.issued_tokens(), vec!["mock-tizen-token-1"]);
    assert!(
        server.presented_tokens().is_empty(),
        "nothing was presented"
    );
}

#[tokio::test]
async fn stored_token_pairs_without_a_prompt() {
    let server = MockTizenServer::start_plain("127.0.0.1:0")
        .await
        .expect("mock binds");

    // First pairing: prompt, token issued.
    let first = plain_client(&server.local_addr())
        .pair()
        .await
        .expect("pair");
    let token = first.token.expect("a token was issued");

    // Second connection with the stored token: no prompt, same token echoed.
    // (`pair()` clears the token by design — it is the first-run API; the
    // stored-token path is `TizenSession::connect`.)
    let client = plain_client(&server.local_addr()).with_token(token.clone());
    let session = TizenSession::connect(&client)
        .await
        .expect("reconnect succeeds");
    assert_eq!(
        session.token(),
        Some(token.as_str()),
        "the token was echoed"
    );
    drop(session);

    assert_eq!(server.prompts_shown(), 1, "the stored token never prompts");
    assert_eq!(server.issued_tokens().len(), 1, "nothing new was issued");
    assert_eq!(
        server.presented_tokens(),
        vec![token],
        "the token was presented"
    );
}

#[tokio::test]
async fn forged_token_is_a_denial_not_a_hang() {
    let server = MockTizenServer::start_plain("127.0.0.1:0")
        .await
        .expect("mock binds");

    // (`pair()` clears the token by design; a presented token rides
    // `TizenSession::connect`, which is where a stale one must fail.)
    let client = plain_client(&server.local_addr()).with_token("forged-token");
    let err = match TizenSession::connect(&client).await {
        Err(err) => err,
        Ok(_) => panic!("a forged token must be refused"),
    };
    assert!(
        matches!(err, TizenError::Unauthorized),
        "expected Unauthorized, got {err}"
    );
    assert_eq!(server.prompts_shown(), 0, "a presented token never prompts");
    assert_eq!(server.presented_tokens(), vec!["forged-token"]);
}

#[tokio::test]
async fn key_presses_ride_the_paired_channel() {
    let server = MockTizenServer::start_plain("127.0.0.1:0")
        .await
        .expect("mock binds");

    let client = plain_client(&server.local_addr()).with_token("mock-tizen-token-1");
    // The mock has not issued that token yet, so seed it through a pairing.
    let seeded = client.pair().await.expect("pair");
    let token = seeded.token.expect("token");

    let mut session = TizenSession::connect(&plain_client(&server.local_addr()).with_token(token))
        .await
        .expect("session opens with no new prompt");
    session
        .send_key(TizenRemoteKey::Mute)
        .await
        .expect("key sent");
    session
        .send_key(TizenRemoteKey::Home)
        .await
        .expect("key sent");
    drop(session);

    // Give the mock a moment to drain the frames the client queued.
    for _ in 0..50 {
        if server.key_commands().len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(
        server.key_commands(),
        vec!["KEY_MUTE".to_string(), "KEY_HOME".to_string()],
        "both key presses reached the channel in order"
    );
}

#[tokio::test]
async fn app_enumeration_answers_the_recorded_shape() {
    let server = MockTizenServer::start_plain("127.0.0.1:0")
        .await
        .expect("mock binds");

    let seeded = plain_client(&server.local_addr())
        .pair()
        .await
        .expect("pair");
    let token = seeded.token.expect("token");

    let mut session = TizenSession::connect(&plain_client(&server.local_addr()).with_token(token))
        .await
        .expect("session opens");
    // The G10b emit, exactly as the playbook records it.
    let emit =
        r#"{"method":"ms.channel.emit","params":{"event":"ed.installedApp.get","to":"host"}}"#;
    session.send_raw(emit).await.expect("emit sent");
    drop(session);

    for _ in 0..50 {
        if server.app_list_requests() >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        server.app_list_requests(),
        1,
        "the enumeration emit reached the channel"
    );
}

#[tokio::test]
async fn tofu_records_and_pins_the_mock_certificate() {
    // TLS is hard-wired to port 8002 in the client, so the mock must bind it.
    let server = match MockTizenServer::start_tls("127.0.0.1:8002").await {
        Ok(server) => server,
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            // Another test run (or a real service) owns 8002; skipping is the
            // honest outcome, and the plain-WS tests above still ran.
            eprintln!("skipping: port 8002 already in use");
            return;
        }
        Err(err) => panic!("mock tls bind failed: {err}"),
    };

    let host = server.local_addr().ip().to_string();
    let sink = cert_sink();
    let client = TizenClient::new(&host)
        .pinned_port(8002)
        .with_tls_policy(TlsPolicy::TrustOnFirstUse(sink.clone()))
        .with_handshake_timeout(Duration::from_secs(5));

    let pairing = client.pair().await.expect("TOFU pairing succeeds");
    assert_eq!(pairing.token.as_deref(), Some("mock-tizen-token-1"));

    // The sink recorded exactly the mock's self-signed certificate.
    let expected = CertFingerprint::of_der(server.cert_der());
    assert_eq!(
        TlsPolicy::TrustOnFirstUse(sink.clone()).recorded_fingerprint(),
        Some(expected),
        "TOFU recorded the mock certificate"
    );

    // The recorded pin re-connects cleanly, carrying the issued token the
    // way a real reconnect would.
    let pinned = expected.to_hex();
    let pin_client = TizenClient::new(&host)
        .pinned_port(8002)
        .with_tls_policy(TlsPolicy::pinned(pinned.trim()).expect("valid pin"))
        .with_token(pairing.token.expect("issued token"))
        .with_handshake_timeout(Duration::from_secs(5));
    let session = TizenSession::connect(&pin_client)
        .await
        .expect("pinned connect");
    drop(session);

    assert_eq!(server.prompts_shown(), 1, "only the first pairing prompted");
    server.shutdown().await;
}
