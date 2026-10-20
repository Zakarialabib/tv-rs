//! Parser for GENA NOTIFY bodies (AVTransport `LastChange`).
//!
//! Separated from [`crate::gena`] because this is the subtle part and deserves to
//! be read on its own.
//!
//! # The doubly-encoded payload
//!
//! A renderer's NOTIFY body looks like this:
//!
//! ```xml
//! <e:propertyset xmlns:e="urn:schemas-upnp-org:event-1-0">
//!   <e:property>
//!     <LastChange>&lt;Event&gt;&lt;InstanceID val="0"&gt;
//!       &lt;TransportState val="PLAYING"/&gt;
//!     &lt;/InstanceID&gt;&lt;/Event&gt;</LastChange>
//!   </e:property>
//! </e:propertyset>
//! ```
//!
//! The `LastChange` text is an **escaped XML document**. Parsing the outer
//! envelope yields the literal string `<Event><InstanceID...` and nothing more;
//! the real state only appears after unescaping and parsing again. A parser that
//! forgets the second pass reports "no state change" for every event a renderer
//! ever sends, and the failure is silent.
//!
//! # And the values are in attributes
//!
//! Inner state is carried as `val="..."` **attributes** on self-closing elements,
//! not as element text. A text-content reader returns empty strings for all of
//! them.

use crate::gena::{TransportEvent, TransportState};

/// Parse a GENA NOTIFY body into a transport event.
///
/// Returns `None` when the body carries no AVTransport state at all — a renderer
/// may legitimately send a NOTIFY for properties we do not track, and that is
/// not an error.
pub fn parse_notify_body(xml: &str) -> Option<TransportEvent> {
    // Pass 1: pull the LastChange payload out of the propertyset.
    let inner_raw = extract_last_change(xml)?;

    // Pass 2: the payload is an escaped document — unescape, then read attributes.
    let inner = unescape(&inner_raw);

    let state = attr_of(&inner, "TransportState").map(|v| TransportState::parse(&v));

    let current_uri = attr_of(&inner, "CurrentTrackURI")
        .or_else(|| attr_of(&inner, "AVTransportURI"))
        .filter(|s| !s.is_empty());

    let medium = attr_of(&inner, "PlaybackStorageMedium").filter(|s| !s.is_empty());

    // Renderers disagree on which field carries the failure, so accept either.
    let error_code = attr_of(&inner, "TransportErrorCode")
        .or_else(|| attr_of(&inner, "TransportErrorDescription").map(|_| "1".to_string()))
        .and_then(|v| v.trim().parse::<u32>().ok());

    // A NOTIFY with no state and no URI tells us nothing actionable.
    if state.is_none() && current_uri.is_none() && error_code.is_none() {
        return None;
    }

    Some(TransportEvent {
        state: state.unwrap_or(TransportState::Unknown),
        current_uri,
        medium,
        error_code,
    })
}

/// Pull the raw (still-escaped) text of the `LastChange` element.
///
/// Matches on local name so `<LastChange>`, `<e:LastChange>` and
/// `<avt:LastChange>` all work.
fn extract_last_change(xml: &str) -> Option<String> {
    let open = find_open_tag(xml, "LastChange")?;
    let close = find_close_tag(xml, "LastChange", open)?;
    let raw = &xml[open..close];
    if raw.trim().is_empty() {
        return None;
    }
    Some(raw.to_string())
}

