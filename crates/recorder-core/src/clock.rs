//! The media clock shared by video and audio.
//!
//! Media time starts at zero when the encoder is ready and does not advance
//! while paused, so pausing leaves no gap in the file: video and audio both
//! simply stop producing samples, and resume from the same timestamp.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
struct State {
    start: Option<Instant>,
    paused_total: Duration,
    paused_at: Option<Instant>,
}

#[derive(Default)]
pub struct MediaClock {
    state: Mutex<State>,
}

impl MediaClock {
    /// Sets time zero. Later calls are ignored.
    pub fn start(&self) {
        let mut s = self.state.lock().unwrap();
        if s.start.is_none() {
            s.start = Some(Instant::now());
        }
    }

    /// Media time, or `None` before [`MediaClock::start`]. Frozen while paused.
    pub fn now(&self) -> Option<Duration> {
        let s = self.state.lock().unwrap();
        let start = s.start?;
        let end = s.paused_at.unwrap_or_else(Instant::now);
        Some(end.saturating_duration_since(start).saturating_sub(s.paused_total))
    }

    pub fn is_paused(&self) -> bool {
        self.state.lock().unwrap().paused_at.is_some()
    }

    /// Returns false if already paused.
    pub fn pause(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        if s.paused_at.is_some() {
            return false;
        }
        s.paused_at = Some(Instant::now());
        true
    }

    /// Returns false if not paused.
    pub fn resume(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        match s.paused_at.take() {
            Some(at) => {
                s.paused_total += at.elapsed();
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_freezes_media_time() {
        let clock = MediaClock::default();
        assert!(clock.now().is_none());
        clock.start();
        std::thread::sleep(Duration::from_millis(30));
        assert!(clock.pause());
        let at_pause = clock.now().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(clock.now().unwrap(), at_pause, "time must not advance while paused");
        assert!(clock.resume());
        std::thread::sleep(Duration::from_millis(20));
        let after = clock.now().unwrap();
        assert!(after >= at_pause + Duration::from_millis(15));
        assert!(after < at_pause + Duration::from_millis(45), "paused time must be excluded");
    }
}
