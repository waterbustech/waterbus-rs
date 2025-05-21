use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gst::prelude::{ElementExt, PadExtManual};
use gst_app::AppSrc;
use m3u8_rs::{AlternativeMedia, AlternativeMediaType, MasterPlaylist, VariantStream};

#[derive(Debug)]
pub struct State {
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub all_mimes: Vec<String>,
    pub path: PathBuf,
    pub wrote_manifest: bool,
}

impl State {
    pub fn new(path: PathBuf) -> Self {
        Self {
            video_streams: Vec::new(),
            audio_streams: Vec::new(),
            all_mimes: Vec::new(),
            path,
            wrote_manifest: false,
        }
    }

    pub fn maybe_write_manifest(&mut self) {
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

    pub fn add_video_stream(&mut self, video_stream: VideoStream) {
        self.video_streams.push(video_stream);
    }

    pub fn add_audio_stream(&mut self, audio_stream: AudioStream) {
        self.audio_streams.push(audio_stream);
    }

    pub fn add_mime(&mut self, mime: String) {
        self.all_mimes.push(mime);
        self.maybe_write_manifest();
    }
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

/// Probes the encoder to extract codec information
pub fn probe_encoder(state: Arc<Mutex<State>>, enc: gst::Element) {
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
