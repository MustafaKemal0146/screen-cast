//! Wayland rootless remote input injection via `org.freedesktop.portal.RemoteDesktop`.

use super::keymap::{char_to_xkb_keysym, map_web_key_to_xkb_keysym, mouse_button_to_evdev};
use super::XdotoolControl;
use crate::wayland::{map_portal_response, portal_request};
use screencast_core::{t, tf, Control, ControlEvent, Error, Result, TextKey};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use tracing::{debug, info};
use zbus::zvariant::{OwnedObjectPath, Value};

static RD_TOKEN_SEQ: AtomicU64 = AtomicU64::new(1);

fn next_rd_token(prefix: &str) -> String {
    let n = RD_TOKEN_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("screencast_rd_{prefix}_{}_{n}", std::process::id())
}

struct RemoteDesktopSession {
    _conn: zbus::Connection,
    proxy: zbus::Proxy<'static>,
    session_handle: OwnedObjectPath,
    last_x: f64,
    last_y: f64,
}

/// Wayland rootless remote input backend via `org.freedesktop.portal.RemoteDesktop` (T-M7-03).
/// Lazily negotiates a portal session on first input event, with XWayland `xdotool` fallback.
pub struct WaylandPortalControl {
    pub screen_width: u32,
    pub screen_height: u32,
    session: Mutex<Option<RemoteDesktopSession>>,
}

impl std::fmt::Debug for WaylandPortalControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaylandPortalControl")
            .field("screen_width", &self.screen_width)
            .field("screen_height", &self.screen_height)
            .finish()
    }
}

impl Default for WaylandPortalControl {
    fn default() -> Self {
        Self::new(1920, 1080)
    }
}

