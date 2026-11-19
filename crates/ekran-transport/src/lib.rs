//! ekran-transport: HTTP + WebSocket sunucu, fMP4 kutu ayrıştırıcı, fan-out hub ve HLS paketleyici.

pub mod api;
pub mod hls;
pub mod http;
pub mod hub;
pub mod mp4;
pub mod ws;

pub use api::{ApiDiscover, ApiError, ApiInfo, ApiStats};
pub use hls::HlsPipeline;
pub use http::{ServerState, StaticAssets, TransportServer};
pub use hub::{ClientTelemetry, OutboundFrame, StreamHub};
pub use mp4::{BoxKind, BoxReader, FragmentAssembler, MediaChunk, Mp4Box};
pub use ws::{compute_accept_key, encode_frame, read_message, Opcode, WsMessage};
