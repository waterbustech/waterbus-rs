use super::aws_utils::get_storage_object_client;
use super::{Segment, StreamState};
use anyhow::Result;
use aws_sdk_s3::Client;
use aws_sdk_s3::presigning::PresigningConfig;
use gst::prelude::{ClockExt, ElementExt, OptionCheckedSub};
use std::path::Path;
use std::sync::Mutex;
use std::{path::PathBuf, sync::Arc};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

#[derive(Clone)]
/// Configuration for Cloudflare R2 storage
pub struct R2Config {
    pub account_id: String,
    pub bucket_name: String,
    pub custom_domain: Option<String>,
    pub path_prefix: Option<String>,
}

/// Upload task message
#[derive(Debug)]
pub struct UploadTask {
    pub local_path: PathBuf,
    pub key: String,
    pub content_type: String,
}

/// R2 storage manager for handling uploads
pub struct R2Storage {
    client: Client,
    pub config: R2Config,
    upload_sender: Option<mpsc::UnboundedSender<UploadTask>>,
}

impl R2Storage {
    /// Create a new R2Storage instance
    pub async fn new(config: R2Config) -> Result<Self> {
        let client = get_storage_object_client().await;

        Ok(Self {
            client,
            config,
            upload_sender: None,
        })
    }

    /// Create a new R2Storage instance with background upload worker
    pub async fn new_with_worker(
        config: R2Config,
    ) -> Result<(Self, mpsc::UnboundedReceiver<UploadTask>)> {
        let client = get_storage_object_client().await;
        let (tx, rx) = mpsc::unbounded_channel();

        let storage = Self {
            client,
            config,
            upload_sender: Some(tx),
        };

        Ok((storage, rx))
    }

    /// Start the upload worker task
    pub fn start_upload_worker(self: Arc<Self>, mut receiver: mpsc::UnboundedReceiver<UploadTask>) {
        tokio::spawn(async move {
            while let Some(task) = receiver.recv().await {
                match self
                    .upload_file_internal(&task.local_path, &task.key, &task.content_type)
                    .await
                {
                    Ok(url) => {
                        println!("Successfully uploaded {} to R2: {}", task.key, url);
                    }
                    Err(e) => {
                        eprintln!("Failed to upload {}: {}", task.key, e);
                    }
                }
            }
        });
    }

    /// Internal upload method
    async fn upload_file_internal(
        &self,
        local_path: &Path,
        key: &str,
        content_type: &str,
    ) -> Result<String> {
        let key = if let Some(prefix) = &self.config.path_prefix {
            format!("{}/{}", prefix, key)
        } else {
            key.to_string()
        };

        // Read file content
        let mut file = File::open(local_path).await?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).await?;

        // Upload to R2
        self.client
            .put_object()
            .bucket(&self.config.bucket_name)
            .key(&key)
            .body(contents.into())
            .content_type(content_type)
            .send()
            .await?;

        // Generate the URL for the uploaded file
        let url = if let Some(domain) = &self.config.custom_domain {
            format!("https://{}/{}", domain, key)
        } else {
            format!(
                "https://{}.r2.cloudflarestorage.com/{}",
                self.config.account_id, key
            )
        };

        Ok(url)
    }

    /// Upload a file to R2 storage (non-blocking via message queue)
    pub fn upload_file(&self, local_path: &Path, key: &str, content_type: &str) -> Result<()> {
        if let Some(sender) = &self.upload_sender {
            let task = UploadTask {
                local_path: local_path.to_path_buf(),
                key: key.to_string(),
                content_type: content_type.to_string(),
            };
            sender
                .send(task)
                .map_err(|_| anyhow::anyhow!("Upload worker channel closed"))?;
        } else {
            return Err(anyhow::anyhow!("Upload worker not initialized"));
        }
        Ok(())
    }

    /// Upload a file to R2 storage asynchronously
    pub async fn upload_file_async(
        &self,
        local_path: &Path,
        key: &str,
        content_type: &str,
    ) -> Result<String> {
        self.upload_file_internal(local_path, key, content_type)
            .await
    }

    /// Get a pre-signed URL for a file in R2 storage asynchronously
    pub async fn get_presigned_url_async(&self, key: &str, expires_in_secs: u64) -> Result<String> {
        let key = if let Some(prefix) = &self.config.path_prefix {
            format!("{}/{}", prefix, key)
        } else {
            key.to_string()
        };

        let presigner =
            PresigningConfig::expires_in(std::time::Duration::from_secs(expires_in_secs))?;

        let presigned_req = self
            .client
            .get_object()
            .bucket(&self.config.bucket_name)
            .key(&key)
            .presigned(presigner)
            .await?;

        Ok(presigned_req.uri().to_string())
    }
}

