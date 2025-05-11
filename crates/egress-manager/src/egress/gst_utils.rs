use gst::{BufferFlags, ClockTime, prelude::*};
use gst_app::{AppSrc, AppStreamType};
use tracing::error;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Error;
use chrono::{DateTime, Duration, Utc};
use m3u8_rs::{
    AlternativeMedia, AlternativeMediaType, MasterPlaylist, MediaPlaylist, MediaSegment,
    ServerControl, VariantStream,
};

#[derive(Debug)]
pub struct State {
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub all_mimes: Vec<String>,
    pub path: PathBuf,
    pub wrote_manifest: bool,
}

impl State {
    fn maybe_write_manifest(&mut self) {
        if self.wrote_manifest {
            return;
        }

        if self.all_mimes.len() < self.video_streams.len() + self.audio_streams.len() {
            return;
        }

        let mut all_mimes = self.all_mimes.clone();
        all_mimes.sort();
        all_mimes.dedup();

        let playlist = MasterPlaylist {
            version: Some(7),
            variants: self
                .video_streams
                .iter()
                .map(|stream| {
                    let mut path = PathBuf::new();

                    path.push(&stream.name);
                    path.push("manifest.m3u8");

                    VariantStream {
                        uri: path.as_path().display().to_string(),
                        bandwidth: stream.bitrate,
                        codecs: Some(all_mimes.join(",")),
                        resolution: Some(m3u8_rs::Resolution {
                            width: stream.width,
                            height: stream.height,
                        }),
                        audio: Some("audio".to_string()),
                        ..Default::default()
                    }
                })
                .collect(),
            alternatives: self
                .audio_streams
                .iter()
                .map(|stream| {
                    let mut path = PathBuf::new();
                    path.push(&stream.name);
                    path.push("manifest.m3u8");

                    AlternativeMedia {
                        media_type: AlternativeMediaType::Audio,
                        uri: Some(path.as_path().display().to_string()),
                        group_id: "audio".to_string(),
                        language: Some(stream.lang.clone()),
                        name: stream.name.clone(),
                        default: stream.default,
                        autoselect: stream.default,
                        channels: Some("2".to_string()),
                        ..Default::default()
                    }
                })
                .collect(),
            independent_segments: true,
            ..Default::default()
        };

        println!("Writing master manifest to {}", self.path.display());

        let mut file = std::fs::File::create(&self.path).unwrap();
        playlist
            .write_to(&mut file)
            .expect("Failed to write master playlist");

        self.wrote_manifest = true;
    }
}

struct Segment {
    date_time: DateTime<Utc>,
    duration: gst::ClockTime,
    path: String,
}

struct UnreffedSegment {
    removal_time: DateTime<Utc>,
    path: String,
}

struct StreamState {
    path: PathBuf,
    segments: VecDeque<Segment>,
    trimmed_segments: VecDeque<UnreffedSegment>,
    start_date_time: Option<DateTime<Utc>>,
    start_time: Option<gst::ClockTime>,
    media_sequence: u64,
    segment_index: u32,
}

#[derive(Debug)]
pub struct VideoStream {
    pub name: String,
    pub bitrate: u64,
    pub width: u64,
    pub height: u64,
    pub video_src: Option<AppSrc>,
    pub codec: String,
}

#[derive(Debug)]
pub struct AudioStream {
    pub name: String,
    pub lang: String,
    pub default: bool,
    pub wave: String,
    pub audio_src: Option<AppSrc>,
}

fn trim_segments(state: &mut StreamState) {
    // Arbitrary 5 segments window
    while state.segments.len() > 5 {
        let segment = state.segments.pop_front().unwrap();

        state.media_sequence += 1;

        state.trimmed_segments.push_back(UnreffedSegment {
            // HLS spec mandates that segments are removed from the filesystem no sooner
            // than the duration of the longest playlist + duration of the segment.
            // This is 15 seconds (12.5 + 2.5) in our case, we use 20 seconds to be on the
            // safe side
            removal_time: segment
                .date_time
                .checked_add_signed(Duration::try_seconds(20).unwrap())
                .unwrap(),
            path: segment.path.clone(),
        });
    }

    while let Some(segment) = state.trimmed_segments.front() {
        if segment.removal_time < state.segments.front().unwrap().date_time {
            let segment = state.trimmed_segments.pop_front().unwrap();

            let mut path = state.path.clone();
            path.push(segment.path);
            println!("Removing {}", path.display());
            std::fs::remove_file(path).expect("Failed to remove old segment");
        } else {
            break;
        }
    }
}

