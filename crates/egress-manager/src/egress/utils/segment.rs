use chrono::{DateTime, Duration, Utc};
use gst::ClockTime;
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Segment {
    pub date_time: DateTime<Utc>,
    pub duration: ClockTime,
    pub path: String,
}

pub struct UnreffedSegment {
    pub removal_time: DateTime<Utc>,
    pub path: String,
}

pub struct StreamState {
    pub path: PathBuf,
    pub segments: VecDeque<Segment>,
    pub trimmed_segments: VecDeque<UnreffedSegment>,
    pub start_date_time: Option<DateTime<Utc>>,
    pub start_time: Option<ClockTime>,
    pub media_sequence: u64,
    pub segment_index: u32,
}

impl StreamState {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            segments: VecDeque::new(),
            trimmed_segments: VecDeque::new(),
            start_date_time: None,
            start_time: ClockTime::NONE,
            media_sequence: 0,
            segment_index: 0,
        }
    }

    pub fn trim_segments(&mut self) {
        // Arbitrary 5 segments window
        while self.segments.len() > 5 {
            let segment = self.segments.pop_front().unwrap();

            self.media_sequence += 1;

            self.trimmed_segments.push_back(UnreffedSegment {
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

        while let Some(segment) = self.trimmed_segments.front() {
            if segment.removal_time < self.segments.front().unwrap().date_time {
                let segment = self.trimmed_segments.pop_front().unwrap();

                let mut path = self.path.clone();
                path.push(&segment.path);
                tracing::debug!("Removing {}", path.display());
                std::fs::remove_file(path).expect("Failed to remove old segment");
            } else {
                break;
            }
        }
    }

    pub fn add_segment(&mut self, segment: Segment) {
        self.segments.push_back(segment);
    }
}
