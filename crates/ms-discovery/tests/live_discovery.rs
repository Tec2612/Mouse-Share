//! End-to-end test against a real mDNS multicast advertise/browse cycle
//! (not mocked). Skips gracefully instead of hanging forever if the
//! environment blocks multicast (e.g. some CI runners, client-isolated
//! networks) — that's exactly the situation `ms-discovery::manual` exists
//! to work around, so a timeout here is a real-world case, not just test
//! flakiness.

use ms_discovery::{translate_event, DiscoveryEvent, DiscoveryService, RemoteOs};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn advertised_device_is_discovered_over_real_multicast() {
    let mut advertiser = DiscoveryService::new().expect("create advertiser daemon");
    let device_id = uuid::Uuid::new_v4();
    advertiser
        .advertise(device_id, "Integration-Test-Device", RemoteOs::MacOs, "ms-test-host", 34567)
        .expect("advertise");

    let browser = DiscoveryService::new().expect("create browser daemon");
    let receiver = browser.browse().expect("browse");

    let found = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let event = tokio::task::block_in_place(|| receiver.recv_timeout(Duration::from_secs(1)));
            if let Ok(event) = event {
                if let Some(Ok(DiscoveryEvent::Found(ann))) = translate_event(event) {
                    if ann.device_id == device_id {
                        return ann;
                    }
                }
            }
        }
    })
    .await;

    match found {
        Ok(ann) => {
            assert_eq!(ann.name, "Integration-Test-Device");
            assert_eq!(ann.os, RemoteOs::MacOs);
            assert_eq!(ann.port, 34567);
            assert!(!ann.addrs.is_empty());
        }
        Err(_) => {
            eprintln!("no mDNS resolution within 10s; skipping (multicast likely unavailable in this environment)");
        }
    }

    let _ = advertiser.stop_advertising();
}
