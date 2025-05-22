use anyhow::Error;
use gst::prelude::*;
use gst::{BufferFlags, ClockTime};
use gst_app::{AppSrc, AppStreamType};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::error;

use super::playlist::setup_appsink;
use super::state::probe_encoder;
use super::{
    R2MasterState, R2Storage, State, VideoStream, probe_encoder_with_r2, setup_r2_appsink,
};

impl VideoStream {
    pub fn new(name: &str, bitrate: u64, width: u64, height: u64, codec: &str) -> Self {
        Self {
            name: name.to_string(),
            bitrate,
            width,
            height,
            video_src: None,
            codec: codec.to_string(),
        }
    }

    pub fn set_video_codec(&mut self, codec: &str) {
        self.codec = codec.to_owned();
    }
}

pub trait VideoStreamExt {
    fn setup(
        &mut self,
        state: Arc<Mutex<State>>,
        master_state: Arc<Mutex<R2MasterState>>,
        r2_storage: Arc<R2Storage>,
        pipeline: &gst::Pipeline,
        path: &Path,
    ) -> Result<(), Error>;
    fn moq_setup(&mut self, pipeline: &gst::Pipeline) -> Result<(), Error>;
    fn write_rtp(
        &self,
        data: &[u8],
        start_time: Instant,
        offset: Arc<Mutex<u64>>,
    ) -> Result<(), Error>;
}