/// Extended StreamState for R2 storage integration
pub struct R2StreamState {
    pub state: StreamState,
    pub r2_storage: Arc<R2Storage>,
    pub uploaded_segments: Vec<String>,
    pub manifest_url: Option<String>,
}

impl R2StreamState {
    /// Create a new R2StreamState with R2 storage integration
    pub fn new(path: PathBuf, r2_storage: Arc<R2Storage>) -> Self {
        Self {
            state: StreamState::new(path),
            r2_storage,
            uploaded_segments: Vec::new(),
            manifest_url: None,
        }
    }

    /// Add a segment and queue it for upload to R2
    pub fn add_segment(&mut self, segment: Segment) -> Result<()> {
        // Add segment to local state
        self.state.add_segment(segment.clone());

        // Queue segment for upload to R2
        let mut path = self.state.path.clone();
        path.push(&segment.path);

        let content_type = if segment.path.ends_with(".cmfv") {
            "video/mp4"
        } else if segment.path.ends_with(".cmfa") {
            "audio/mp4"
        } else if segment.path.ends_with(".m3u8") {
            "application/vnd.apple.mpegurl"
        } else if segment.path.ends_with(".cmfi") {
            "video/mp4"
        } else {
            "application/octet-stream"
        };

        self.r2_storage
            .upload_file(&path, &segment.path, content_type)?;

        Ok(())
    }

    /// Add a segment and upload it to R2 asynchronously (spawned task)
    pub fn add_segment_async(&mut self, segment: Segment) {
        // Add segment to local state
        self.state.add_segment(segment.clone());

        // Upload segment to R2 in background
        let mut path = self.state.path.clone();
        path.push(&segment.path);

        let content_type = if segment.path.ends_with(".cmfv") {
            "video/mp4"
        } else if segment.path.ends_with(".cmfa") {
            "audio/mp4"
        } else if segment.path.ends_with(".m3u8") {
            "application/vnd.apple.mpegurl"
        } else if segment.path.ends_with(".cmfi") {
            "video/mp4"
        } else {
            "application/octet-stream"
        };

        let r2_storage = Arc::clone(&self.r2_storage);
        let segment_path = segment.path.clone();

        // Use the message queue instead of tokio::spawn
        if let Err(e) = r2_storage.upload_file(&path, &segment_path, content_type) {
            eprintln!("Failed to queue segment upload {}: {}", segment_path, e);
        }
    }

    /// Upload the initialization segment to R2
    pub fn upload_init_segment(&mut self) -> Result<()> {
        let mut path = self.state.path.clone();
        path.push("init.cmfi");

        if path.exists() {
            self.r2_storage
                .upload_file(&path, "init.cmfi", "video/mp4")?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Initialization segment not found"))
        }
    }

    /// Upload the initialization segment to R2 asynchronously
    pub fn upload_init_segment_async(&mut self) {
        let mut path = self.state.path.clone();
        path.push("init.cmfi");

        if path.exists() {
            if let Err(e) = self.r2_storage.upload_file(&path, "init.cmfi", "video/mp4") {
                eprintln!("Failed to queue init segment upload: {}", e);
            }
        } else {
            eprintln!("Initialization segment not found");
        }
    }

    /// Update and upload the manifest to R2
    pub fn update_manifest(&mut self) -> Result<()> {
        // First update the local manifest
        super::playlist::update_manifest(&mut self.state);

        // Then queue it for upload to R2
        let mut path = self.state.path.clone();
        path.push("manifest.m3u8");

        self.r2_storage
            .upload_file(&path, "manifest.m3u8", "application/vnd.apple.mpegurl")?;

        Ok(())
    }

    /// Update and upload the manifest to R2 asynchronously
    pub fn update_manifest_async(&mut self) {
        // First update the local manifest
        super::playlist::update_manifest(&mut self.state);

        // Then queue it for upload to R2
        let mut path = self.state.path.clone();
        path.push("manifest.m3u8");

        if let Err(e) =
            self.r2_storage
                .upload_file(&path, "manifest.m3u8", "application/vnd.apple.mpegurl")
        {
            eprintln!("Failed to queue manifest upload: {}", e);
        }
    }

    /// Perform cleanup of old segments both locally and in R2
    pub fn cleanup_old_segments(&mut self) -> Result<()> {
        // Trim segments locally (relying on existing implementation)
        self.state.trim_segments();

        // TODO: We could also implement deletion of old segments in R2 here
        // if needed, but often it's better to use R2's lifecycle policies

        Ok(())
    }
}

