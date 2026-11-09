//! Screen recordings, as the Simulator's File ▸ Record Screen makes them: the
//! screen's frames as H.264 in an MP4. pomme draws a frame only when something
//! changes, so each frame lasts until the next one (a variable frame rate), and
//! the video keeps the app's own timing: a still screen is one long frame.
//!
//! Frames are RGBA at the size the recording started with. The phone may turn
//! while it records; later frames of another size are centred on black, as the
//! video's size is fixed.

use mp4::{AvcConfig, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig, TrackType};
use openh264::OpenH264API;
use openh264::encoder::{
    Encoder, EncoderConfig, FrameType, IntraFramePeriod, UsageType, VuiConfig,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Units per second of the video's clock (the usual one for video).
const TIMESCALE: u32 = 90_000;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub struct Recorder {
    encoder: Encoder,
    /// The video's size: the first frame's, rounded down to even (4:2:0 needs it).
    width: u32,
    height: u32,
    file: Option<BufWriter<File>>,
    mp4: Option<Mp4Writer<BufWriter<File>>>,
    /// The first frame's time: the video starts there.
    start_ns: Option<u64>,
    /// The last frame encoded, waiting to learn how long it lasts: (time, AVCC bytes, key frame).
    pending: Option<(u64, Vec<u8>, bool)>,
    /// Where the next sample starts on the video's clock (so rounding never drifts).
    written: u64,
    canvas: Vec<u8>,
}

impl Recorder {
    /// Record to `path` (an .mp4), `width` × `height` pixels.
    pub fn create(path: &Path, width: u32, height: u32) -> Result<Self, Error> {
        let (width, height) = (width & !1, height & !1);
        if width == 0 || height == 0 {
            return Err("a recording needs a picture at least 2 pixels each way".into());
        }
        let config = EncoderConfig::new()
            .usage_type(UsageType::ScreenContentRealTime)
            // A key frame every 2 s at most, so the video seeks well.
            .intra_frame_period(IntraFramePeriod::from_num_frames(120))
            .skip_frames(false)
            // openh264 converts RGB to YUV with BT.601's limited range: say so, or players shift the colours.
            .vui(VuiConfig::bt601());
        let encoder = Encoder::with_api_config(OpenH264API::from_source(), config)?;
        let file = BufWriter::new(File::create(path)?);
        Ok(Self {
            encoder,
            width,
            height,
            file: Some(file),
            mp4: None,
            start_ns: None,
            pending: None,
            written: 0,
            canvas: Vec::new(),
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Add a frame (RGBA, `width` × `height`) shown from `at_ns` until the next.
    pub fn push(&mut self, rgba: &[u8], width: u32, height: u32, at_ns: u64) -> Result<(), Error> {
        let start = *self.start_ns.get_or_insert(at_ns);
        let at_ns = at_ns.max(start);
        self.fit(rgba, width, height);
        let yuv = YUVBuffer::from_rgb_source(RgbaSliceU8::new(
            &self.canvas,
            (self.width as usize, self.height as usize),
        ));
        let stream = self.encoder.encode(&yuv)?;
        let key = matches!(stream.frame_type(), FrameType::IDR | FrameType::I);
        // Annex B NAL units to the MP4's length-prefixed ones; SPS and PPS go in the track's header.
        let (mut sample, mut sps, mut pps) = (Vec::new(), None, None);
        for l in 0..stream.num_layers() {
            let Some(layer) = stream.layer(l) else {
                continue;
            };
            for n in 0..layer.nal_count() {
                let Some(nal) = layer.nal_unit(n).map(strip_start_code) else {
                    continue;
                };
                match nal.first().map(|b| b & 0x1f) {
                    Some(7) => sps = Some(nal.to_vec()),
                    Some(8) => pps = Some(nal.to_vec()),
                    Some(_) => {
                        sample.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                        sample.extend_from_slice(nal);
                    }
                    None => {}
                }
            }
        }
        if self.mp4.is_none()
            && let (Some(sps), Some(pps)) = (sps, pps)
        {
            self.start_mp4(sps, pps)?;
        }
        if sample.is_empty() {
            return Ok(()); // nothing new to show (the encoder skipped it)
        }
        self.write_pending(at_ns)?;
        self.pending = Some((at_ns, sample, key));
        Ok(())
    }

    /// End the recording at `at_ns` (the last frame lasts until then) and write the file.
    pub fn finish(mut self, at_ns: u64) -> Result<(), Error> {
        let end = self.pending.as_ref().map_or(at_ns, |p| at_ns.max(p.0 + 1));
        self.write_pending(end)?;
        let Some(mut mp4) = self.mp4.take() else {
            return Err("nothing was recorded".into());
        };
        mp4.write_end()?;
        use std::io::Write;
        mp4.into_writer().flush()?;
        Ok(())
    }

    fn start_mp4(&mut self, sps: Vec<u8>, pps: Vec<u8>) -> Result<(), Error> {
        let file = self.file.take().ok_or("the recording's file is gone")?;
        let config = Mp4Config {
            major_brand: "isom".parse()?,
            minor_version: 512,
            compatible_brands: vec![
                "isom".parse()?,
                "iso2".parse()?,
                "avc1".parse()?,
                "mp41".parse()?,
            ],
            timescale: 1000,
        };
        let mut mp4 = Mp4Writer::write_start(file, &config)?;
        let avc = AvcConfig {
            width: self.width as u16,
            height: self.height as u16,
            seq_param_set: sps,
            pic_param_set: pps,
        };
        mp4.add_track(&TrackConfig {
            track_type: TrackType::Video,
            timescale: TIMESCALE,
            language: "und".into(),
            media_conf: MediaConfig::AvcConfig(avc),
        })?;
        self.mp4 = Some(mp4);
        Ok(())
    }

    /// Write the waiting frame, which lasts until `until_ns`.
    fn write_pending(&mut self, until_ns: u64) -> Result<(), Error> {
        let Some((_, bytes, key)) = self.pending.take() else {
            return Ok(());
        };
        let Some(mp4) = self.mp4.as_mut() else {
            return Ok(());
        };
        let end = ticks(until_ns - self.start_ns.unwrap_or(0)).max(self.written + 1);
        let sample = Mp4Sample {
            start_time: self.written,
            duration: (end - self.written) as u32,
            rendering_offset: 0,
            is_sync: key,
            bytes: bytes.into(),
        };
        mp4.write_sample(1, &sample)?;
        self.written = end;
        Ok(())
    }

    /// The frame on the video's canvas: as it is when the size matches, else centred on black.
    fn fit(&mut self, rgba: &[u8], width: u32, height: u32) {
        let (w, h) = (self.width as usize, self.height as usize);
        self.canvas.resize(w * h * 4, 0);
        if (width as usize, height as usize) == (w, h) {
            self.canvas.copy_from_slice(&rgba[..w * h * 4]);
            return;
        }
        for px in self.canvas.chunks_mut(4) {
            px.copy_from_slice(&[0, 0, 0, 255]);
        }
        let (sw, sh) = (width as usize, height as usize);
        let (cw, ch) = (sw.min(w), sh.min(h));
        let (dx, dy) = ((w - cw) / 2, (h - ch) / 2);
        let (sx, sy) = ((sw - cw) / 2, (sh - ch) / 2);
        for row in 0..ch {
            let src = ((sy + row) * sw + sx) * 4;
            let dst = ((dy + row) * w + dx) * 4;
            self.canvas[dst..dst + cw * 4].copy_from_slice(&rgba[src..src + cw * 4]);
        }
    }
}

fn ticks(ns: u64) -> u64 {
    (ns as u128 * TIMESCALE as u128 / 1_000_000_000) as u64
}

/// A NAL unit without its Annex B start code.
fn strip_start_code(nal: &[u8]) -> &[u8] {
    if nal.starts_with(&[0, 0, 0, 1]) {
        &nal[4..]
    } else if nal.starts_with(&[0, 0, 1]) {
        &nal[3..]
    } else {
        nal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recording is an MP4 with one H.264 track, its frames lasting until the next.
    #[test]
    fn records_frames_with_their_timing() {
        let dir = std::env::temp_dir().join(format!("pomme-video-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("r.mp4");
        let (w, h) = (64, 48);
        let mut r = Recorder::create(&path, w, h).unwrap();
        let frame = |shade: u8| -> Vec<u8> {
            (0..w * h)
                .flat_map(|_| [shade, 255 - shade, 0, 255])
                .collect()
        };
        // Three frames: 0 s, 0.5 s and 0.6 s; the recording ends at 2 s.
        r.push(&frame(0), w, h, 1_000_000_000).unwrap();
        r.push(&frame(100), w, h, 1_500_000_000).unwrap();
        r.push(&frame(200), w, h, 1_600_000_000).unwrap();
        r.finish(3_000_000_000).unwrap();

        let f = File::open(&path).unwrap();
        let size = f.metadata().unwrap().len();
        let mut mp4 = mp4::Mp4Reader::read_header(std::io::BufReader::new(f), size).unwrap();
        let track = mp4.tracks().get(&1).unwrap();
        assert_eq!((track.width(), track.height()), (64, 48));
        assert_eq!(track.media_type().unwrap(), mp4::MediaType::H264);
        assert_eq!(track.sample_count(), 3);
        let durations: Vec<u32> = (1..=3)
            .map(|i| mp4.read_sample(1, i).unwrap().unwrap().duration)
            .collect();
        assert_eq!(durations, [45_000, 9_000, 126_000]);
        assert!(mp4.read_sample(1, 1).unwrap().unwrap().is_sync);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// After the phone turns, frames of the other size sit centred on black.
    #[test]
    fn other_sizes_are_centred() {
        let dir = std::env::temp_dir().join(format!("pomme-video-fit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut r = Recorder::create(&dir.join("f.mp4"), 4, 2).unwrap();
        r.fit(&[9; 2 * 2 * 4], 2, 2);
        let row: Vec<u8> = r.canvas[..16].to_vec();
        assert_eq!(row, [0, 0, 0, 255, 9, 9, 9, 9, 9, 9, 9, 9, 0, 0, 0, 255]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
