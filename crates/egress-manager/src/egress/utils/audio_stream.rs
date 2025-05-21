use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::Error;

use super::{AudioStream, State};

use gst::prelude::*;
use gst::{BufferFlags, ClockTime};
use gst_app::{AppSrc, AppStreamType};
use tracing::error;

use super::playlist::setup_appsink;
use super::state::probe_encoder;

pub trait AudioStreamExt {
    fn setup(
        &mut self,
        state: Arc<Mutex<State>>,
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

impl AudioStreamExt for AudioStream {
    fn setup(
        &mut self,
        state: Arc<Mutex<State>>,
        pipeline: &gst::Pipeline,
        path: &Path,
    ) -> Result<(), Error> {
        let caps = gst::Caps::builder("application/x-rtp")
            .field("media", "audio")
            .field("encoding-name", "OPUS")
            .field("payload", 97i32)
            .field("clock-rate", 48000i32)
            .build();

        let src = gst::ElementFactory::make("appsrc")
            .property("is-live", true)
            .property("format", gst::Format::Time)
            .property("do-timestamp", true)
            .property("caps", caps)
            .build()?;

        let rtp_depay = gst::ElementFactory::make("rtpopusdepay").build()?;
        let opusdec = gst::ElementFactory::make("opusdec").build()?;
        let audioconvert = gst::ElementFactory::make("audioconvert").build()?;
        let audioresample = gst::ElementFactory::make("audioresample").build()?;
        let aacenc = gst::ElementFactory::make("avenc_aac").build()?;
        let aacparse = gst::ElementFactory::make("aacparse").build()?;
        let mux = gst::ElementFactory::make("cmafmux")
            .property_from_str("header-update-mode", "update")
            .property("write-mehd", true)
            .property("fragment-duration", 200.mseconds())
            .build()?;
        let appsink = gst_app::AppSink::builder().buffer_list(true).build();

        pipeline.add_many([
            &src,
            &rtp_depay,
            &opusdec,
            &audioconvert,
            &audioresample,
            &aacenc,
            &aacparse,
            &mux,
            appsink.upcast_ref(),
        ])?;

        gst::Element::link_many([
            &src,
            &rtp_depay,
            &opusdec,
            &audioconvert,
            &audioresample,
            &aacenc,
            &aacparse,
            &mux,
            appsink.upcast_ref(),
        ])?;

        probe_encoder(state, aacenc);

        setup_appsink(&appsink, &self.name, path, false);

        let audio_src = src.downcast::<AppSrc>().expect("Element is not an AppSrc");

        audio_src.set_is_live(true);
        audio_src.set_stream_type(AppStreamType::Stream);
        audio_src.set_latency(ClockTime::from_mseconds(0), ClockTime::from_mseconds(200));

        self.audio_src = Some(audio_src);

        Ok(())
    }

    fn moq_setup(&mut self, pipeline: &gst::Pipeline) -> Result<(), Error> {
        let caps = gst::Caps::builder("application/x-rtp")
            .field("media", "audio")
            .field("encoding-name", "OPUS")
            .field("payload", 97i32)
            .field("clock-rate", 48000i32)
            .build();

        let src = gst::ElementFactory::make("appsrc")
            .property("is-live", true)
            .property("format", gst::Format::Time)
            .property("do-timestamp", true)
            .property("caps", caps)
            .build()?;

        let rtp_depay = gst::ElementFactory::make("rtpopusdepay").build()?;
        let opusdec = gst::ElementFactory::make("opusdec").build()?;
        let audioconvert = gst::ElementFactory::make("audioconvert").build()?;
        let audioresample = gst::ElementFactory::make("audioresample").build()?;
        let aacenc = gst::ElementFactory::make("avenc_aac").build()?;
        let aacparse = gst::ElementFactory::make("aacparse").build()?;
        let queue = gst::ElementFactory::make("queue").name("a_queue").build()?;

        pipeline.add_many([
            &src,
            &rtp_depay,
            &opusdec,
            &audioconvert,
            &audioresample,
            &aacenc,
            &aacparse,
            &queue,
        ])?;

        gst::Element::link_many([
            &src,
            &rtp_depay,
            &opusdec,
            &audioconvert,
            &audioresample,
            &aacenc,
            &aacparse,
            &queue,
        ])?;

        let mux = pipeline
            .by_name("mux")
            .ok_or_else(|| anyhow::anyhow!("mux not found"))?;

        let mux_sink_pad = mux
            .request_pad_simple("sink_%u")
            .ok_or_else(|| anyhow::anyhow!("Failed to request sink pad from mux"))?;

        let queue_pad = queue
            .static_pad("src")
            .ok_or_else(|| anyhow::anyhow!("queue has no src pad"))?;

        queue_pad.link(&mux_sink_pad)?;

        // let appsink = gst_app::AppSink::builder().buffer_list(true).build();
        // setup_appsink(&appsink, &self.name, path, false);

        let audio_src = src.downcast::<AppSrc>().expect("Element is not an AppSrc");
        audio_src.set_is_live(true);
        audio_src.set_stream_type(AppStreamType::Stream);

        self.audio_src = Some(audio_src);

        Ok(())
    }

    /// Writes an RTP audio packet to the appsrc.
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
            // Note: For accurate synchronization with audio or other streams,
            // you might need to use the RTP timestamps from the packet itself
            // and convert them to GStreamer time.
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

        match &self.audio_src {
            Some(src) => {
                // Push the buffer to the appsrc element
                let result = src.push_buffer(buffer);

                // Handle the result of the push operation
                match result {
                    Ok(gst::FlowSuccess::Ok) => Ok(()), // Buffer pushed successfully
                    Ok(other_flow) => {
                        // Handle other successful flow states if necessary
                        error!("Unexpected FlowReturn from audio_src: {:?}", other_flow);
                        Err(anyhow::anyhow!(
                            "Unexpected GStreamer FlowReturn: {:?}",
                            other_flow
                        ))
                    }
                    Err(err) => {
                        // Handle errors during buffer push
                        error!("Failed to push RTP packet to audio_src: {:?}", err);
                        Err(anyhow::Error::from(err))
                    }
                }
            }
            None => Ok(()),
        }
    }
}
