//! XDG Desktop Portal ScreenCast D-Bus session negotiation and keep-alive management.

use screencast_core::{t, tf, Error, Result, TextKey};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info};
use zbus::zvariant::{OwnedFd, OwnedObjectPath, OwnedValue, Value};

static TOKEN_SEQ: AtomicU64 = AtomicU64::new(1);

fn next_token(prefix: &str) -> String {
    let n = TOKEN_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("screencast_{prefix}_{}_{n}", std::process::id())
}

/// PipeWire stream metadata returned by the XDG ScreenCast portal.
///
/// `conn` MUST stay alive for as long as the stream is consumed: the portal ties the
/// ScreenCast session to the D-Bus connection that created it and tears the PipeWire
/// node down as soon as that connection closes.
#[derive(Debug)]
pub struct PortalStream {
    pub node_id: u32,
    pub fd: OwnedFd,
    pub session_handle: OwnedObjectPath,
    pub conn: zbus::Connection,
}

/// Map portal `Response` uint32 codes (`0`=success, `1`=cancelled, `2`=error) to localized errors.
pub fn map_portal_response(code: u32, stage: &str) -> Result<()> {
    match code {
        0 => Ok(()),
        1 => Err(Error::PortalDenied(tf(
            TextKey::PortalCancelled,
            &[("stage", stage)],
        ))),
        other => Err(Error::PortalDenied(tf(
            TextKey::PortalStageFailed,
            &[("stage", stage), ("code", &other.to_string())],
        ))),
    }
}

pub async fn portal_request<F, Fut>(
    conn: &zbus::Connection,
    token: &str,
    call: F,
) -> Result<(u32, HashMap<String, OwnedValue>)>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<OwnedObjectPath, zbus::Error>>,
{
    use zbus::MatchRule;

    let unique = conn
        .unique_name()
        .ok_or_else(|| Error::PortalDenied(t(TextKey::DbusUniqueName).into()))?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let expected_path = format!("/org/freedesktop/portal/desktop/request/{unique}/{token}");

    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")
        .map_err(|e| Error::PortalDenied(e.to_string()))?
        .member("Response")
        .map_err(|e| Error::PortalDenied(e.to_string()))?
        .path(expected_path.as_str())
        .map_err(|e| Error::PortalDenied(e.to_string()))?
        .build();

    let mut stream = zbus::MessageStream::for_match_rule(rule, conn, Some(8))
        .await
        .map_err(|e| {
            Error::PortalDenied(tf(TextKey::DbusSignalListen, &[("err", &e.to_string())]))
        })?;

    let req_path = call(token.to_string()).await.map_err(|e| {
        Error::PortalDenied(tf(TextKey::PortalCallFailed, &[("err", &e.to_string())]))
    })?;
    debug!("portal request path: {}", req_path.as_str());

    let msg = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        next_signal(&mut stream).await
    })
    .await
    .map_err(|_| Error::PortalDenied(t(TextKey::PortalTimeout).into()))?
    .ok_or_else(|| Error::PortalDenied(t(TextKey::PortalStreamClosed).into()))?;

    let body = msg.body();
    let (code, results): (u32, HashMap<String, OwnedValue>) = body.deserialize().map_err(|e| {
        Error::PortalDenied(tf(TextKey::PortalReplyDecode, &[("err", &e.to_string())]))
    })?;
    Ok((code, results))
}

