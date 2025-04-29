use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::Ok;
use gst::prelude::{ElementExt, ElementExtManual, GstBinExt, GstObjectExt, PipelineExt};
use tokio::task;

use super::gst_utils::{AudioStream, State, VideoStream};

#[derive(Debug, Clone)]
pub struct MoQWriter {
    pipeline: gst::Pipeline,
    state: Arc<Mutex<State>>,
    start_time: Instant,
    video_offset: Arc<Mutex<u64>>,
    audio_offset: Arc<Mutex<u64>>,
}

impl MoQWriter {
    pub fn new(participant_id: &str) -> Result<Self, anyhow::Error> {
        gst::init()?;
        gstfmp4::plugin_register_static()?;
        gstmoq::plugin_register_static()?;

        let dir = "./hls/moq";
        let path = PathBuf::from(dir);

        std::fs::create_dir_all(&path).expect("failed to create directory");

        let mut manifest_path = path.clone();
        manifest_path.push("manifest.m3u8");

        let pipeline = gst::Pipeline::default();

        let state = Arc::new(Mutex::new(State {
            video_streams: vec![VideoStream {
                name: "video_0".to_string(),
                bitrate: 2_048_000,
                width: 1280,
                height: 720,
                video_src: None,
            }],
            audio_streams: vec![AudioStream {
                name: "audio_0".to_string(),
                lang: "eng".to_string(),
                default: true,
                wave: "sine".to_string(),
                audio_src: None,
            }],
            all_mimes: vec![],
            path: manifest_path.clone(),
            wrote_manifest: false,
        }));

        let moq_url = format!("http://localhost:4443/waterbus/{}", participant_id);

        println!("[moq] url: {:?}", moq_url);

        {
            let mut state_lock = state.lock().unwrap();

            // Use &mut to get mutable references to the streams
            for stream in &mut state_lock.video_streams {
                let _ = stream.moq_setup(&pipeline);
            }

            // Assuming audio_streams also needs mutable setup
            for stream in &mut state_lock.audio_streams {
                let _ = stream.moq_setup(&pipeline);
            }
        }

        let _ = Self::_setup_moq_sink(&moq_url, &pipeline);

        pipeline.auto_clock();

        let this = Self {
            state,
            pipeline: pipeline.clone(),
            start_time: Instant::now(),
            video_offset: Arc::new(Mutex::new(0)),
            audio_offset: Arc::new(Mutex::new(0)),
        };

        let hls_writer_arc = Arc::new(this.clone());
        let writer_clone_for_blocking = Arc::clone(&hls_writer_arc);

        task::spawn_blocking(move || writer_clone_for_blocking.run_pipeline_blocking(pipeline));

        Ok(this)
    }

    pub fn run_pipeline_blocking(
        self: Arc<Self>,
        pipeline: gst::Pipeline,
    ) -> Result<(), anyhow::Error> {
        pipeline.set_state(gst::State::Playing)?;

        let bus = pipeline
            .bus()
            .expect("Pipeline without bus. Shouldn't happen!");

        // This loop is blocking the current thread (which is the spawn_blocking thread)
        for msg in bus.iter_timed(gst::ClockTime::NONE) {
            use gst::MessageView;

            match msg.view() {
                MessageView::Eos(..) => {
                    println!("Pipeline received EOS. Stopping.");
                    break; // Exit the loop on EOS
                }
                MessageView::Error(err) => {
                    eprintln!(
                        "Got error from {}: {} ({})",
                        msg.src()
                            .map(|s| String::from(s.path_string()))
                            .unwrap_or_else(|| "None".into()),
                        err.error(),
                        err.debug().unwrap_or_else(|| "".into()),
                    );
                    // Stop pipeline on error before returning
                    let _ = pipeline.set_state(gst::State::Null); // Ignore potential error during stopping
                    return Err(anyhow::anyhow!("GStreamer pipeline error: {}", err.error()));
                }
                MessageView::Warning(warn) => {
                    eprintln!(
                        "Got warning from {}: {} ({})",
                        msg.src()
                            .map(|s| String::from(s.path_string()))
                            .unwrap_or_else(|| "None".into()),
                        warn.error(),
                        warn.debug().unwrap_or_else(|| "".into()),
                    );
                }
                _ => (),
            }
        }

        let _ = pipeline.set_state(gst::State::Null);
        println!("Pipeline stopped.");

        Ok(())
    }

    pub fn stop(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }

    pub fn write_rtp(&self, data: &[u8], is_video: bool) -> Result<(), anyhow::Error> {
        if is_video {
            {
                let state_lock = self.state.lock().unwrap();

                for stream in &state_lock.video_streams {
                    let _ = stream.write_rtp(data, self.start_time, self.video_offset.clone());
                }
            }
        } else {
            {
                let state_lock = self.state.lock().unwrap();

                for stream in &state_lock.audio_streams {
                    let _ = stream.write_rtp(data, self.start_time, self.audio_offset.clone());
                }
            }
        }

        Ok(())
    }

    fn _setup_moq_sink(moq_url: &str, pipeline: &gst::Pipeline) -> Result<(), anyhow::Error> {
        let mux = pipeline
            .by_name("mux")
            .ok_or_else(|| anyhow::anyhow!("mux not found"))?;

        let moq_sink = gst::ElementFactory::make("moqsink")
            .property("url", moq_url)
            .property("tls-disable-verify", true)
            .build()?;

        pipeline.add(&moq_sink)?;

        mux.link(&moq_sink)?;

        Ok(())
    }
}
