//! End-to-end test of the pairing handshake itself: a real TLS 1.3
//! connection using `TofuVerifier` on both sides, exporter keying material
//! pulled from the live session, and the resulting SAS codes compared.

use ms_security::pairing::derive_sas;
use ms_security::tls::pairing_configs;
use ms_security::DeviceIdentity;
use rustls::pki_types::ServerName;
use std::sync::Arc;
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

#[tokio::test]
async fn both_sides_of_a_real_handshake_derive_the_identical_sas() {
    let alice = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Alice-PC").unwrap();
    let bob = DeviceIdentity::generate(uuid::Uuid::new_v4(), "Bob-Mac").unwrap();

    let (alice_client_cfg, alice_client_verifier, _alice_server_cfg, _) = pairing_configs(&alice).unwrap();
    let (_bob_client_cfg, _, bob_server_cfg, bob_server_verifier) = pairing_configs(&bob).unwrap();

    let (client_sock, server_sock) = loopback_pair().await;
    let connector = TlsConnector::from(Arc::new(alice_client_cfg));
    let acceptor = TlsAcceptor::from(Arc::new(bob_server_cfg));
    let server_name = ServerName::try_from("mouse-share.local").unwrap();

    let (client_conn, server_conn) = tokio::join!(
        connector.connect(server_name, client_sock),
        acceptor.accept(server_sock),
    );
    let client_conn = client_conn.expect("TOFU handshake must succeed for any well-formed cert");
    let server_conn = server_conn.expect("TOFU handshake must succeed for any well-formed cert");

    // Exporter keying material must be pulled after the handshake
    // completes; both ends derive it independently from the same
    // negotiated secret, so they should agree without ever sending it over
    // the wire.
    let mut alice_exporter = [0u8; 32];
    client_conn
        .get_ref()
        .1
        .export_keying_material(&mut alice_exporter, b"mouse-share-pairing", None)
        .unwrap();
    let mut bob_exporter = [0u8; 32];
    server_conn
        .get_ref()
        .1
        .export_keying_material(&mut bob_exporter, b"mouse-share-pairing", None)
        .unwrap();
    assert_eq!(alice_exporter, bob_exporter, "both sides of one TLS session must export identical keying material");

    let alice_fp = alice.fingerprint();
    let bob_observed_alice_fp = bob_server_verifier.observed_fingerprint().expect("server must have observed a client cert");
    assert_eq!(alice_fp, bob_observed_alice_fp);

    let bob_fp = bob.fingerprint();
    let alice_observed_bob_fp = alice_client_verifier.observed_fingerprint().expect("client must have observed a server cert");
    assert_eq!(bob_fp, alice_observed_bob_fp);

    // Each side combines its *own* fingerprint with the peer fingerprint it
    // observed during the handshake (which is why the fingerprint
    // assertions above matter: they confirm both sides are talking about
    // the same pair of certs before the SAS comparison even happens).
    let alice_sas = derive_sas(&alice_exporter, &alice_fp, &alice_observed_bob_fp);
    let bob_sas = derive_sas(&bob_exporter, &bob_observed_alice_fp, &bob_fp);
    assert_eq!(alice_sas, bob_sas, "both parties must see the same short authentication string to confirm by eye");

    // Sanity: keep the connections alive until here so exporter material
    // isn't invalidated by an early drop/close on either side.
    drop((client_conn, server_conn));
}
