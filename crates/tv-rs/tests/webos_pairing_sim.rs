//! webOS SSAP pairing exits (plan §3 slice 2) against the mock server.
//!
//! The A2 exit wants pairing proven without hardware: first pairing through
//! the prompt, a stored `client-key` pairing silently, command round-trips on
//! the paired session, and the prompt being dismissed. The mock speaks the
//! framing the client's recorded types parse — see [`MockWebOsServer`]'s docs
//! for what is and is not modelled (`MockWebOsServer` is re-exported from
//! `tv_rs_sim`).

use std::time::Duration;

use serde_json::json;
use tv_rs_sim::{MockWebOsServer, PromptAcceptance};
use tv_rs_webos::{SsapUri, WebosClient, WebosError};

async fn start_mock() -> (MockWebOsServer, String, u16) {
    let server = MockWebOsServer::start("127.0.0.1:0").await.expect("mock binds");
    let address = server.local_addr();
    let host = address.ip().to_string();
    (server, host, address.port())
}

#[tokio::test]
async fn first_pairing_goes_through_the_prompt_and_issues_a_key() {
    let (server, host, port) = start_mock().await;
    let client = WebosClient::new(&host).pinned_port(port);

    let pairing = client
        .pair()
        .await
        .expect("first pairing: the prompt is accepted");

    assert_eq!(pairing.client_key.as_deref(), Some("mock-client-key"));
    assert_eq!(pairing.device_name, "tv-rs", "the prompt name is the default");
    assert_eq!(server.prompts_shown(), 1, "the prompt was shown once");
    assert!(
        server.presented_client_keys().is_empty(),
        "a first pairing presents no key"
    );
    assert_eq!(server.issued_client_keys(), vec!["mock-client-key"]);
    server.shutdown().await;
}

#[tokio::test]
async fn a_stored_client_key_pairs_without_the_prompt() {
    let (server, host, port) = start_mock().await;
    let client = WebosClient::new(&host)
        .pinned_port(port)
        .with_client_key("already-paired");

    let pairing = client
        .pair()
        .await
        .expect("a presented key pairs silently");

    // The playbook's contract: same key confirmed back, no prompt.
    assert_eq!(pairing.client_key.as_deref(), Some("already-paired"));
    assert_eq!(server.prompts_shown(), 0, "a stored key never prompts");
    assert_eq!(server.presented_client_keys(), vec!["already-paired"]);
    assert!(
        server.issued_client_keys().is_empty(),
        "nothing was issued"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn commands_round_trip_on_the_paired_session() {
    let (server, host, port) = start_mock().await;
    let client = WebosClient::new(&host)
        .pinned_port(port)
        .with_client_key("already-paired");

    let mut session = client
        .connect()
        .await
        .expect("paired connect");
    let payload = session
        .request(SsapUri::AudioSetVolume, Some(json!({ "volume": 33 })))
        .await
        .expect("the mock answers the recorded envelope");

    assert_eq!(payload["returnValue"], json!(true));
    assert_eq!(
        server.commands(),
        vec!["ssap://audio/setVolume".to_string()],
        "the command arrived as its URI"
    );
    assert_eq!(server.prompts_shown(), 0);

    server.shutdown().await;
}

#[tokio::test]
async fn a_dismissed_prompt_is_a_registration_failure_not_a_hang() {
    let deny = MockWebOsServer::start_with_acceptance("127.0.0.1:0", PromptAcceptance::Deny)
        .await
        .expect("mock binds");
    let host = deny.local_addr().ip().to_string();
    let port = deny.local_addr().port();
    let client = WebosClient::new(&host).pinned_port(port);

    let error = client.pair().await.expect_err("a denied prompt fails pairing");
    match error {
        WebosError::RegistrationFailed {
            detail,
            presented_key,
        } => {
            assert!(detail.contains("pairing denied"), "got: {detail}");
            assert!(!presented_key, "nothing was presented on a first pairing");
        }
        other => panic!("expected RegistrationFailed, got {other:?}"),
    }
    assert_eq!(deny.prompts_shown(), 1);

    deny.shutdown().await;
}

#[tokio::test]
async fn the_prompt_delay_is_part_of_the_contract_against_a_late_acceptance() {
    // A real television stays silent while "Allow?" is on screen; the client
    // must survive that silence and still pair. The delay is short (2s) —
    // long enough to prove waiting, short enough for CI.
    let server = MockWebOsServer::start_with_acceptance(
        "127.0.0.1:0",
        PromptAcceptance::Accept {
            delay: Duration::from_secs(2),
            client_key: "slow-key".to_string(),
        },
    )
    .await
    .expect("mock binds");
    let host = server.local_addr().ip().to_string();
    let port = server.local_addr().port();
    let client = WebosClient::new(&host).pinned_port(port);

    let pairing = client
        .pair()
        .await
        .expect("the client waits through the prompt silence");
    assert_eq!(pairing.client_key.as_deref(), Some("slow-key"));
    assert_eq!(server.prompts_shown(), 1);

    server.shutdown().await;
}

#[tokio::test]
async fn the_client_key_is_never_leaked_to_the_prompt_path() {
    // A registered client-key must not travel through the prompt: the mock
    // only issues keys on the prompt path, so a presented key produces no
    // issued keys and no prompt.
    let (server, host, port) = start_mock().await;
    let client = WebosClient::new(&host)
        .pinned_port(port)
        .with_client_key("presented-only");

    let pairing = client
        .pair()
        .await
        .expect("a presented key pairs silently");

    assert_eq!(pairing.client_key.as_deref(), Some("presented-only"));
    assert_eq!(server.prompts_shown(), 0);
    assert_eq!(server.presented_client_keys(), vec!["presented-only"]);
    assert!(server.issued_client_keys().is_empty());

    server.shutdown().await;
}
