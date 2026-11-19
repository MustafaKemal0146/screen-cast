//! Wi-Fi Display (WFD / Miracast) RTSP M1..M7 server and RFC 2250 RTP/MPEG-TS streamer.

mod handshake;
mod params;
mod rtp;
mod rtsp;

pub use handshake::WfdRtspServer;
pub use params::parse_wfd_client_rtp_ports;
pub use rtp::{RtpTsPacketizer, WfdMediaStreamer};
pub use rtsp::WfdRtspSession;

use std::net::IpAddr;

/// Default Wi-Fi Display RTSP session management port.
pub const DEFAULT_WFD_RTSP_PORT: u16 = 7236;

/// Negotiated audio codec for WFD MPEG-TS transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WfdAudioCodec {
    Aac,
    Lpcm,
    None,
}

/// Parameters negotiated with the TV/sink during RTSP M1..M7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WfdNegotiatedParams {
    pub client_ip: IpAddr,
    pub client_rtp_port: u16,
    pub use_raw_udp: bool,
    pub session_id: String,
    pub audio_codec: WfdAudioCodec,
}

#[cfg(test)]
mod tests {
    use super::*;
    use params::negotiate_wfd_video_format;
    use rtsp::read_rtsp_message;
    use screencast_core::{Quality, StreamPlan};
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, BufReader};
    use tokio::net::{TcpStream, UdpSocket};

    #[test]
    fn rtp_packetizer_packs_seven_ts_packets() {
        let mut p = RtpTsPacketizer::new(0x1122_3344);
        let ts_input = vec![0x47u8; 188 * 8];
        let pkts = p.push_bytes(&ts_input);
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].len(), 12 + 188 * 7);
        assert_eq!(pkts[0][0], 0x80);
        assert_eq!(pkts[0][1], 33); // PT=33 MP2T
        assert_eq!(&pkts[0][8..12], &0x1122_3344u32.to_be_bytes());

        let rem = p.flush().unwrap();
        assert_eq!(rem.len(), 12 + 188);

        let mut empty_p = RtpTsPacketizer::default();
        assert!(empty_p.push_bytes(&[0x47u8; 100]).is_empty());
        assert!(empty_p.flush().is_none());

        assert_eq!(
            parse_wfd_client_rtp_ports(
                "wfd_client_rtp_ports: RAW/RAW/UDP;unicast 15000 0 mode=play\r\n"
            ),
            (15000, true)
        );
        assert_eq!(
            parse_wfd_client_rtp_ports("unrelated: 123\r\n"),
            (1028, false)
        );

        let capped = negotiate_wfd_video_format(
            Quality::P1080,
            "wfd_video_formats: 00 00 01 02 00000020 00000000 00000000 00 0000 0000 00 none none\r\n",
        );
        assert!(capped.starts_with("28 00 01 02 00000020"));
    }

    #[tokio::test]
    async fn rtsp_m1_to_m7_and_rtp_ts_stream() {
        let server = WfdRtspServer::bind("127.0.0.1", 0).await.unwrap();
        let rtsp_addr = server.local_addr;

        let udp_rx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let rtp_port = udp_rx.local_addr().unwrap().port();

        let srv_handle = tokio::spawn(async move {
            server
                .accept_and_negotiate(&StreamPlan::default(), "127.0.0.1", Duration::from_secs(3))
                .await
                .unwrap()
        });

        let sink_handle = tokio::spawn(async move {
            let stream = TcpStream::connect(rtsp_addr).await.unwrap();
            let (rd, mut wr) = stream.into_split();
            let mut reader = BufReader::new(rd);

            // M1: Sink sends OPTIONS *
            wr.write_all(b"OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nRequire: org.wfa.wfd1.0\r\n\r\n")
                .await
                .unwrap();
            let m1_reply = read_rtsp_message(&mut reader).await.unwrap();
            assert_eq!(m1_reply.status_code(), Some(200));

            // M2: Source sends OPTIONS *
            let m2 = read_rtsp_message(&mut reader).await.unwrap();
            assert_eq!(m2.method(), "OPTIONS");
            wr.write_all(
                format!(
                    "RTSP/1.0 200 OK\r\nCSeq: {}\r\nPublic: org.wfa.wfd1.0, GET_PARAMETER, SET_PARAMETER, SETUP, PLAY, TEARDOWN\r\n\r\n",
                    m2.cseq()
                )
                .as_bytes(),
            )
            .await
            .unwrap();

            // M3: GET_PARAMETER
            let m3 = read_rtsp_message(&mut reader).await.unwrap();
            assert_eq!(m3.method(), "GET_PARAMETER");
            let m3_body = format!(
                "wfd_video_formats: 00 00 02 04 00000080 00000000 00000000 00 0000 0000 00 none none\r\n\
                 wfd_audio_codecs: AAC 00000001 00\r\n\
                 wfd_client_rtp_ports: RTP/AVP/UDP;unicast {rtp_port} 0 mode=play\r\n"
            );
            wr.write_all(
                format!(
                    "RTSP/1.0 200 OK\r\nCSeq: {}\r\nContent-Type: text/parameters\r\nContent-Length: {}\r\n\r\n{m3_body}",
                    m3.cseq(),
                    m3_body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();

            // M4: SET_PARAMETER
            let m4 = read_rtsp_message(&mut reader).await.unwrap();
            assert_eq!(m4.method(), "SET_PARAMETER");
            wr.write_all(format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", m4.cseq()).as_bytes())
                .await
                .unwrap();

            // M5: SET_PARAMETER wfd_trigger_method: SETUP
            let m5 = read_rtsp_message(&mut reader).await.unwrap();
            assert!(m5.body.contains("wfd_trigger_method: SETUP"));
            wr.write_all(format!("RTSP/1.0 200 OK\r\nCSeq: {}\r\n\r\n", m5.cseq()).as_bytes())
                .await
                .unwrap();

            // M6: Sink sends SETUP
            wr.write_all(
                format!(
                    "SETUP rtsp://127.0.0.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 2\r\nTransport: RTP/AVP/UDP;unicast;client_port={rtp_port}\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            let m6_reply = read_rtsp_message(&mut reader).await.unwrap();
            assert_eq!(m6_reply.status_code(), Some(200));
            let sess = m6_reply
                .headers
                .get("session")
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_string();

            // M7: Sink sends PLAY
            wr.write_all(
                format!("PLAY rtsp://127.0.0.1/wfd1.0/streamid=0 RTSP/1.0\r\nCSeq: 3\r\nSession: {sess}\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
            let m7_reply = read_rtsp_message(&mut reader).await.unwrap();
            assert_eq!(m7_reply.status_code(), Some(200));
        });

        let session = srv_handle.await.unwrap();
        sink_handle.await.unwrap();
        assert_eq!(session.params.client_rtp_port, rtp_port);
        assert_eq!(session.params.audio_codec, WfdAudioCodec::Aac);

        // Stream fixture fMP4 -> MPEG-TS -> RTP -> UDP receiver
        let sample_mp4 =
            include_bytes!("../../../screencast-transport/tests/fixtures/sample_frag.mp4");
        let streamer = WfdMediaStreamer::start_from_reader(
            Box::new(std::io::Cursor::new(&sample_mp4[..])),
            SocketAddr::from((session.params.client_ip, session.params.client_rtp_port)),
            session.params.use_raw_udp,
            WfdAudioCodec::None,
        )
        .await
        .unwrap();

        let mut pkt = [0u8; 2048];
        let (n, _) = tokio::time::timeout(Duration::from_secs(4), udp_rx.recv_from(&mut pkt))
            .await
            .unwrap()
            .unwrap();
        assert!(n >= 12 + 188);
        assert_eq!(pkt[0] & 0xC0, 0x80);
        assert_eq!(pkt[1] & 0x7F, 33); // PT=33 MPEG2-TS
        assert_eq!(pkt[12], 0x47); // TS sync byte

        streamer.stop();
        session.stop().await;
    }
}