fn update_manifest(state: &mut StreamState) {
    // Now write the manifest
    let mut path = state.path.clone();
    path.push("manifest.m3u8");

    println!("writing manifest to {}", path.display());

    trim_segments(state);

    // LL-HLS
    let server_control = Some(ServerControl {
        can_skip_until: None,
        can_block_reload: true,
        can_skip_dateranges: true,
        hold_back: Some(1.2),
        part_hold_back: Some(0.6),
    });

    let playlist = MediaPlaylist {
        version: Some(7),
        server_control: server_control,
        target_duration: 1,
        media_sequence: state.media_sequence,
        segments: state
            .segments
            .iter()
            .enumerate()
            .map(|(idx, segment)| MediaSegment {
                uri: segment.path.to_string(),
                duration: (segment.duration.nseconds() as f64
                    / gst::ClockTime::SECOND.nseconds() as f64) as f32,
                map: Some(m3u8_rs::Map {
                    uri: "init.cmfi".into(),
                    ..Default::default()
                }),
                program_date_time: if idx == 0 {
                    Some(segment.date_time.into())
                } else {
                    None
                },
                ..Default::default()
            })
            .collect(),
        end_list: false,
        playlist_type: None,
        i_frames_only: false,
        start: None,
        independent_segments: true,
        ..Default::default()
    };

    let mut file = std::fs::File::create(path).unwrap();
    playlist
        .write_to(&mut file)
        .expect("Failed to write media playlist");
}

