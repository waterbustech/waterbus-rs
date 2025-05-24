use gst::prelude::{ClockExt, ElementExt, OptionCheckedSub};
use m3u8_rs::{MediaPlaylist, MediaSegment, ServerControl};
use std::path::PathBuf;

use crate::egress::utils::Segment;

use super::StreamState;

/// Update the HLS manifest file with current segment information
pub fn update_manifest(state: &mut StreamState) {
    // Create the path for the manifest file
    let mut path = state.path.clone();
    path.push("manifest.m3u8");

    println!("writing manifest to {}", path.display());

    // Trim old segments before updating the manifest
    state.trim_segments();

    // LL-HLS configuration
    let server_control = Some(ServerControl {
        can_skip_until: None,
        can_block_reload: true,
        can_skip_dateranges: true,
        hold_back: Some(1.2),
        part_hold_back: Some(0.6),
    });

    // Create the media playlist
    let playlist = MediaPlaylist {
        version: Some(7),
        server_control,
        target_duration: 2,
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
        playlist_type: Some(m3u8_rs::MediaPlaylistType::Vod),
        i_frames_only: false,
        start: None,
        independent_segments: true,
        ..Default::default()
    };

    // Write the playlist to file
    let mut file = std::fs::File::create(path).unwrap();
    playlist
        .write_to(&mut file)
        .expect("Failed to write media playlist");
}

/// Setup AppSink for handling processed media segments
pub fn setup_appsink(
    appsink: &gst_app::AppSink,
    name: &str,
    path: &std::path::Path,
    is_video: bool,
) {
    let mut path: PathBuf = path.into();
    path.push(name);

    let name_arc = std::sync::Arc::new(name.to_string());

    let state = std::sync::Arc::new(std::sync::Mutex::new(StreamState::new(path)));

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

                    tracing::debug!("writing header to {}", path.display());
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
                    let now_utc = chrono::Utc::now();
                    let now_gst = sink.clock().unwrap().time().unwrap();
                    let pts_clock_time = pts + sink.base_time().unwrap();

                    let diff = now_gst.checked_sub(pts_clock_time).unwrap();
                    let pts_utc = now_utc
                        .checked_sub_signed(chrono::Duration::nanoseconds(diff.nseconds() as i64))
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
                    .checked_add_signed(chrono::Duration::nanoseconds(
                        pts.opt_checked_sub(state.start_time)
                            .unwrap()
                            .unwrap()
                            .nseconds() as i64,
                    ))
                    .unwrap();

                state.add_segment(Segment {
                    duration,
                    path: basename.to_string(),
                    date_time,
                });

                update_manifest(&mut state);

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
