mod audio_stream;
mod playlist;
mod segment;
mod state;
mod uploader;
mod video_stream;

// Re-export main types
pub use audio_stream::AudioStreamExt;
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
