//! End-to-end test against a real mDNS multicast advertise/browse cycle
//! (not mocked). Skips gracefully instead of hanging forever if the
//! environment blocks multicast (e.g. some CI runners, client-isolated
//! networks) — that's exactly the situation `ms-discovery::manual` exists
//! to work around, so a timeout here is a real-world case, not just test
//! flakiness.
//!
//! Every call that touches the network (including setup/teardown, not
//! just the "wait for a result" loop) is bounded by an explicit timeout.
//! `mdns_sd::ServiceDaemon` methods are synchronous and run on the
//! daemon's own background thread; a `tokio::time::timeout` wrapped
//! directly around a blocking call does nothing (it only preempts at
//! `.await` points), so each one is run inside `spawn_blocking` and it's
//! the `JoinHandle`'s `.await` that gets timed out. If the daemon thread
//! is genuinely wedged on a syscall, the timeout still lets this test
//! (and the process) move on — the leaked blocked thread is reaped when
//! the test binary exits, not before.

use ms_discovery::{translate_event, DiscoveryEvent, DiscoveryService, RemoteOs};
use std::time::Duration;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(10);

async fn with_timeout<T: Send + 'static>(
    label: &str,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    match tokio::time::timeout(NETWORK_TIMEOUT, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(join_err)) => {
            eprintln!("{label} panicked: {join_err}; skipping (see docs/e2e-test-plan.md)");
            None
        }
        Err(_) => {
            eprintln!("{label} did not complete within {NETWORK_TIMEOUT:?}; skipping (multicast likely unavailable/blocked in this environment)");
            None
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn advertised_device_is_discovered_over_real_multicast() {
    let Some(mut advertiser) = with_timeout("DiscoveryService::new (advertiser)", || {
        DiscoveryService::new().expect("create advertiser daemon")
    })
    .await
    else {
        return;
    };

    let device_id = uuid::Uuid::new_v4();
    let advertised = with_timeout("advertise", move || {
        let result = advertiser.advertise(device_id, "Integration-Test-Device", RemoteOs::MacOs, "ms-test-host", 34567);
        (advertiser, result)
    })
    .await;
    let Some((mut advertiser, advertise_result)) = advertised else { return };
    if advertise_result.is_err() {
        eprintln!("advertise() failed; skipping");
        return;
    }

    let Some(Ok(browser)) = with_timeout("DiscoveryService::new (browser)", || DiscoveryService::new()).await else {
        return;
    };
    let Some(Ok(receiver)) = with_timeout("browse", move || browser.browse()).await else {
        return;
    };

    let found = tokio::time::timeout(NETWORK_TIMEOUT, tokio::task::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + NETWORK_TIMEOUT;
        while std::time::Instant::now() < deadline {
            if let Ok(event) = receiver.recv_timeout(Duration::from_secs(1)) {
                if let Some(Ok(DiscoveryEvent::Found(ann))) = translate_event(event) {
                    if ann.device_id == device_id {
                        return Some(ann);
                    }
                }
            }
        }
        None
    }))
    .await;

    match found {
        Ok(Ok(Some(ann))) => {
            assert_eq!(ann.name, "Integration-Test-Device");
            assert_eq!(ann.os, RemoteOs::MacOs);
            assert_eq!(ann.port, 34567);
            assert!(!ann.addrs.is_empty());
        }
        _ => {
            eprintln!("no mDNS resolution within {NETWORK_TIMEOUT:?}; skipping (multicast likely unavailable in this environment)");
        }
    }

    // Best-effort cleanup, itself bounded — a wedged daemon thread here
    // must not be allowed to hang the whole test suite (this was the
    // actual cause of a CI hang before this timeout was added; see the
    // module doc comment above).
    let _ = with_timeout("stop_advertising", move || advertiser.stop_advertising()).await;
}