impl VideoStreamExt for VideoStream {
    fn setup(
        &mut self,
        state: Arc<Mutex<State>>,
        master_state: Arc<Mutex<R2MasterState>>,
        r2_storage: Arc<R2Storage>,
        pipeline: &gst::Pipeline,
        path: &Path,
    ) -> Result<(), Error> {
        let caps = gst::Caps::builder("application/x-rtp")
            .field("media", "video")
            .field("clock-rate", 90000i32)
            .build();

        let src = gst::ElementFactory::make("appsrc")
            .property("is-live", true)
            .property("format", gst::Format::Time)
            .property("do-timestamp", true)
            .property("caps", caps)
            .build()?;

        // Decoder part based on codec, encoding still to H.264
        let depay;
        let decoder;

        match self.codec.as_str() {
            "h264" => {
                depay = gst::ElementFactory::make("rtph264depay").build()?;
                decoder = gst::ElementFactory::make("avdec_h264").build()?;
            }
            "vp8" => {
                depay = gst::ElementFactory::make("rtpvp8depay").build()?;
                decoder = gst::ElementFactory::make("vp8dec").build()?;
            }
            "vp9" => {
                depay = gst::ElementFactory::make("rtpvp9depay").build()?;
                decoder = gst::ElementFactory::make("vp9dec").build()?;
            }
            "av1" => {
                depay = gst::ElementFactory::make("rtpav1depay").build()?;
                decoder = gst::ElementFactory::make("av1dec").build()?;
            }
            _ => return Err(anyhow::anyhow!("Unsupported codec")),
        }

        let videoscale = gst::ElementFactory::make("videoscale").build()?;
        let videorate = gst::ElementFactory::make("videorate").build()?;

        // Encoding to H.264
        let raw_capsfilter = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst_video::VideoCapsBuilder::new()
                    .format(gst_video::VideoFormat::I420)
                    .width(self.width as i32)
                    .height(self.height as i32)
                    .framerate(30.into())
                    .build(),
            )
            .build()?;
        let enc = gst::ElementFactory::make("x264enc")
            .property("bframes", 0u32)
            .property("bitrate", self.bitrate as u32 / 1000u32)
            .property_from_str("tune", "zerolatency")
            .property_from_str("speed-preset", "ultrafast")
            .build()?;
        let h264_capsfilter = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-h264")
                    .field("profile", "main")
                    .build(),
            )
            .build()?;
        let mux = gst::ElementFactory::make("cmafmux")
            .property("fragment-duration", 200.mseconds())
            .property("write-mehd", true)
            .build()?;
        let appsink = gst_app::AppSink::builder().buffer_list(true).build();

        pipeline.add_many([
            &src,
            &depay,
            &decoder,
            &videoscale,
            &videorate,
            &raw_capsfilter,
            &enc,
            &h264_capsfilter,
            &mux,
            appsink.upcast_ref(),
        ])?;

        gst::Element::link_many([
            &src,
            &depay,
            &decoder,
            &videoscale,
            &videorate,
            &raw_capsfilter,
            &enc,
            &h264_capsfilter,
            &mux,
            appsink.upcast_ref(),
        ])?;

        probe_encoder(state, enc.clone());
        probe_encoder_with_r2(master_state, enc);

        setup_appsink(&appsink, &self.name, path, true);
        setup_r2_appsink(&appsink, &self.name, path, true, r2_storage);

        let video_src = src.downcast::<AppSrc>().expect("Element is not an AppSrc");
        video_src.set_is_live(true);
        video_src.set_stream_type(AppStreamType::Stream);
        video_src.set_latency(ClockTime::from_mseconds(0), ClockTime::from_mseconds(200));

        self.video_src = Some(video_src);

        Ok(())
    }

    fn moq_setup(&mut self, pipeline: &gst::Pipeline) -> Result<(), Error> {
        // RTP Caps
        let caps = gst::Caps::builder("application/x-rtp")
            .field("media", "video")
            .field("clock-rate", 90000i32)
            .build();

        // AppSrc to push incoming video frames
        let src = gst::ElementFactory::make("appsrc")
            .property("is-live", true)
            .property("format", gst::Format::Time)
            .property("do-timestamp", true)
            .property("caps", caps)
            .build()?;

        // Decoder elements based on codec selection
        let depay;
        let decoder;

        match self.codec.as_str() {
            "h264" => {
                depay = gst::ElementFactory::make("rtph264depay").build()?;
                decoder = gst::ElementFactory::make("h264parse").build()?;
            }
            "vp8" => {
                depay = gst::ElementFactory::make("rtpvp8depay").build()?;
                decoder = gst::ElementFactory::make("vp8dec").build()?;
            }
            "vp9" => {
                depay = gst::ElementFactory::make("rtpvp9depay").build()?;
                decoder = gst::ElementFactory::make("vp9dec").build()?;
            }
            "av1" => {
                depay = gst::ElementFactory::make("rtpav1depay").build()?;
                decoder = gst::ElementFactory::make("av1dec").build()?;
            }
            _ => return Err(anyhow::anyhow!("Unsupported codec")),
        }

        let queue = gst::ElementFactory::make("queue").name("v_queue").build()?;
        let identity = gst::ElementFactory::make("identity")
            .property("sync", true)
            .build()?;

        pipeline.add_many([&src, &depay, &decoder, &queue, &identity])?;

        gst::Element::link_many([&src, &depay, &decoder, &queue, &identity])?;

        // Set up the muxer to write output to MP4
        let mux = gst::ElementFactory::make("isofmp4mux")
            .name("mux")
            .property("fragment-duration", 1.nseconds())
            .property("chunk-duration", 1.nseconds())
            .build()?;

        pipeline.add(&mux)?;

        let mux_sink_pad = mux
            .request_pad_simple("sink_%u")
            .ok_or_else(|| anyhow::anyhow!("Failed to request sink pad from mux"))?;

        let identity_pad = identity
            .static_pad("src")
            .ok_or_else(|| anyhow::anyhow!("identity has no src pad"))?;

        identity_pad.link(&mux_sink_pad)?;

        // Configure appsrc for live streaming
        let video_src = src.downcast::<AppSrc>().expect("Element is not an AppSrc");
        video_src.set_is_live(true);
        video_src.set_stream_type(AppStreamType::Stream);

        self.video_src = Some(video_src);

        Ok(())
    }

    /// Writes an RTP video packet to the appsrc.
    /// This function takes the raw RTP packet data.
    fn write_rtp(
        &self,
        data: &[u8],
        start_time: Instant,
        offset: Arc<Mutex<u64>>,
    ) -> Result<(), Error> {
        // Create a GStreamer buffer from the RTP packet data
        let mut buffer = gst::Buffer::from_mut_slice(data.to_vec());
        // Get the current elapsed time since the stream started
        let now = start_time.elapsed().as_nanos() as u64;

        // Lock the offset mutex to update and get the current offset
        let mut offset_lock = offset.lock().unwrap();
        let offset = *offset_lock;
        let offset_end = offset + data.len() as u64;

        {
            // Get a mutable reference to the buffer's metadata
            let buffer_mut = buffer
                .get_mut()
                .ok_or_else(|| anyhow::anyhow!("Failed to get mutable buffer"))?;
            // Set PTS and DTS based on elapsed time.
            buffer_mut.set_pts(gst::ClockTime::from_nseconds(now));
            buffer_mut.set_dts(gst::ClockTime::from_nseconds(now));
            // Mark the buffer as live data
            buffer_mut.set_flags(BufferFlags::LIVE);
            // Set the buffer offset and end offset (useful for tracking data flow)
            buffer_mut.set_offset(offset);
            buffer_mut.set_offset_end(offset_end);
        }

        // Update the offset for the next buffer
        *offset_lock = offset_end;

        match &self.video_src {
            Some(src) => {
                // Push the buffer to the appsrc element
                let result = src.push_buffer(buffer);

                // Handle the result of the push operation
                match result {
                    Ok(gst::FlowSuccess::Ok) => Ok(()), // Buffer pushed successfully
                    Ok(other_flow) => {
                        // Handle other successful flow states if necessary
                        error!("Unexpected FlowReturn from video_src: {:?}", other_flow);
                        Err(anyhow::anyhow!(
                            "Unexpected GStreamer FlowReturn: {:?}",
                            other_flow
                        ))
                    }
                    Err(err) => {
                        // Handle errors during buffer push
                        error!("Failed to push RTP packet to video_src: {:?}", err);
                        Err(anyhow::Error::from(err))
                    }
                }
            }
            None => Ok(()),
        }
    }
}
