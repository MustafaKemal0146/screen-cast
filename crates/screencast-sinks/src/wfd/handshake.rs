//! WFD RTSP server executing the M1..M7 capability negotiation handshake.

use super::params::{
    negotiate_wfd_video_format, parse_client_port_from_transport, parse_wfd_client_rtp_ports,
    select_audio_codec,
};
use super::rtsp::{read_rtsp_message, send_rtsp_raw, RtspMessage, WfdRtspSession};
use super::{WfdAudioCodec, WfdNegotiatedParams};
use screencast_core::i18n::{t, tf, TextKey};
use screencast_core::{Error, Result, StreamPlan};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};
use tracing::{debug, info};

/// Bound WFD RTSP Server listening for an incoming TV/Sink connection (M1..M7).
pub struct WfdRtspServer {
    listener: TcpListener,
    pub local_addr: SocketAddr,
}

impl WfdRtspServer {
    pub async fn bind(bind_ip: &str, port: u16) -> Result<Self> {
        let addr = format!("{bind_ip}:{port}");
        let listener = TcpListener::bind(&addr).await?;
        let local_addr = listener.local_addr()?;
        info!("WFD RTSP server listening on {local_addr}");
        Ok(Self {
            listener,
            local_addr,
        })
    }

    /// Wait for the WFD sink to connect and execute the M1..M7 RTSP capability negotiation.
    pub async fn accept_and_negotiate(
        self,
        plan: &StreamPlan,
        local_ip_hint: &str,
        timeout: Duration,
    ) -> Result<WfdRtspSession> {
        let port_s = self.local_addr.port().to_string();
        let (stream, peer) = tokio::time::timeout(timeout, self.listener.accept())
            .await
            .map_err(|_| {
                Error::SinkRefused(tf(TextKey::WfdRtspConnectTimeout, &[("port", &port_s)]))
            })??;

        info!("WFD RTSP client connected: {peer}");
        let socket_local_ip = stream
            .local_addr()
            .ok()
            .map(|a| a.ip())
            .filter(|ip| !ip.is_unspecified() && !ip.is_loopback())
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| local_ip_hint.to_string());
        let (rd_half, wr_half) = stream.into_split();
        let mut reader = BufReader::new(rd_half);
        let writer = Arc::new(Mutex::new(wr_half));
        let mut next_cseq = 1u32;