fn setup_appsink(appsink: &gst_app::AppSink, name: &str, path: &Path, is_video: bool) {
    let mut path: PathBuf = path.into();
    path.push(name);

    let name_arc = Arc::new(name.to_string());

    let state = Arc::new(Mutex::new(StreamState {
        segments: VecDeque::new(),
        trimmed_segments: VecDeque::new(),
        path,
        start_date_time: None,
        start_time: gst::ClockTime::NONE,
        media_sequence: 0,
        segment_index: 0,
    }));

    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let mut state = state.lock().unwrap();

                // The muxer only outputs non-empty buffer lists
                let mut buffer_list = sample.buffer_list_owned().expect("no buffer list");
                assert!(!buffer_list.is_empty());

                let mut first = buffer_list.get(0).unwrap();

                // Each list contains a full segment, i.e. does not start with a DELTA_UNIT
                assert!(!first.flags().contains(gst::BufferFlags::DELTA_UNIT));

                // If the buffer has the DISCONT and HEADER flag set then it contains the media
                // header, i.e. the `ftyp`, `moov` and other media boxes.
                //
                // This might be the initial header or the updated header at the end of the stream.
                if first
                    .flags()
                    .contains(gst::BufferFlags::DISCONT | gst::BufferFlags::HEADER)
                {
                    let mut path = state.path.clone();
                    std::fs::create_dir_all(&path).expect("failed to create directory");
                    path.push("init.cmfi");

                    println!("writing header to {}", path.display());
                    let map = first.map_readable().unwrap();
                    std::fs::write(path, &map).expect("failed to write header");
                    drop(map);

                    // Remove the header from the buffer list
                    buffer_list.make_mut().remove(0..1);

                    // If the list is now empty then it only contained the media header and nothing
                    // else.
                    if buffer_list.is_empty() {
                        return Ok(gst::FlowSuccess::Ok);
                    }

                    // Otherwise get the next buffer and continue working with that.
                    first = buffer_list.get(0).unwrap();
                }

                // If the buffer only has the HEADER flag set then this is a segment header that is
                // followed by one or more actual media buffers.
                assert!(first.flags().contains(gst::BufferFlags::HEADER));

                let mut path = state.path.clone();
                let basename = format!(
                    "segment_{}.{}",
                    state.segment_index,
                    if is_video { "cmfv" } else { "cmfa" }
                );
                state.segment_index += 1;
                path.push(&basename);

                let segment = sample
                    .segment()
                    .expect("no segment")
                    .downcast_ref::<gst::ClockTime>()
                    .expect("no time segment");
                let pts = segment
                    .to_running_time(first.pts().unwrap())
                    .expect("can't get running time");

                if state.start_time.is_none() {
                    state.start_time = Some(pts);
                }

                if state.start_date_time.is_none() {
                    let now_utc = Utc::now();
                    let now_gst = sink.clock().unwrap().time().unwrap();
                    let pts_clock_time = pts + sink.base_time().unwrap();

                    let diff = now_gst.checked_sub(pts_clock_time).unwrap();
                    let pts_utc = now_utc
                        .checked_sub_signed(Duration::nanoseconds(diff.nseconds() as i64))
                        .unwrap();

                    state.start_date_time = Some(pts_utc);
                }

                let duration = first.duration().unwrap();

                let mut file = std::fs::File::create(&path).expect("failed to open fragment");
                for buffer in &*buffer_list {
                    use std::io::prelude::*;

                    let map = buffer.map_readable().unwrap();
                    file.write_all(&map).expect("failed to write fragment");
                }

                let date_time = state
                    .start_date_time
                    .unwrap()
                    .checked_add_signed(Duration::nanoseconds(
                        pts.opt_checked_sub(state.start_time)
                            .unwrap()
                            .unwrap()
                            .nseconds() as i64,
                    ))
                    .unwrap();

                // println!(
                //     "wrote segment with date time {} to {}",
                //     date_time,
                //     path.display()
                // );

                state.segments.push_back(Segment {
                    duration,
                    path: basename.to_string(),
                    date_time,
                });

                update_manifest(&mut state);

                Ok(gst::FlowSuccess::Ok)
            })
            .eos({
                // Clone the Arc<String> for the eos closure
                let name_clone = Arc::clone(&name_arc);
                // state is not used in eos in your original code, but you might need it.
                // If so, clone the state Arc here too: let state = Arc::clone(&state);
                move |_sink| {
                    // Replace unreachable!() with logging or graceful shutdown logic
                    // Use the captured name_clone
                    println!(
                        "AppSink for stream '{}' received EOS signal.",
                        name_clone.as_ref()
                    );
                    // tracing::info!("AppSink for stream '{}' received EOS signal.", name_clone.as_ref()); // Using tracing macro
                    // You might want to signal something to a higher level
                    // or clean up resources associated with this specific stream.
                }
            })
            .build(),
    );
}

fn probe_encoder(state: Arc<Mutex<State>>, enc: gst::Element) {
    enc.static_pad("src").unwrap().add_probe(
        gst::PadProbeType::EVENT_DOWNSTREAM,
        move |_pad, info| {
            let Some(ev) = info.event() else {
                return gst::PadProbeReturn::Ok;
            };
            let gst::EventView::Caps(ev) = ev.view() else {
                return gst::PadProbeReturn::Ok;
            };

            let mime = gst_pbutils::codec_utils_caps_get_mime_codec(ev.caps());

            let mut state = state.lock().unwrap();
            state.all_mimes.push(mime.unwrap().into());
            state.maybe_write_manifest();

            gst::PadProbeReturn::Remove
        },
    );
}

impl VideoStream {
    pub fn set_video_codec(&mut self, codec: &str) {
        self.codec = codec.to_owned();
    }

    pub fn setup(
        &mut self,
        state: Arc<Mutex<State>>,
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

        probe_encoder(state, enc);

        setup_appsink(&appsink, &self.name, path, true);

        let video_src = src.downcast::<AppSrc>().expect("Element is not an AppSrc");
        video_src.set_is_live(true);
        video_src.set_stream_type(AppStreamType::Stream);
        video_src.set_latency(ClockTime::from_mseconds(0), ClockTime::from_mseconds(200));

        self.video_src = Some(video_src);

        Ok(())
    }

    pub fn moq_setup(&mut self, pipeline: &gst::Pipeline) -> Result<(), Error> {
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
    pub fn write_rtp(
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

impl AudioStream {
    pub fn setup(
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

    pub fn moq_setup(&mut self, pipeline: &gst::Pipeline) -> Result<(), Error> {
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
    pub fn write_rtp(
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
