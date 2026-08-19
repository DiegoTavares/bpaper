//! The Gmail REST provider (spec `v9-gmail-backlog-capture.md` §10.2):
//! `labels.list` / `messages.list` / `messages.get`, MIME-tree walking,
//! base64url body decoding, RFC 2047 header decoding, and an honest HTML →
//! plain-text reduction. Read-only toward Google — no label is ever modified.

use anyhow::{Context as _, Result, anyhow, bail};
use base64::Engine as _;
use chrono::{DateTime, Local};
use futures::AsyncReadExt as _;
use gpui::{AsyncApp, Task};
use http_client::{AsyncBody, HttpClient, Request, http};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::gmail::{CapturedEmail, ImportMode, MailFetched, MailProvider, thread_marker_id};
use crate::google_auth::{AuthRevoked, GoogleClient, TokenKeeper, Unauthorized};

const API_BASE: &str = "https://gmail.googleapis.com/gmail/v1";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LabelsResponse {
    labels: Vec<GmailLabel>,
}

pub async fn list_labels(http: &Arc<dyn HttpClient>, access_token: &str) -> Result<Vec<GmailLabel>> {
    let body = get_json(http, &format!("{API_BASE}/users/me/labels"), access_token).await?;
    let response: LabelsResponse =
        serde_json::from_str(&body).context("failed to parse Gmail labels response")?;
    Ok(response.labels)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MessageRef {
    pub id: String,
    pub thread_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MessagesPage {
    messages: Vec<MessageRef>,
    next_page_token: Option<String>,
}

/// Every message currently carrying `label_id`, newest first (Gmail's
/// ordering), following `nextPageToken`. Ids only — bodies are fetched per
/// message.
pub async fn list_label_messages(
    http: &Arc<dyn HttpClient>,
    access_token: &str,
    label_id: &str,
) -> Result<Vec<MessageRef>> {
    let mut messages = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("labelIds", label_id)
            .append_pair("maxResults", "100");
        if let Some(token) = &page_token {
            query.append_pair("pageToken", token);
        }
        let url = format!("{API_BASE}/users/me/messages?{}", query.finish());
        let body = get_json(http, &url, access_token).await?;
        let page: MessagesPage =
            serde_json::from_str(&body).context("failed to parse Gmail messages response")?;
        messages.extend(page.messages);
        match page.next_page_token {
            Some(token) => page_token = Some(token),
            None => return Ok(messages),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct MessageHeader {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PartBody {
    data: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MessagePart {
    mime_type: String,
    headers: Vec<MessageHeader>,
    body: PartBody,
    parts: Vec<MessagePart>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct GmailMessage {
    id: String,
    thread_id: String,
    /// Milliseconds since the epoch, as a string.
    internal_date: Option<String>,
    payload: MessagePart,
}

async fn get_message(
    http: &Arc<dyn HttpClient>,
    access_token: &str,
    message_id: &str,
    mode: ImportMode,
) -> Result<GmailMessage> {
    let query = match mode {
        // Headers are all title mode needs.
        ImportMode::Title => {
            "format=metadata&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=Date"
                .to_string()
        }
        ImportMode::Full => "format=full".to_string(),
    };
    let url = format!(
        "{API_BASE}/users/me/messages/{}?{query}",
        url_path_escape(message_id)
    );
    let body = get_json(http, &url, access_token).await?;
    serde_json::from_str(&body).context("failed to parse Gmail message response")
}

fn url_path_escape(segment: &str) -> String {
    url::form_urlencoded::byte_serialize(segment.as_bytes()).collect()
}

/// Plain GET returning the body. 401 is typed as [`Unauthorized`] for the
/// retry-once-behind-refresh path; a 403 for a token without the Gmail scope
/// (a legacy V8 grant hand-copied into the unified slot) is [`AuthRevoked`],
/// so it degrades to a reconnect affordance instead of an endless retry.
async fn get_json(http: &Arc<dyn HttpClient>, url: &str, access_token: &str) -> Result<String> {
    let request = Request::builder()
        .method(http::Method::GET)
        .uri(url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Accept", "application/json")
        .body(AsyncBody::default())?;
    let mut response = http.send(request).await?;
    if response.status() == http::StatusCode::UNAUTHORIZED {
        return Err(anyhow!(Unauthorized));
    }
    let mut body = String::new();
    response.body_mut().read_to_string(&mut body).await?;
    if response.status() == http::StatusCode::FORBIDDEN
        && body.to_lowercase().contains("insufficient")
    {
        return Err(anyhow!(AuthRevoked));
    }
    if !response.status().is_success() {
        bail!("Gmail API request failed with status {}: {body}", response.status());
    }
    Ok(body)
}

fn header_value<'a>(payload: &'a MessagePart, name: &str) -> Option<&'a str> {
    payload
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

/// The first `text/plain` leaf of the MIME tree, falling back to the first
/// `text/html` leaf reduced to text (spec §5.4). Attachments and everything
/// else are ignored.
fn extract_text_body(payload: &MessagePart) -> Option<String> {
    fn find_leaf<'a>(part: &'a MessagePart, mime_type: &str) -> Option<&'a str> {
        if part.mime_type.eq_ignore_ascii_case(mime_type)
            && let Some(data) = part.body.data.as_deref()
            && !data.is_empty()
        {
            return Some(data);
        }
        part.parts.iter().find_map(|child| find_leaf(child, mime_type))
    }
    if let Some(data) = find_leaf(payload, "text/plain") {
        return decode_body_data(data);
    }
    let html = decode_body_data(find_leaf(payload, "text/html")?)?;
    Some(html_to_text(&html))
}

/// Gmail bodies are base64url, usually unpadded.
fn decode_body_data(data: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data.trim_end_matches('='))
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Decodes RFC 2047 encoded words (`=?charset?B|Q?…?=`) in a header value.
/// UTF-8 and Latin-1 are handled; anything else decodes lossily. Whitespace
/// between two adjacent encoded words is dropped, per the RFC.
fn decode_rfc2047(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    let mut previous_was_encoded = false;
    while let Some(start) = rest.find("=?") {
        let gap = &rest[..start];
        match decode_encoded_word(&rest[start..]) {
            Some((decoded, remainder)) => {
                let whitespace_between_words =
                    previous_was_encoded && gap.chars().all(char::is_whitespace);
                if !whitespace_between_words {
                    out.push_str(gap);
                }
                out.push_str(&decoded);
                previous_was_encoded = true;
                rest = remainder;
            }
            None => {
                // Not a real encoded word: emit up to and past the marker.
                out.push_str(gap);
                out.push_str("=?");
                previous_was_encoded = false;
                rest = &rest[start + 2..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One `=?charset?B|Q?payload?=` at the start of `text`; returns the decoded
/// word and what follows it.
fn decode_encoded_word(text: &str) -> Option<(String, &str)> {
    let inner = text.strip_prefix("=?")?;
    let (charset, inner) = inner.split_once('?')?;
    let (encoding, inner) = inner.split_once('?')?;
    let (payload, rest) = inner.split_once("?=")?;
    let bytes = match encoding {
        "B" | "b" => base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(payload.trim_end_matches('='))
            .ok()?,
        "Q" | "q" => decode_q_encoding(payload),
        _ => return None,
    };
    let decoded = if charset.eq_ignore_ascii_case("iso-8859-1") {
        bytes.iter().map(|&byte| byte as char).collect()
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    Some((decoded, rest))
}

fn decode_q_encoding(payload: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(payload.len());
    let mut chars = payload.bytes();
    while let Some(byte) = chars.next() {
        match byte {
            b'_' => bytes.push(b' '),
            b'=' => {
                let high = chars.next();
                let low = chars.next();
                let decoded = (|| {
                    let high = (high? as char).to_digit(16)?;
                    let low = (low? as char).to_digit(16)?;
                    Some((high * 16 + low) as u8)
                })();
                match decoded {
                    Some(value) => bytes.push(value),
                    None => bytes.push(b'='),
                }
            }
            other => bytes.push(other),
        }
    }
    bytes
}

/// Reduces HTML to honest plain text (spec §5.4): scripts and styles dropped,
/// block-level closings become line breaks, entities decoded, blank runs
/// collapsed. Not a Markdown conversion, deliberately.
fn html_to_text(html: &str) -> String {
    let html = strip_element(html, "script");
    let html = strip_element(&html, "style");
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html.as_str();
    while let Some(open) = rest.find('<') {
        push_entities_decoded(&mut out, &rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[open + 1..open + close];
        let name = tag
            .trim_start_matches('/')
            .split([' ', '\t', '\n', '/'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let closing = tag.starts_with('/');
        match name.as_str() {
            "br" | "hr" => out.push('\n'),
            "p" | "div" | "li" | "tr" | "ul" | "ol" | "blockquote" | "table" | "h1" | "h2"
            | "h3" | "h4" | "h5" | "h6" => {
                if closing {
                    out.push('\n');
                }
            }
            _ => {}
        }
        rest = &rest[open + close + 1..];
    }
    push_entities_decoded(&mut out, rest);

    // Trim every line and collapse runs of blanks to a single blank line.
    let mut result = String::with_capacity(out.len());
    let mut blank_run = 0usize;
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank_run += 1;
            continue;
        }
        if !result.is_empty() {
            result.push('\n');
            if blank_run > 0 {
                result.push('\n');
            }
        }
        blank_run = 0;
        result.push_str(line);
    }
    result
}

fn strip_element(html: &str, element: &str) -> String {
    let open = format!("<{element}");
    let close = format!("</{element}");
    let lower = html.to_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;
    while let Some(start) = lower[cursor..].find(&open) {
        let start = cursor + start;
        out.push_str(&html[cursor..start]);
        match lower[start..].find(&close) {
            Some(end) => {
                let end = start + end;
                cursor = lower[end..]
                    .find('>')
                    .map(|offset| end + offset + 1)
                    .unwrap_or(lower.len());
            }
            None => {
                cursor = lower.len();
            }
        }
    }
    out.push_str(&html[cursor..]);
    out
}

fn push_entities_decoded(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let candidate = &rest[start..];
        let entity_end = candidate[..candidate.len().min(12)].find(';');
        let Some(end) = entity_end else {
            out.push('&');
            rest = &candidate[1..];
            continue;
        };
        let entity = &candidate[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(character) => {
                out.push(character);
                rest = &candidate[end + 1..];
            }
            None => {
                out.push('&');
                rest = &candidate[1..];
            }
        }
    }
    out.push_str(rest);
}

/// [`MailProvider`] over the REST client: owns the token keeper and the
/// resolved label id. The refresh token stays in the keychain and is read on
/// demand — from the unified slot only, since a legacy calendar token lacks
/// the Gmail scope.
pub struct GoogleMailProvider {
    inner: Arc<MailInner>,
}

struct MailInner {
    http: Arc<dyn HttpClient>,
    keeper: TokenKeeper,
    account: String,
    label: String,
    label_id: Mutex<Option<String>>,
}

impl GoogleMailProvider {
    pub fn new(http: Arc<dyn HttpClient>, client: GoogleClient, account: String, label: String) -> Self {
        Self {
            inner: Arc::new(MailInner {
                http,
                keeper: TokenKeeper::new(client),
                account,
                label,

                label_id: Mutex::new(None),
            }),
        }
    }
}

impl MailProvider for GoogleMailProvider {
    fn fetch_labeled(
        &self,
        mode: ImportMode,
        skip: &HashSet<String>,
        cx: &AsyncApp,
    ) -> Task<Result<MailFetched>> {
        let inner = self.inner.clone();
        let skip = skip.clone();
        cx.spawn(async move |cx| inner.fetch(mode, &skip, cx).await)
    }
}

impl MailInner {
    async fn fetch(
        self: &Arc<Self>,
        mode: ImportMode,
        skip: &HashSet<String>,
        cx: &mut AsyncApp,
    ) -> Result<MailFetched> {
        let token = self.keeper.valid_access_token(&self.http, cx).await?;
        match self.fetch_with_token(mode, skip, &token).await {
            Err(error) if error.is::<Unauthorized>() => {
                // The token aged out server-side: refresh once and retry.
                self.keeper.invalidate_access_token();
                let token = self.keeper.valid_access_token(&self.http, cx).await?;
                match self.fetch_with_token(mode, skip, &token).await {
                    Err(error) if error.is::<Unauthorized>() => Err(anyhow!(AuthRevoked)),
                    other => other,
                }
            }
            other => other,
        }
    }

    async fn fetch_with_token(
        &self,
        mode: ImportMode,
        skip: &HashSet<String>,
        access_token: &str,
    ) -> Result<MailFetched> {
        let Some(label_id) = self.resolve_label_id(access_token).await? else {
            return Ok(MailFetched::LabelNotFound);
        };
        let refs = match list_label_messages(&self.http, access_token, &label_id).await {
            Ok(refs) => refs,
            Err(error) => {
                // The cached label may have been deleted; re-resolve next poll.
                if let Ok(mut cached) = self.label_id.lock() {
                    *cached = None;
                }
                return Err(error);
            }
        };

        // Gmail lists newest first, so the first message seen for a thread is
        // the one that represents it (spec §4.2).
        let mut seen_threads = HashSet::new();
        let mut wanted = Vec::new();
        for reference in refs {
            if !seen_threads.insert(reference.thread_id.clone()) {
                continue;
            }
            if skip.contains(&thread_marker_id(&self.account, &reference.thread_id)) {
                continue;
            }
            wanted.push(reference);
        }

        let mut emails = Vec::with_capacity(wanted.len());
        for reference in wanted {
            let message = get_message(&self.http, access_token, &reference.id, mode).await?;
            emails.push(captured_email(&message, mode, &reference.thread_id));
        }
        Ok(MailFetched::Emails(emails))
    }

    async fn resolve_label_id(&self, access_token: &str) -> Result<Option<String>> {
        if let Ok(cached) = self.label_id.lock()
            && let Some(id) = cached.clone()
        {
            return Ok(Some(id));
        }
        let labels = list_labels(&self.http, access_token).await?;
        let id = labels
            .into_iter()
            .find(|label| label.name.eq_ignore_ascii_case(&self.label))
            .map(|label| label.id);
        if let (Ok(mut cached), Some(id)) = (self.label_id.lock(), &id) {
            *cached = Some(id.clone());
        }
        Ok(id)
    }
}

fn captured_email(message: &GmailMessage, mode: ImportMode, thread_id: &str) -> CapturedEmail {
    let subject = header_value(&message.payload, "Subject")
        .map(decode_rfc2047)
        .unwrap_or_default();
    let from = header_value(&message.payload, "From")
        .map(decode_rfc2047)
        .unwrap_or_default();
    let date = message
        .internal_date
        .as_deref()
        .and_then(|millis| millis.parse::<i64>().ok())
        .and_then(|millis| DateTime::from_timestamp_millis(millis))
        .map(|instant| instant.with_timezone(&Local))
        .unwrap_or_else(Local::now);
    let body = match mode {
        ImportMode::Title => None,
        ImportMode::Full => extract_text_body(&message.payload),
    };
    CapturedEmail {
        thread_id: thread_id.to_string(),
        subject,
        from,
        date,
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use http_client::{FakeHttpClient, Response};

    fn base64url(text: &str) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text.as_bytes())
    }

    #[test]
    fn rfc2047_decoding() {
        assert_eq!(decode_rfc2047("plain subject"), "plain subject");
        assert_eq!(
            decode_rfc2047("=?UTF-8?B?SW52b2ljZSDinIU=?="),
            "Invoice ✅"
        );
        assert_eq!(
            decode_rfc2047("=?utf-8?Q?caf=C3=A9_menu?="),
            "café menu"
        );
        assert_eq!(
            decode_rfc2047("=?ISO-8859-1?Q?f=FCr_dich?="),
            "für dich"
        );
        // Whitespace between adjacent encoded words is dropped…
        assert_eq!(
            decode_rfc2047("=?UTF-8?B?YWI=?= =?UTF-8?B?Y2Q=?="),
            "abcd"
        );
        // …but real text between them survives.
        assert_eq!(
            decode_rfc2047("=?UTF-8?B?YWI=?= and =?UTF-8?B?Y2Q=?="),
            "ab and cd"
        );
        // A stray marker that isn't an encoded word passes through.
        assert_eq!(decode_rfc2047("worth =? nothing"), "worth =? nothing");
    }

    #[test]
    fn html_reduction() {
        let html = "<html><head><style>p { color: red }</style></head><body>\
                    <p>Hi &amp; welcome,</p><script>alert(1)</script>\
                    <div>Your invoice<br>is attached.</div>\
                    <ul><li>one</li><li>two &#8212; both</li></ul>\
                    <p>&nbsp;</p><p></p>&copy; Acme</body></html>";
        assert_eq!(
            html_to_text(html),
            "Hi & welcome,\nYour invoice\nis attached.\none\ntwo — both\n\n&copy; Acme"
        );
    }

    #[test]
    fn body_extraction_prefers_plain_text() {
        let multipart = MessagePart {
            mime_type: "multipart/alternative".to_string(),
            parts: vec![
                MessagePart {
                    mime_type: "text/html".to_string(),
                    body: PartBody {
                        data: Some(base64url("<p>rich</p>")),
                    },
                    ..Default::default()
                },
                MessagePart {
                    mime_type: "text/plain".to_string(),
                    body: PartBody {
                        data: Some(base64url("plain wins")),
                    },
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(extract_text_body(&multipart).as_deref(), Some("plain wins"));

        let html_only = MessagePart {
            mime_type: "text/html".to_string(),
            body: PartBody {
                data: Some(base64url("<p>only &lt;html&gt;</p>")),
            },
            ..Default::default()
        };
        assert_eq!(extract_text_body(&html_only).as_deref(), Some("only <html>"));

        let attachment_only = MessagePart {
            mime_type: "application/pdf".to_string(),
            ..Default::default()
        };
        assert_eq!(extract_text_body(&attachment_only), None);
    }

    #[test]
    fn provider_fetch_dedups_threads_and_skips_imported() {
        let http = FakeHttpClient::create(|request| async move {
            let uri = request.uri().to_string();
            let body = if uri.contains("/labels") {
                r#"{"labels": [
                    {"id": "Label_7", "name": "Backlog"},
                    {"id": "INBOX", "name": "INBOX"}
                ]}"#
                    .to_string()
            } else if uri.contains("/messages?") || uri.contains("labelIds") {
                assert!(uri.contains("labelIds=Label_7"), "{uri}");
                r#"{"messages": [
                    {"id": "m3", "threadId": "t-new"},
                    {"id": "m2", "threadId": "t-new"},
                    {"id": "m1", "threadId": "t-old"}
                ]}"#
                    .to_string()
            } else if uri.contains("/messages/m3") {
                assert!(uri.contains("format=metadata"), "{uri}");
                r#"{"id": "m3", "threadId": "t-new", "internalDate": "1755500000000",
                    "payload": {"headers": [
                        {"name": "Subject", "value": "=?UTF-8?Q?Caf=C3=A9?= plans"},
                        {"name": "From", "value": "Ana <ana@example.com>"}
                    ]}}"#
                    .to_string()
            } else {
                panic!("unexpected request to {uri}");
            };
            Ok(Response::builder()
                .status(200)
                .body(AsyncBody::from(body.into_bytes()))
                .unwrap())
        });
        let http: Arc<dyn HttpClient> = http;

        let inner = MailInner {
            http,
            keeper: TokenKeeper::new(GoogleClient {
                client_id: "id".to_string(),
                client_secret: None,
            }),
            account: "diego@example.com".to_string(),
            label: "backlog".to_string(),
            label_id: Mutex::new(None),
        };
        let skip: HashSet<String> =
            [thread_marker_id("diego@example.com", "t-old")].into_iter().collect();
        let fetched =
            block_on(inner.fetch_with_token(ImportMode::Title, &skip, "token")).unwrap();
        match fetched {
            MailFetched::Emails(emails) => {
                // t-new fetched once (newest message), t-old skipped without
                // a message request (the fake panics on /messages/m1).
                assert_eq!(emails.len(), 1);
                assert_eq!(emails[0].thread_id, "t-new");
                assert_eq!(emails[0].subject, "Café plans");
                assert_eq!(emails[0].from, "Ana <ana@example.com>");
                assert_eq!(emails[0].body, None);
            }
            MailFetched::LabelNotFound => panic!("expected emails"),
        }
    }

    #[test]
    fn missing_label_is_reported_not_errored() {
        let http = FakeHttpClient::create(|request| async move {
            let uri = request.uri().to_string();
            assert!(uri.contains("/labels"), "unexpected request to {uri}");
            Ok(Response::builder()
                .status(200)
                .body(AsyncBody::from(br#"{"labels": []}"#.to_vec()))
                .unwrap())
        });
        let http: Arc<dyn HttpClient> = http;
        let inner = MailInner {
            http,
            keeper: TokenKeeper::new(GoogleClient {
                client_id: "id".to_string(),
                client_secret: None,
            }),
            account: "diego@example.com".to_string(),
            label: "backlog".to_string(),
            label_id: Mutex::new(None),
        };
        let fetched =
            block_on(inner.fetch_with_token(ImportMode::Title, &HashSet::new(), "token")).unwrap();
        assert_eq!(fetched, MailFetched::LabelNotFound);
    }

    #[test]
    fn insufficient_scope_is_auth_revoked() {
        let http = FakeHttpClient::create(|_| async move {
            Ok(Response::builder()
                .status(403)
                .body(AsyncBody::from(
                    br#"{"error": {"status": "PERMISSION_DENIED",
                        "message": "Request had insufficient authentication scopes."}}"#
                        .to_vec(),
                ))
                .unwrap())
        });
        let http: Arc<dyn HttpClient> = http;
        let error = block_on(list_labels(&http, "token")).unwrap_err();
        assert!(error.is::<AuthRevoked>());
    }
}