/// Setup an R2-enabled AppSink for handling processed media segments
pub fn setup_r2_appsink(
    appsink: &gst_app::AppSink,
    name: &str,
    path: &std::path::Path,
    is_video: bool,
    r2_storage: Arc<R2Storage>,
) {
    let mut path: PathBuf = path.into();
    path.push(name);

    // Create directories if they don't exist
    std::fs::create_dir_all(&path).expect("Failed to create directories");

    let name_arc = Arc::new(name.to_string());

    let state = Arc::new(Mutex::new(R2StreamState::new(path, r2_storage)));

    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let mut state_guard = state.lock().unwrap();

                // The muxer only outputs non-empty buffer lists
                let mut buffer_list = sample.buffer_list_owned().expect("no buffer list");
                assert!(!buffer_list.is_empty());

                let mut first = buffer_list.get(0).unwrap();

                // Each list contains a full segment, i.e. does not start with a DELTA_UNIT
                assert!(!first.flags().contains(gst::BufferFlags::DELTA_UNIT));

                // If the buffer has the DISCONT and HEADER flag set then it contains the media
                // header, i.e. the `ftyp`, `moov` and other media boxes.
                if first
                    .flags()
                    .contains(gst::BufferFlags::DISCONT | gst::BufferFlags::HEADER)
                {
                    let mut path = state_guard.state.path.clone();
                    std::fs::create_dir_all(&path).expect("failed to create directory");
                    path.push("init.cmfi");

                    println!("writing header to {}", path.display());
                    let map = first.map_readable().unwrap();
                    std::fs::write(&path, &map).expect("failed to write header");

                    // After writing the init segment locally, upload it to R2 asynchronously
                    state_guard.upload_init_segment_async();

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

                let mut path = state_guard.state.path.clone();
                let basename = format!(
                    "segment_{}.{}",
                    state_guard.state.segment_index,
                    if is_video { "cmfv" } else { "cmfa" }
                );
                state_guard.state.segment_index += 1;
                path.push(&basename);

                let segment = sample
                    .segment()
                    .expect("no segment")
                    .downcast_ref::<gst::ClockTime>()
                    .expect("no time segment");
                let pts = segment
                    .to_running_time(first.pts().unwrap())
                    .expect("can't get running time");

                if state_guard.state.start_time.is_none() {
                    state_guard.state.start_time = Some(pts);
                }

                if state_guard.state.start_date_time.is_none() {
                    let now_utc = chrono::Utc::now();
                    let now_gst = sink.clock().unwrap().time().unwrap();
                    let pts_clock_time = pts + sink.base_time().unwrap();

                    let diff = now_gst.checked_sub(pts_clock_time).unwrap();
                    let pts_utc = now_utc
                        .checked_sub_signed(chrono::Duration::nanoseconds(diff.nseconds() as i64))
                        .unwrap();

                    state_guard.state.start_date_time = Some(pts_utc);
                }

                let duration = first.duration().unwrap();

                // Write segment file locally
                let mut file = std::fs::File::create(&path).expect("failed to open fragment");
                for buffer in &*buffer_list {
                    use std::io::prelude::*;

                    let map = buffer.map_readable().unwrap();
                    file.write_all(&map).expect("failed to write fragment");
                }

                let date_time = state_guard
                    .state
                    .start_date_time
                    .unwrap()
                    .checked_add_signed(chrono::Duration::nanoseconds(
                        pts.opt_checked_sub(state_guard.state.start_time)
                            .unwrap()
                            .unwrap()
                            .nseconds() as i64,
                    ))
                    .unwrap();

                // Create and add the segment
                let segment = Segment {
                    duration,
                    path: basename.to_string(),
                    date_time,
                };

                // Add segment and upload it to R2 asynchronously
                state_guard.add_segment_async(segment);

                // Update and upload the manifest asynchronously
                state_guard.update_manifest_async();

                // Clean up old segments
                if let Err(e) = state_guard.cleanup_old_segments() {
                    eprintln!("Failed to cleanup old segments: {}", e);
                }

                Ok(gst::FlowSuccess::Ok)
            })
            .eos({
                let name_clone = std::sync::Arc::clone(&name_arc);
                move |_sink| {
                    tracing::warn!(
                        "AppSink for stream '{}' received EOS signal.",
                        name_clone.as_ref()
                    );
                }
            })
            .build(),
    );
}
