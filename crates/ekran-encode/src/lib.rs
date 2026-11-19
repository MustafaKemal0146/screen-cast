//! ekran-encode: kodlayıcı seçimi, ffmpeg argüman üretimi ve adaptif kalite kontrolü.

pub mod adaptive;
pub mod codec;
pub mod plan;

pub use adaptive::{frame_signature, AdaptiveController, IdleDetector, NetworkSample};
pub use codec::{select_codec, Codec, EncoderProbe};
pub use plan::{build_ffmpeg_args, EncodeOverrides, VideoInput};