async fn next_signal(stream: &mut zbus::MessageStream) -> Option<zbus::Message> {
    use std::pin::Pin;
    use std::task::{Context, Poll};
    std::future::poll_fn(|cx: &mut Context<'_>| {
        use zbus::export::ordered_stream::OrderedStream;
        match Pin::new(&mut *stream).poll_next_before(cx, None) {
            Poll::Ready(zbus::export::ordered_stream::PollResult::Item { data, .. }) => {
                Poll::Ready(data.ok())
            }
            Poll::Ready(zbus::export::ordered_stream::PollResult::Terminated) => Poll::Ready(None),
            Poll::Ready(zbus::export::ordered_stream::PollResult::NoneBefore) => Poll::Pending,
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

/// Negotiate an XDG ScreenCast portal session and return the PipeWire remote fd + node ID.
pub async fn open_screencast_portal() -> Result<PortalStream> {
    let (stream, _) = open_screencast_portal_with_token(None).await?;
    Ok(stream)
}

/// Negotiate an XDG ScreenCast portal session using an optional `restore_token`
/// (`persist_mode = 2`) so audio/quality restarts avoid re-prompting the user.
pub async fn open_screencast_portal_with_token(
    restore_token: Option<&str>,
) -> Result<(PortalStream, Option<String>)> {
    let conn = zbus::Connection::session().await.map_err(|e| {
        Error::PortalDenied(tf(TextKey::PortalDbusConnect, &[("err", &e.to_string())]))
    })?;

    let proxy = zbus::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.ScreenCast",
    )
    .await
    .map_err(|e| {
        Error::PortalDenied(tf(
            TextKey::PortalScreenCastMissing,
            &[("err", &e.to_string())],
        ))
    })?;

    let session_token = next_token("sess");
    let create_tok = next_token("create");
    let proxy_ref = &proxy;

    let (code, res) = portal_request(&conn, &create_tok, |tok| async move {
        let mut o = HashMap::new();
        o.insert("handle_token", Value::from(tok));
        o.insert("session_handle_token", Value::from(session_token));
        proxy_ref
            .call::<_, _, OwnedObjectPath>("CreateSession", &(o,))
            .await
    })
    .await?;
    map_portal_response(code, "CreateSession")?;

    let session_handle_str = res
        .get("session_handle")
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str<'_>>().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::PortalDenied(t(TextKey::PortalMissingSessionHandle).into()))?;
    let session_handle = OwnedObjectPath::try_from(session_handle_str).map_err(|e| {
        Error::PortalDenied(tf(
            TextKey::PortalInvalidSessionHandle,
            &[("err", &e.to_string())],
        ))
    })?;

    let sel_tok = next_token("select");
    let sh_ref = &session_handle;
    let restore_owned = restore_token.map(str::to_string);
    let (code, _) = portal_request(&conn, &sel_tok, |tok| async move {
        let mut o = HashMap::new();
        o.insert("handle_token", Value::from(tok));
        o.insert("types", Value::from(1u32 | 2u32));
        o.insert("multiple", Value::from(false));
        o.insert("cursor_mode", Value::from(2u32));
        o.insert("persist_mode", Value::from(2u32));
        if let Some(rt) = restore_owned {
            o.insert("restore_token", Value::from(rt));
        }
        proxy_ref
            .call::<_, _, OwnedObjectPath>("SelectSources", &(sh_ref, o))
            .await
    })
    .await?;
    map_portal_response(code, "SelectSources")?;

    let start_tok = next_token("start");
    let (code, start_res) = portal_request(&conn, &start_tok, |tok| async move {
        let mut o = HashMap::new();
        o.insert("handle_token", Value::from(tok));
        proxy_ref
            .call::<_, _, OwnedObjectPath>("Start", &(sh_ref, "", o))
            .await
    })
    .await?;
    map_portal_response(code, "Start")?;

    let next_restore_token = start_res
        .get("restore_token")
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str<'_>>().ok())
        .map(|s| s.to_string());

    let streams_val = start_res
        .get("streams")
        .and_then(|v| v.try_clone().ok())
        .ok_or_else(|| Error::PortalDenied(t(TextKey::PortalMissingStreams).into()))?;
    let streams: Vec<(u32, HashMap<String, OwnedValue>)> =
        Vec::<(u32, HashMap<String, OwnedValue>)>::try_from(streams_val).map_err(|e| {
            Error::PortalDenied(tf(TextKey::PortalStreamsDecode, &[("err", &e.to_string())]))
        })?;
    let node_id = streams
        .first()
        .map(|(id, _)| *id)
        .ok_or_else(|| Error::PortalDenied(t(TextKey::PortalEmptyStreams).into()))?;

    let empty: HashMap<&str, Value<'_>> = HashMap::new();
    let fd: OwnedFd = proxy
        .call("OpenPipeWireRemote", &(&session_handle, empty))
        .await
        .map_err(|e| {
            Error::PortalDenied(tf(TextKey::PortalPipewireFd, &[("err", &e.to_string())]))
        })?;

    info!("Wayland portal PipeWire node ready: node_id={node_id}");
    Ok((
        PortalStream {
            node_id,
            fd,
            session_handle,
            conn,
        },
        next_restore_token,
    ))
}

pub(crate) fn keep_portal_session_alive(child_pid: Option<u32>, portal: PortalStream) {
    let PortalStream {
        fd,
        session_handle,
        conn,
        ..
    } = portal;
    drop(fd);
    tokio::spawn(async move {
        if let Some(pid) = child_pid {
            let proc_path = format!("/proc/{pid}");
            while std::path::Path::new(&proc_path).exists() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
        let _ = conn
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                session_handle.as_str(),
                Some("org.freedesktop.portal.Session"),
                "Close",
                &(),
            )
            .await;
        drop(conn);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    #[test]
    fn portal_response_codes_give_localized_errors() {
        assert!(map_portal_response(0, "Start").is_ok());
        let denied = map_portal_response(1, "Start").unwrap_err();
        assert!(denied.to_string().contains("denied"));
        let err = map_portal_response(2, "CreateSession").unwrap_err();
        assert!(err.to_string().contains("code 2"));
    }

    #[tokio::test]
    #[ignore = "Requires live Wayland desktop session and interactive portal approval"]
    async fn live_portal_opens_pipewire_fd() {
        let stream = open_screencast_portal().await.expect("portal ok");
        assert!(stream.node_id > 0);
        assert!(stream.fd.as_raw_fd() >= 0);
    }
}