impl WaylandPortalControl {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            screen_width: width.max(1),
            screen_height: height.max(1),
            session: Mutex::new(None),
        }
    }

    async fn open_session() -> Result<RemoteDesktopSession> {
        let conn = zbus::Connection::session().await.map_err(|e| {
            Error::PortalDenied(tf(TextKey::PortalDbusConnect, &[("err", &e.to_string())]))
        })?;

        let proxy = zbus::Proxy::new(
            &conn,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.RemoteDesktop",
        )
        .await
        .map_err(|e| {
            Error::PortalDenied(tf(
                TextKey::RemoteDesktopMissing,
                &[("err", &e.to_string())],
            ))
        })?;

        let sess_tok = next_rd_token("sess");
        let create_tok = next_rd_token("create");
        let p_ref = &proxy;
        let (code, res) = portal_request(&conn, &create_tok, |tok| async move {
            let mut o = HashMap::new();
            o.insert("handle_token", Value::from(tok));
            o.insert("session_handle_token", Value::from(sess_tok));
            p_ref
                .call::<_, _, OwnedObjectPath>("CreateSession", &(o,))
                .await
        })
        .await?;
        map_portal_response(code, "RemoteDesktop CreateSession")?;

        let sh_str = res
            .get("session_handle")
            .and_then(|v| v.downcast_ref::<zbus::zvariant::Str<'_>>().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| Error::PortalDenied(t(TextKey::RemoteDesktopSessionHandle).into()))?;
        let session_handle = OwnedObjectPath::try_from(sh_str).map_err(|e| {
            Error::PortalDenied(tf(
                TextKey::PortalInvalidSessionHandle,
                &[("err", &e.to_string())],
            ))
        })?;

        let sel_tok = next_rd_token("select");
        let sh_ref = &session_handle;
        let (code, _) = portal_request(&conn, &sel_tok, |tok| async move {
            let mut o = HashMap::new();
            o.insert("handle_token", Value::from(tok));
            o.insert("types", Value::from(1u32 | 2u32));
            o.insert("persist_mode", Value::from(2u32));
            p_ref
                .call::<_, _, OwnedObjectPath>("SelectDevices", &(sh_ref, o))
                .await
        })
        .await?;
        map_portal_response(code, "RemoteDesktop SelectDevices")?;

        let start_tok = next_rd_token("start");
        let (code, start_res) = portal_request(&conn, &start_tok, |tok| async move {
            let mut o = HashMap::new();
            o.insert("handle_token", Value::from(tok));
            p_ref
                .call::<_, _, OwnedObjectPath>("Start", &(sh_ref, "", o))
                .await
        })
        .await?;
        map_portal_response(code, "RemoteDesktop Start")?;

        let devices = start_res
            .get("devices")
            .and_then(|v| v.downcast_ref::<u32>().ok())
            .unwrap_or(0);
        info!("Wayland RemoteDesktop portal session ready (devices={devices:#x})");

        Ok(RemoteDesktopSession {
            _conn: conn,
            proxy,
            session_handle,
            last_x: 0.5,
            last_y: 0.5,
        })
    }

    async fn apply_portal(&self, event: &ControlEvent) -> Result<()> {
        let mut guard = self.session.lock().await;
        if guard.is_none() {
            *guard = Some(Self::open_session().await?);
        }
        let sess = guard.as_mut().unwrap();
        let empty: HashMap<&str, Value<'_>> = HashMap::new();

        match event {
            ControlEvent::Move { x, y } => {
                let nx = x.clamp(0.0, 1.0);
                let ny = y.clamp(0.0, 1.0);
                let dx = (nx - sess.last_x) * (self.screen_width as f64);
                let dy = (ny - sess.last_y) * (self.screen_height as f64);
                sess.last_x = nx;
                sess.last_y = ny;

                let px = nx * (self.screen_width as f64);
                let py = ny * (self.screen_height as f64);
                if sess
                    .proxy
                    .call::<_, _, ()>(
                        "NotifyPointerMotionAbsolute",
                        &(&sess.session_handle, &empty, 0u32, px, py),
                    )
                    .await
                    .is_err()
                {
                    sess.proxy
                        .call::<_, _, ()>(
                            "NotifyPointerMotion",
                            &(&sess.session_handle, &empty, dx, dy),
                        )
                        .await
                        .map_err(|e| {
                            Error::PortalDenied(tf(
                                TextKey::RemoteDesktopPointer,
                                &[("err", &e.to_string())],
                            ))
                        })?;
                }
            }
            ControlEvent::Click {
                button,
                x,
                y,
                double,
            } => {
                let nx = x.clamp(0.0, 1.0);
                let ny = y.clamp(0.0, 1.0);
                let dx = (nx - sess.last_x) * (self.screen_width as f64);
                let dy = (ny - sess.last_y) * (self.screen_height as f64);
                let px = nx * (self.screen_width as f64);
                let py = ny * (self.screen_height as f64);
                sess.last_x = nx;
                sess.last_y = ny;
                if sess
                    .proxy
                    .call::<_, _, ()>(
                        "NotifyPointerMotionAbsolute",
                        &(&sess.session_handle, &empty, 0u32, px, py),
                    )
                    .await
                    .is_err()
                {
                    let _ = sess
                        .proxy
                        .call::<_, _, ()>(
                            "NotifyPointerMotion",
                            &(&sess.session_handle, &empty, dx, dy),
                        )
                        .await;
                }
                let btn = mouse_button_to_evdev(*button);
                let clicks = if *double { 2 } else { 1 };
                for _ in 0..clicks {
                    sess.proxy
                        .call::<_, _, ()>(
                            "NotifyPointerButton",
                            &(&sess.session_handle, &empty, btn, 1u32),
                        )
                        .await
                        .map_err(|e| {
                            Error::PortalDenied(tf(
                                TextKey::RemoteDesktopClick,
                                &[("err", &e.to_string())],
                            ))
                        })?;
                    sess.proxy
                        .call::<_, _, ()>(
                            "NotifyPointerButton",
                            &(&sess.session_handle, &empty, btn, 0u32),
                        )
                        .await
                        .map_err(|e| {
                            Error::PortalDenied(tf(
                                TextKey::RemoteDesktopClick,
                                &[("err", &e.to_string())],
                            ))
                        })?;
                }
            }
            ControlEvent::Scroll { dy } => {
                let steps = (*dy).clamp(-10, 10);
                sess.proxy
                    .call::<_, _, ()>(
                        "NotifyPointerAxisDiscrete",
                        &(&sess.session_handle, &empty, 0u32, steps),
                    )
                    .await
                    .map_err(|e| {
                        Error::PortalDenied(tf(
                            TextKey::RemoteDesktopScroll,
                            &[("err", &e.to_string())],
                        ))
                    })?;
            }
            ControlEvent::Key { code, key, .. } => {
                let sym = map_web_key_to_xkb_keysym(code, key);
                sess.proxy
                    .call::<_, _, ()>(
                        "NotifyKeyboardKeysym",
                        &(&sess.session_handle, &empty, sym, 1u32),
                    )
                    .await
                    .map_err(|e| {
                        Error::PortalDenied(tf(
                            TextKey::RemoteDesktopKey,
                            &[("err", &e.to_string())],
                        ))
                    })?;
                let _ = sess
                    .proxy
                    .call::<_, _, ()>(
                        "NotifyKeyboardKeysym",
                        &(&sess.session_handle, &empty, sym, 0u32),
                    )
                    .await;
            }
            ControlEvent::Text { text } => {
                for ch in text.chars() {
                    let sym = char_to_xkb_keysym(ch);
                    let _ = sess
                        .proxy
                        .call::<_, _, ()>(
                            "NotifyKeyboardKeysym",
                            &(&sess.session_handle, &empty, sym, 1u32),
                        )
                        .await;
                    let _ = sess
                        .proxy
                        .call::<_, _, ()>(
                            "NotifyKeyboardKeysym",
                            &(&sess.session_handle, &empty, sym, 0u32),
                        )
                        .await;
                }
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Control for WaylandPortalControl {
    async fn apply(&self, event: ControlEvent) -> Result<()> {
        match self.apply_portal(&event).await {
            Ok(()) => Ok(()),
            Err(portal_err) => {
                debug!(
                    "RemoteDesktop portal unavailable ({portal_err}); attempting XWayland fallback"
                );
                if std::env::var("DISPLAY").is_ok_and(|d| !d.is_empty()) && which_xdotool() {
                    let x11 = XdotoolControl {
                        screen_width: self.screen_width,
                        screen_height: self.screen_height,
                        offset_x: 0,
                        offset_y: 0,
                    };
                    return x11.apply(event).await;
                }
                Err(portal_err)
            }
        }
    }

    fn backend_name(&self) -> &'static str {
        "portal"
    }
}

fn which_xdotool() -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| std::path::Path::new(dir).join("xdotool").is_file())
}
