//! screencast-encode: encoder selection, ffmpeg argument generation, and adaptive quality control.

pub mod adaptive;
pub mod codec;
pub mod plan;

pub use adaptive::{frame_signature, AdaptiveController, IdleDetector, NetworkSample};
pub use codec::{probe_vaapi_device, select_codec, Codec, EncoderProbe};
pub use plan::{build_ffmpeg_args, build_ffmpeg_args_with_device, EncodeOverrides, VideoInput};
