//! End-to-end tests exercising real TLS handshakes over a real TCP loopback
//! socket (not mocked crypto), proving the security properties the app
//! depends on: an unpaired device cannot complete a handshake, a paired
//! device can, and revocation immediately breaks a would-be reconnect.

use ms_security::tls::pinned_configs;
use ms_security::{DeviceIdentity, PairedDevice, TrustStore};
use rustls::pki_types::ServerName;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

async fn loopback_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accept = tokio::spawn(async move { listener.accept().await.unwrap().0 });
    let client = TcpStream::connect(addr).await.unwrap();
    let server = accept.await.unwrap();
    (client, server)
}

fn server_name() -> ServerName<'static> {
    ServerName::try_from("mouse-share.local").unwrap()
}

#[tokio::test]
async fn paired_devices_complete_a_mutual_tls_handshake() {
    let alice = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Alice-PC").unwrap();
    let bob = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Bob-Mac").unwrap();

    let mut alice_trust = TrustStore::new();
    alice_trust.add(PairedDevice {
        device_id: uuid::Uuid::new_v4(),
        name: "Bob-Mac".into(),
        fingerprint: bob.fingerprint(),
        paired_at_unix_ms: 0,
    });
    let mut bob_trust = TrustStore::new();
    bob_trust.add(PairedDevice {
        device_id: uuid::Uuid::new_v4(),
        name: "Alice-PC".into(),
        fingerprint: alice.fingerprint(),
        paired_at_unix_ms: 0,
    });

    let (alice_client_cfg, alice_server_cfg) = pinned_configs(&alice, Arc::new(Mutex::new(alice_trust))).unwrap();
    let (_bob_client_cfg, bob_server_cfg) = pinned_configs(&bob, Arc::new(Mutex::new(bob_trust))).unwrap();

    let (client_sock, server_sock) = loopback_pair().await;

    let connector = TlsConnector::from(Arc::new(alice_client_cfg));
    let acceptor = TlsAcceptor::from(Arc::new(bob_server_cfg));
    let _ = alice_server_cfg; // unused in this test; alice only acts as client here

    let client_fut = connector.connect(server_name(), client_sock);
    let server_fut = acceptor.accept(server_sock);

    let (client_res, server_res) = tokio::join!(client_fut, server_fut);
    assert!(client_res.is_ok(), "client (Alice) handshake should succeed: {:?}", client_res.err());
    assert!(server_res.is_ok(), "server (Bob) handshake should succeed: {:?}", server_res.err());
}

#[tokio::test]
async fn unpaired_device_cannot_complete_the_handshake() {
    let alice = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Alice-PC").unwrap();
    let stranger = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Unknown-Device").unwrap();

    // Bob's trust store only knows about Alice, not the stranger.
    let mut bob_trust = TrustStore::new();
    bob_trust.add(PairedDevice {
        device_id: uuid::Uuid::new_v4(),
        name: "Alice-PC".into(),
        fingerprint: alice.fingerprint(),
        paired_at_unix_ms: 0,
    });
    let bob = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Bob-Mac").unwrap();
    let (_bob_client_cfg, bob_server_cfg) = pinned_configs(&bob, Arc::new(Mutex::new(bob_trust))).unwrap();

    // The stranger's own trust store trusts nobody in particular, but that
    // doesn't matter: it's Bob's server-side verification of the
    // stranger's client cert that must reject the connection.
    let stranger_trust = TrustStore::new();
    let (stranger_client_cfg, _stranger_server_cfg) =
        pinned_configs(&stranger, Arc::new(Mutex::new(stranger_trust))).unwrap();

    let (client_sock, server_sock) = loopback_pair().await;
    let connector = TlsConnector::from(Arc::new(stranger_client_cfg));
    let acceptor = TlsAcceptor::from(Arc::new(bob_server_cfg));

    let client_fut = connector.connect(server_name(), client_sock);
    let server_fut = acceptor.accept(server_sock);
    let (client_res, server_res) = tokio::join!(client_fut, server_fut);

    assert!(server_res.is_err(), "Bob must reject a client certificate it never paired with");
    assert!(client_res.is_err(), "the handshake must fail end-to-end, not just server-side bookkeeping");
}

#[tokio::test]
async fn revoked_device_is_rejected_on_the_next_connection_attempt() {
    let alice = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Alice-PC").unwrap();
    let bob = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Bob-Mac").unwrap();

    let bob_trust = Arc::new(Mutex::new(TrustStore::new()));
    bob_trust.lock().unwrap().add(PairedDevice {
        device_id: uuid::Uuid::new_v4(),
        name: "Alice-PC".into(),
        fingerprint: alice.fingerprint(),
        paired_at_unix_ms: 0,
    });

    let alice_trust = Arc::new(Mutex::new(TrustStore::new()));
    alice_trust.lock().unwrap().add(PairedDevice {
        device_id: uuid::Uuid::new_v4(),
        name: "Bob-Mac".into(),
        fingerprint: bob.fingerprint(),
        paired_at_unix_ms: 0,
    });

    let (alice_client_cfg, _alice_server_cfg) = pinned_configs(&alice, alice_trust).unwrap();
    let (_bob_client_cfg, bob_server_cfg) = pinned_configs(&bob, bob_trust.clone()).unwrap();

    // First connection succeeds while Alice is still paired.
    {
        let (client_sock, server_sock) = loopback_pair().await;
        let connector = TlsConnector::from(Arc::new(alice_client_cfg.clone()));
        let acceptor = TlsAcceptor::from(Arc::new(bob_server_cfg.clone()));
        let (client_res, server_res) = tokio::join!(
            connector.connect(server_name(), client_sock),
            acceptor.accept(server_sock)
        );
        assert!(client_res.is_ok() && server_res.is_ok());
    }

    // Revoke Alice on Bob's side, then a fresh connection attempt must fail.
    bob_trust.lock().unwrap().revoke(&alice.fingerprint());
    {
        let (client_sock, server_sock) = loopback_pair().await;
        let connector = TlsConnector::from(Arc::new(alice_client_cfg));
        let acceptor = TlsAcceptor::from(Arc::new(bob_server_cfg));
        let (client_res, server_res) = tokio::join!(
            connector.connect(server_name(), client_sock),
            acceptor.accept(server_sock)
        );
        // The server — the party actually deciding whether to admit the
        // connection and start forwarding input events — must reject a
        // revoked client certificate. This is the property that matters:
        // it's what `ms-core-service` checks before treating a session as
        // live. (In TLS 1.3 the client sends its Certificate *after* it
        // already received the server's Finished, so the client's own
        // handshake future can resolve successfully a moment before the
        // server's rejection alert arrives — that's expected, not a gap,
        // since the server never proceeds to relay any protocol traffic.)
        assert!(server_res.is_err(), "revoked device must be rejected on the next handshake");
        if let Ok(mut client_tls) = client_res {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let _ = client_tls.write_all(b"ping").await;
            let mut buf = [0u8; 1];
            let read_result = client_tls.read(&mut buf).await;
            assert!(
                matches!(read_result, Ok(0) | Err(_)),
                "client must observe the connection being torn down, not a live session"
            );
        }
    }
}