        // M1: Wait for Sink's `OPTIONS *`
        let m1 = read_timeout(&mut reader, 5, TextKey::WfdM1Timeout).await?;
        debug!("WFD M1 <- {}", m1.start_line);
        if m1.method() != "OPTIONS" {
            return Err(Error::Protocol(tf(
                TextKey::WfdM1ExpectedOptions,
                &[("line", &m1.start_line)],
            )));
        }
        send_rtsp_raw(
            &writer,
            &format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nPublic: org.wfa.wfd1.0, GET_PARAMETER, SET_PARAMETER\r\n\r\n",
                m1.cseq()
            ),
        )
        .await?;

        // M2: Source -> Sink `OPTIONS *`
        let m2_req =
            format!("OPTIONS * RTSP/1.0\r\nCSeq: {next_cseq}\r\nRequire: org.wfa.wfd1.0\r\n\r\n");
        next_cseq += 1;
        send_rtsp_raw(&writer, &m2_req).await?;
        let m2_resp = read_timeout(&mut reader, 5, TextKey::WfdM2Timeout).await?;
        if !m2_resp.is_response() || m2_resp.status_code() != Some(200) {
            return Err(Error::Protocol(tf(
                TextKey::WfdM2Failed,
                &[("line", &m2_resp.start_line)],
            )));
        }

        // M3: Source -> Sink `GET_PARAMETER`
        let m3_body = "wfd_video_formats\r\nwfd_audio_codecs\r\nwfd_client_rtp_ports\r\nwfd_content_protection\r\n";
        let m3_req = format!(
            "GET_PARAMETER rtsp://localhost/wfd1.0 RTSP/1.0\r\n\
             CSeq: {next_cseq}\r\n\
             Content-Type: text/parameters\r\n\
             Content-Length: {}\r\n\r\n\
             {m3_body}",
            m3_body.len()
        );
        next_cseq += 1;
        send_rtsp_raw(&writer, &m3_req).await?;
        let m3_resp = read_timeout(&mut reader, 5, TextKey::WfdM3Timeout).await?;
        debug!("WFD M3 <-\n{}", m3_resp.body);

        let (mut client_rtp_port, mut use_raw_udp) = parse_wfd_client_rtp_ports(&m3_resp.body);
        if client_rtp_port == 0 {
            client_rtp_port = 1028;
        }
        let audio_codec = select_audio_codec(plan.audio, &m3_resp.body);

        // M4: Source -> Sink `SET_PARAMETER`
        let video_fmt = negotiate_wfd_video_format(plan.quality, &m3_resp.body);
        let audio_line = match audio_codec {
            WfdAudioCodec::Aac => "wfd_audio_codecs: AAC 00000001 00\r\n",
            WfdAudioCodec::Lpcm | WfdAudioCodec::None => "",
        };
        let transport_proto = if use_raw_udp {
            "RAW/RAW/UDP"
        } else {
            "RTP/AVP/UDP"
        };
        let m4_body = format!(
            "wfd_video_formats: {video_fmt}\r\n\
             {audio_line}\
             wfd_presentation_URL: rtsp://{socket_local_ip}/wfd1.0/streamid=0 none\r\n\
             wfd_client_rtp_ports: {transport_proto};unicast {client_rtp_port} 0 mode=play\r\n"
        );
        send_set_param(&writer, &mut next_cseq, &m4_body).await?;
        let m4_resp = read_timeout(&mut reader, 5, TextKey::WfdM4Timeout).await?;
        if !m4_resp.is_response() || m4_resp.status_code() != Some(200) {
            return Err(Error::Protocol(tf(
                TextKey::WfdM4Rejected,
                &[("line", &m4_resp.start_line)],
            )));
        }

        // M5: Source -> Sink `SET_PARAMETER wfd_trigger_method: SETUP`
        send_set_param(&writer, &mut next_cseq, "wfd_trigger_method: SETUP\r\n").await?;
        let m5_resp = read_timeout(&mut reader, 5, TextKey::WfdM5Timeout).await?;
        if !m5_resp.is_response() || m5_resp.status_code() != Some(200) {
            return Err(Error::Protocol(tf(
                TextKey::WfdM5Rejected,
                &[("line", &m5_resp.start_line)],
            )));
        }

        // M6: Sink -> Source `SETUP`
        let m6 = read_timeout(&mut reader, 6, TextKey::WfdM6Timeout).await?;
        if m6.method() != "SETUP" {
            return Err(Error::Protocol(tf(
                TextKey::WfdM6ExpectedSetup,
                &[("line", &m6.start_line)],
            )));
        }
        if let Some(hdr) = m6.headers.get("transport") {
            if let Some(p) = parse_client_port_from_transport(hdr) {
                client_rtp_port = p;
            }
            if hdr.contains("RAW/RAW/UDP") {
                use_raw_udp = true;
            }
        }
        let session_id = format!("{:08X}", std::process::id().wrapping_mul(0x9E37_79B9));
        let proto = if use_raw_udp {
            "RAW/RAW/UDP"
        } else {
            "RTP/AVP/UDP"
        };
        send_rtsp_raw(
            &writer,
            &format!(
                "RTSP/1.0 200 OK\r\n\
                 CSeq: {}\r\n\
                 Session: {session_id};timeout=60\r\n\
                 Transport: {proto};unicast;client_port={client_rtp_port};server_port=5004\r\n\r\n",
                m6.cseq()
            ),
        )
        .await?;

        // M7: Sink -> Source `PLAY`
        let m7 = read_timeout(&mut reader, 6, TextKey::WfdM7Timeout).await?;
        if m7.method() != "PLAY" {
            return Err(Error::Protocol(tf(
                TextKey::WfdM7ExpectedPlay,
                &[("line", &m7.start_line)],
            )));
        }
        send_rtsp_raw(
            &writer,
            &format!(
                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: {session_id}\r\n\r\n",
                m7.cseq()
            ),
        )
        .await?;

        info!(
            "WFD M1..M7 completed -> target={}:{client_rtp_port} (raw_udp={use_raw_udp}, audio={audio_codec:?})",
            peer.ip()
        );

        let keepalive_stop = spawn_keepalive_loop(reader, writer.clone(), session_id.clone());

        Ok(WfdRtspSession {
            params: WfdNegotiatedParams {
                client_ip: peer.ip(),
                client_rtp_port,
                use_raw_udp,
                session_id,
                audio_codec,
            },
            writer,
            cseq: Arc::new(Mutex::new(next_cseq)),
            keepalive_stop,
        })
    }
}

async fn read_timeout(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    secs: u64,
    timeout_key: TextKey,
) -> Result<RtspMessage> {
    tokio::time::timeout(Duration::from_secs(secs), read_rtsp_message(reader))
        .await
        .map_err(|_| Error::Protocol(t(timeout_key).into()))?
}

async fn send_set_param(
    writer: &Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    next_cseq: &mut u32,
    body: &str,
) -> Result<()> {
    let seq = *next_cseq;
    *next_cseq += 1;
    let req = format!(
        "SET_PARAMETER rtsp://localhost/wfd1.0 RTSP/1.0\r\n\
         CSeq: {seq}\r\n\
         Content-Type: text/parameters\r\n\
         Content-Length: {}\r\n\r\n\
         {body}",
        body.len()
    );
    send_rtsp_raw(writer, &req).await
}

fn spawn_keepalive_loop(
    mut reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    session_id: String,
) -> watch::Sender<bool> {
    let (keepalive_stop, mut stop_rx) = watch::channel(false);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() { break; }
                }
                msg_res = read_rtsp_message(&mut reader) => {
                    match msg_res {
                        Ok(msg) if !msg.is_response() => {
                            let method = msg.method().to_string();
                            let resp = format!(
                                "RTSP/1.0 200 OK\r\nCSeq: {}\r\nSession: {session_id}\r\n\r\n",
                                msg.cseq()
                            );
                            let _ = send_rtsp_raw(&writer, &resp).await;
                            if method == "TEARDOWN" {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
        }
    });
    keepalive_stop
}
