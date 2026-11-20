//! Buzzer tone: a 40 mV-ish square wave at the buzzer frequency (enabled with `--features audio`).
#[cfg(feature = "audio")]
mod imp {
    use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
    use std::sync::Arc;
    struct Square { f: Arc<AtomicU32>, on: Arc<AtomicBool>, phase: f32 }
    impl Iterator for Square {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            if !self.on.load(Relaxed) { return Some(0.0); }
            self.phase = (self.phase + f32::from_bits(self.f.load(Relaxed)) / 44100.0).fract();
            Some(if self.phase < 0.5 { 0.04 } else { -0.04 })
        }
    }
    impl Source for Square {
        fn current_frame_len(&self) -> Option<usize> { None }
        fn channels(&self) -> u16 { 1 }
        fn sample_rate(&self) -> u32 { 44100 }
        fn total_duration(&self) -> Option<std::time::Duration> { None }
    }
    #[derive(Default)]
    pub struct Audio { out: Option<(OutputStream, OutputStreamHandle, Sink)>, f: Arc<AtomicU32>, on: Arc<AtomicBool>, failed: bool }
    impl Audio {
        pub fn set(&mut self, tone: Option<f64>) {
            if self.out.is_none() && tone.is_some() && !self.failed {
                // created lazily on the first tone, like a browser AudioContext
                let made = OutputStream::try_default().ok().and_then(|(s, h)| Sink::try_new(&h).ok().map(|k| (s, h, k)));
                match made {
                    Some((s, h, k)) => { k.append(Square { f: self.f.clone(), on: self.on.clone(), phase: 0.0 }); self.out = Some((s, h, k)); }
                    None => self.failed = true,
                }
            }
            if let Some(t) = tone { self.f.store((t as f32).to_bits(), Relaxed); }
            self.on.store(tone.is_some(), Relaxed);
        }
    }
}
#[cfg(not(feature = "audio"))]
mod imp {
    /// No-op build (default): enable sound with `cargo run --release --features audio`.
    #[derive(Default)]
    pub struct Audio;
    impl Audio { pub fn set(&mut self, _tone: Option<f64>) {} }
}
pub use imp::Audio;