/// Byte offset just past the opening tag with this local name.
fn find_open_tag(xml: &str, local: &str) -> Option<usize> {
    let mut i = 0;
    while let Some(rel) = xml[i..].find('<') {
        let lt = i + rel;
        let after = lt + 1;
        if after >= xml.len() {
            return None;
        }
        let b = xml.as_bytes()[after];
        if b == b'/' || b == b'!' || b == b'?' {
            i = after;
            continue;
        }
        let name_end = xml[after..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map(|p| p + after)?;
        let qname = &xml[after..name_end];
        let gt = xml[name_end..].find('>').map(|p| p + name_end)?;
        if qname.rsplit(':').next().unwrap_or(qname) == local {
            return Some(gt + 1);
        }
        i = gt + 1;
    }
    None
}

/// Byte offset of the closing tag with this local name, searching from `from`.
fn find_close_tag(xml: &str, local: &str, from: usize) -> Option<usize> {
    let mut i = from;
    while let Some(rel) = xml[i..].find("</") {
        let at = i + rel;
        let after = at + 2;
        let name_end = xml[after..]
            .find(|c: char| c.is_whitespace() || c == '>')
            .map(|p| p + after)?;
        let qname = &xml[after..name_end];
        if qname.rsplit(':').next().unwrap_or(qname) == local {
            return Some(at);
        }
        i = name_end;
    }
    None
}

/// Read the `val` attribute of the first element with this local name.
///
/// The inner `LastChange` document uses `<TransportState val="PLAYING"/>`, so the
/// value is an attribute — reading element text yields nothing.
fn attr_of(xml: &str, element: &str) -> Option<String> {
    let mut i = 0;
    while let Some(rel) = xml[i..].find('<') {
        let lt = i + rel;
        let after = lt + 1;
        if after >= xml.len() {
            return None;
        }
        let b = xml.as_bytes()[after];
        if b == b'/' || b == b'!' || b == b'?' {
            i = after;
            continue;
        }
        let name_end = xml[after..]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .map(|p| p + after)?;
        let qname = &xml[after..name_end];
        let gt = xml[name_end..].find('>').map(|p| p + name_end)?;

        if qname.rsplit(':').next().unwrap_or(qname) == element {
            return read_val(&xml[name_end..gt]);
        }
        i = gt + 1;
    }
    None
}

/// Extract `val="..."` from an attribute region.
fn read_val(attrs: &str) -> Option<String> {
    let mut i = 0;
    while let Some(rel) = attrs[i..].find('=') {
        let eq = i + rel;
        let mut name_start = eq;
        while name_start > i && !attrs.as_bytes()[name_start - 1].is_ascii_whitespace() {
            name_start -= 1;
        }
        let name = attrs[name_start..eq].trim();

        let after_eq = &attrs[eq + 1..];
        let q_rel = after_eq.find(['"', '\''])?;
        let quote = after_eq.as_bytes()[q_rel] as char;
        let v_start = eq + 1 + q_rel + 1;
        let v_end = attrs[v_start..].find(quote).map(|p| p + v_start)?;

        if name.eq_ignore_ascii_case("val") {
            return Some(unescape(&attrs[v_start..v_end]));
        }
        i = v_end + 1;
    }
    None
}

/// Reverse XML entities, including numeric references.
///
/// Applied twice over the course of a parse: once to turn the escaped
/// `LastChange` text into a document, and again on attribute values inside it —
/// a track URI with a query string arrives as `&amp;amp;` and needs both passes
/// to come back as `&`.
fn unescape(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let Some(semi) = tail.find(';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        // A ';' far away is punctuation, not an entity terminator.
        if semi > 12 {
            out.push('&');
            rest = &tail[1..];
            continue;
        }
        match &tail[1..semi] {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            e if e.starts_with("#x") || e.starts_with("#X") => {
                match u32::from_str_radix(&e[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(c) => out.push(c),
                    None => out.push_str(&tail[..=semi]),
                }
            }
            e if e.starts_with('#') => match e[1..].parse::<u32>().ok().and_then(char::from_u32) {
                Some(c) => out.push(c),
                None => out.push_str(&tail[..=semi]),
            },
            _ => out.push_str(&tail[..=semi]),
        }
        rest = &tail[semi + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn unescapes_named_entities() {
        assert_eq!(unescape("a&amp;b"), "a&b");
        assert_eq!(unescape("&lt;x&gt;"), "<x>");
        assert_eq!(unescape("&quot;q&quot;"), "\"q\"");
        assert_eq!(unescape("single&apos;"), "single'");
    }

    #[test]
    fn unescapes_numeric_references() {
        assert_eq!(unescape("&#65;"), "A");
        assert_eq!(unescape("&#x41;"), "A");
    }

    #[test]
    fn leaves_lonely_ampersands_alone() {
        // A bare '&' with no entity terminator stays literal.
        assert_eq!(unescape("a & b"), "a & b");
    }

    #[test]
    fn a_far_semicolon_is_not_an_entity() {
        // Punctuation, not a terminator — must survive verbatim.
        assert_eq!(unescape("a;b & c;d"), "a;b & c;d");
    }
}
