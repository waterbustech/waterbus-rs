// Main library file for HLS streaming with Cloudflare R2 integration
mod audio_stream;
mod aws_utils;
mod cloud_master_playlist;
mod cloud_upload;
mod playlist;
mod segment;
mod state;
mod video_stream;

// Re-export main types
pub use audio_stream::AudioStreamExt;
pub use cloud_master_playlist::{R2MasterState, probe_encoder_with_r2};
pub use cloud_upload::{R2Config, R2Storage, R2StreamState, setup_r2_appsink};
pub use playlist::update_manifest;
pub use segment::{Segment, StreamState, UnreffedSegment};
pub use state::{AudioStream, State, VideoStream};
pub use video_stream::VideoStreamExt;

// Initialize GStreamer
pub fn init() -> Result<(), anyhow::Error> {
    gst::init()?;
    gstfmp4::plugin_register_static()?;
    gstmoq::plugin_register_static()?;
    Ok(())
}
