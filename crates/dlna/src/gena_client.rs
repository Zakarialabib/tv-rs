//! GENA HTTP client: SUBSCRIBE / renew / UNSUBSCRIBE.
//!
//! GENA uses three non-standard HTTP verbs against the renderer's `eventSubURL`:
//!
//! | Verb          | Headers                          | Purpose                     |
//! |---------------|----------------------------------|-----------------------------|
//! | `SUBSCRIBE`   | `CALLBACK`, `NT`, `TIMEOUT`      | Establish, returns `SID`    |
//! | `SUBSCRIBE`   | `SID`, `TIMEOUT` (no `CALLBACK`) | Renew an existing `SID`     |
//! | `UNSUBSCRIBE` | `SID`                            | Cancel cleanly              |
//!
//! The renew form is the one that trips people: sending `CALLBACK` **and** `SID`
//! together is a protocol error (`400 Bad Request`), and omitting `NT` on the
//! initial subscribe gets `412 Precondition Failed`. Both are encoded here so
//! callers cannot get it wrong.
//!
//! Kept separate from [`crate::gena`] so the pure builders and parsers there stay
//! free of `reqwest` and remain trivially testable.

use std::time::{Duration, SystemTime};

use crate::error::DlnaError;
use crate::gena::{callback_header, parse_timeout_header, timeout_header, Subscription};
use crate::models::Renderer;

/// Notification type for UPnP eventing — the only legal value.
const NT_EVENT: &str = "upnp:event";

/// How long to wait on a renderer that accepted the TCP connection but is slow
/// to answer. Deliberately shorter than the SOAP control timeout: a subscribe is
/// a bookkeeping call, and a renderer that cannot answer in 5s is not going to
/// deliver timely events either.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

/// Establish a new subscription.
///
/// `callback_url` must be an address the *renderer* can reach — a LAN IP, never
/// `localhost`. The renderer opens a fresh TCP connection back to it, so a
/// loopback callback silently yields a subscription that never delivers
/// anything.
pub async fn subscribe(event_sub_url: &str, callback_url: &str) -> Result<Subscription, DlnaError> {
    let client = build_client()?;

    let response = client
        .request(method("SUBSCRIBE")?, event_sub_url)
        .header("CALLBACK", callback_header(callback_url))
        .header("NT", NT_EVENT)
        .header("TIMEOUT", timeout_header())
        .send()
        .await
        .map_err(|e| DlnaError::SoapError(format!("SUBSCRIBE to {event_sub_url} failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(DlnaError::SoapError(format!(
            "SUBSCRIBE to {event_sub_url} returned HTTP {status}"
        )));
    }

    // The SID is mandatory in a success response. Without it we cannot renew or
    // cancel, so a 200 with no SID is a failure rather than a usable subscription.
    let sid = header_value(&response, "SID").ok_or_else(|| {
        DlnaError::SoapError(format!(
            "renderer at {event_sub_url} accepted SUBSCRIBE but returned no SID"
        ))
    })?;

    let timeout = parse_timeout_header(header_value(&response, "TIMEOUT").as_deref());

    Ok(Subscription {
        sid,
        callback_url: callback_url.to_string(),
        event_sub_url: event_sub_url.to_string(),
        timeout,
        established: SystemTime::now(),
    })
}

/// Subscribe to a discovered renderer's AVTransport events.
///
/// Convenience over [`subscribe`]: pulls the `event_sub_url` from the
/// [`Renderer`] discovered by [`crate::discover_renderers`].
pub async fn subscribe_renderer(
    renderer: &Renderer,
    callback_url: &str,
) -> Result<Subscription, DlnaError> {
    let event_sub_url = renderer.event_sub_url.as_deref().ok_or_else(|| {
        DlnaError::SoapError(format!(
            "renderer {} offers no AVTransport eventing",
            renderer.friendly_name
        ))
    })?;
    subscribe(event_sub_url, callback_url).await
}

/// Renew an existing subscription in place.
///
/// Sends `SID` + `TIMEOUT` and **no `CALLBACK`** — including the callback
/// alongside a SID is a protocol violation that renderers answer with `400`.
///
/// On success the returned `Subscription` carries a refreshed `established`
/// stamp and whatever timeout the renderer granted this time (which may differ
/// from the first grant). On failure the caller should re-`subscribe` from
/// scratch: a rejected renew usually means the renderer forgot the SID, e.g.
/// after a reboot.
pub async fn renew(subscription: &Subscription) -> Result<Subscription, DlnaError> {
    let client = build_client()?;

    let response = client
        .request(method("SUBSCRIBE")?, &subscription.event_sub_url)
        .header("SID", &subscription.sid)
        .header("TIMEOUT", timeout_header())
        .send()
        .await
        .map_err(|e| {
            DlnaError::SoapError(format!(
                "renew of {sid} failed: {e}",
                sid = subscription.sid
            ))
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(DlnaError::SoapError(format!(
            "renew of {sid} returned HTTP {status}",
            sid = subscription.sid
        )));
    }

    // A renderer may issue a NEW sid on renew; honour it when present.
    let sid = header_value(&response, "SID").unwrap_or_else(|| subscription.sid.clone());
    let timeout = parse_timeout_header(header_value(&response, "TIMEOUT").as_deref());

    Ok(Subscription {
        sid,
        callback_url: subscription.callback_url.clone(),
        event_sub_url: subscription.event_sub_url.clone(),
        timeout,
        established: SystemTime::now(),
    })
}

/// Cancel a subscription.
///
/// Best-effort by design — the caller is usually shutting down or dropping a
/// device, and a renderer that has already forgotten the SID answers `412`,
/// which is not a problem worth propagating. Errors are returned so a caller
/// *can* log them, but nothing should abort on this.
pub async fn unsubscribe(subscription: &Subscription) -> Result<(), DlnaError> {
    let client = build_client()?;

    let response = client
        .request(method("UNSUBSCRIBE")?, &subscription.event_sub_url)
        .header("SID", &subscription.sid)
        .send()
        .await
        .map_err(|e| DlnaError::SoapError(format!("UNSUBSCRIBE failed: {e}")))?;

    if !response.status().is_success() {
        return Err(DlnaError::SoapError(format!(
            "UNSUBSCRIBE returned HTTP {}",
            response.status()
        )));
    }
    Ok(())
}

/// Build the shared HTTP client.
fn build_client() -> Result<reqwest::Client, DlnaError> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| DlnaError::SoapError(format!("failed to build HTTP client: {e}")))
}

/// Parse one of GENA's extension verbs into a `reqwest::Method`.
fn method(name: &str) -> Result<reqwest::Method, DlnaError> {
    reqwest::Method::from_bytes(name.as_bytes())
        .map_err(|e| DlnaError::SoapError(format!("invalid HTTP method {name}: {e}")))
}

/// Read a response header as an owned `String`.
///
/// Header names are matched case-insensitively by `reqwest`, which matters here:
/// renderers send `SID`, `Sid` and `sid` interchangeably.
fn header_value(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delayed_verbs_parse() {
        // SUBSCRIBE / UNSUBSCRIBE are not standard HTTP methods; reqwest requires
        // explicit construction and we must not fall back to GET silently. Methods
        // with invalid token bytes must fail construction.
        assert_eq!(
            method("SUBSCRIBE").unwrap(),
            reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap()
        );
        assert!(method("SUB Scribe").is_err(), "spaces are not tokens");
        assert!(method("").is_err(), "empty is not a method");
    }
}
