use std::{
    env,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::Ok;
use gst::{
    ClockTime,
    prelude::{ElementExt, GstObjectExt, PipelineExt},
};
use tokio::task;

use super::utils::{
    AudioStream, AudioStreamExt, R2Config, R2MasterState, R2Storage, State, VideoStream,
    VideoStreamExt, init,
};

#[derive(Debug, Clone)]
pub struct HlsWriter {
    pipeline: gst::Pipeline,
    state: Arc<Mutex<State>>,
    start_time: Instant,
    video_offset: Arc<Mutex<u64>>,
    audio_offset: Arc<Mutex<u64>>,
}

impl HlsWriter {
    pub async fn new(dir: &str, prefix_path: String) -> Result<Self, anyhow::Error> {
        init()?;

        let path = PathBuf::from(dir);
        let pipeline = gst::Pipeline::default();
        std::fs::create_dir_all(&path).expect("failed to create directory");

        let r2_config: Option<R2Config> = Self::_get_r2_config(prefix_path);

        let (r2_storage, master_state) = if let Some(config) = r2_config {
            // Use new_with_worker instead of new
            let (r2_storage, upload_receiver) = R2Storage::new_with_worker(config.clone()).await?;
            let r2_storage = Arc::new(r2_storage);

            // Start the upload worker
            let worker_storage = r2_storage.clone();
            worker_storage.start_upload_worker(upload_receiver);

            let cloud_url_base = match &r2_storage.config.custom_domain {
                Some(domain) => Some(format!("https://{domain}")),
                None => {
                    let account = r2_storage.config.account_id.clone();
                    let bucket = r2_storage.config.bucket_name.clone();
                    Some(format!(
                        "https://{account}.r2.cloudflarestorage.com/{bucket}"
                    ))
                }
            };

            let mut manifest_path = path.clone();
            manifest_path.push("manifest.m3u8");

            let master_state = Arc::new(std::sync::Mutex::new(R2MasterState::new(
                manifest_path.clone(),
                r2_storage.clone(),
                cloud_url_base.clone(),
            )));

            (Some(r2_storage), Some(master_state))
        } else {
            (None, None)
        };

        let mut manifest_path = path.clone();
        manifest_path.push("manifest.m3u8");

        let state = Arc::new(Mutex::new(State {
            video_streams: vec![VideoStream {
                name: "video_0".to_string(),
                bitrate: 2_048_000,
                width: 1280,
                height: 720,
                video_src: None,
                codec: "h264".to_owned(),
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

        {
            let mut state_lock = state.lock().unwrap();

            for stream in &mut state_lock.video_streams {
                let _ = stream.setup(
                    state.clone(),
                    master_state.clone(),
                    r2_storage.clone(),
                    &pipeline,
                    &path,
                );
            }

            for stream in &mut state_lock.audio_streams {
                stream.setup(
                    state.clone(),
                    master_state.clone(),
                    r2_storage.clone(),
                    &pipeline,
                    &path,
                )?;
            }
        }

        // pipeline.set_latency(ClockTime::from_nseconds(0));
        pipeline.auto_clock();
        pipeline.set_delay(ClockTime::from_nseconds(0));

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

    pub fn set_video_codec(&self, _codec: &str) {}

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

    fn _get_r2_config(path_prefix: String) -> Option<R2Config> {
        dotenvy::dotenv().ok();

        let account_id = env::var("STORAGE_ACCOUNT_ID").ok()?;
        let bucket_name = env::var("STORAGE_BUCKET_NAME").ok()?;
        let custom_domain = env::var("STORAGE_CUSTOM_DOMAIN").ok();

        let r2_config = R2Config {
            account_id,
            bucket_name,
            custom_domain,
            path_prefix: Some(path_prefix),
        };

        Some(r2_config)
    }
}
