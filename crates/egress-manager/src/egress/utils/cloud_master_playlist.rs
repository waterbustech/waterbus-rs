use anyhow::Result;
use gst::prelude::{ElementExt, PadExtManual};
use m3u8_rs::{AlternativeMedia, AlternativeMediaType, MasterPlaylist, VariantStream};
use std::path::PathBuf;
use std::sync::Arc;

use super::{AudioStream, State, VideoStream, cloud_upload::R2Storage};

/// State wrapper that includes R2 upload capabilities
pub struct R2MasterState {
    pub state: State,
    pub r2_storage: Arc<R2Storage>,
    pub cloud_url_base: Option<String>,
}

impl R2MasterState {
    pub fn new(path: PathBuf, r2_storage: Arc<R2Storage>, cloud_url_base: Option<String>) -> Self {
        Self {
            state: State::new(path),
            r2_storage,
            cloud_url_base,
        }
    }

    /// Write the master playlist and upload it to R2
    pub fn maybe_write_and_upload_manifest(&mut self) -> Result<Option<String>> {
        if self.state.wrote_manifest {
            return Ok(None);
        }

        if self.state.all_mimes.len()
            < self.state.video_streams.len() + self.state.audio_streams.len()
        {
            return Ok(None);
        }

        let mut all_mimes = self.state.all_mimes.clone();
        all_mimes.sort();
        all_mimes.dedup();

        // First, create the master playlist for local storage
        let playlist = MasterPlaylist {
            version: Some(7),
            variants: self
                .state
                .video_streams
                .iter()
                .map(|stream| {
                    let mut path = PathBuf::new();
                    path.push(&stream.name);
                    path.push("manifest.m3u8");

                    // If we have a cloud URL base, use it to create absolute URLs
                    let uri = if let Some(base_url) = &self.cloud_url_base {
                        format!("{}/{}/{}", base_url, stream.name, "manifest.m3u8")
                    } else {
                        path.as_path().display().to_string()
                    };

                    VariantStream {
                        uri,
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
                .state
                .audio_streams
                .iter()
                .map(|stream| {
                    let mut path = PathBuf::new();
                    path.push(&stream.name);
                    path.push("manifest.m3u8");

                    // If we have a cloud URL base, use it to create absolute URLs
                    let uri = if let Some(base_url) = &self.cloud_url_base {
                        format!("{}/{}/{}", base_url, stream.name, "manifest.m3u8")
                    } else {
                        path.as_path().display().to_string()
                    };

                    AlternativeMedia {
                        media_type: AlternativeMediaType::Audio,
                        uri: Some(uri),
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

        println!("Writing master manifest to {}", self.state.path.display());

        // Write local copy first
        let mut file = std::fs::File::create(&self.state.path).unwrap();
        playlist
            .write_to(&mut file)
            .expect("Failed to write master playlist");

        // Mark as written in local state
        self.state.wrote_manifest = true;

        // Now upload to R2
        let url = self.r2_storage.upload_file(
            &self.state.path,
            "master.m3u8",
            "application/vnd.apple.mpegurl",
        )?;

        println!("Uploaded master manifest to R2: {}", url);

        Ok(Some(url))
    }

    pub fn add_video_stream(&mut self, video_stream: VideoStream) {
        self.state.add_video_stream(video_stream);
    }

    pub fn add_audio_stream(&mut self, audio_stream: AudioStream) {
        self.state.add_audio_stream(audio_stream);
    }

    pub fn add_mime(&mut self, mime: String) -> Result<Option<String>> {
        self.state.add_mime(mime);
        self.maybe_write_and_upload_manifest()
    }
}

/// Probes the encoder to extract codec information and updates R2MasterState
pub fn probe_encoder_with_r2(state: Arc<std::sync::Mutex<R2MasterState>>, enc: gst::Element) {
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

            if let Ok(mime_str) = mime {
                let mut state_guard = state.lock().unwrap();
                if let Err(e) = state_guard.add_mime(mime_str.to_string()) {
                    eprintln!("Failed to add MIME type and upload manifest: {}", e);
                }
            }

            gst::PadProbeReturn::Remove
        },
    );
}
