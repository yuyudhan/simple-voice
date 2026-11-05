// FilePath: src-tauri/src/platform/output_mute/session.rs
//! Mutes system output for one recording and unmutes exactly what it muted. Muting waits for the
//! start cue to be heard; stopping or cancelling unmutes the device that was muted (even when the
//! default output changed meanwhile) and puts back its volume when it was silenced that way.
//! Output the user had muted is left muted. What was muted is also kept in a marker file until it
//! is unmuted, so a crash or quit mid-recording is undone at the next launch.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sv_domain::MutedOutput;
use tokio::sync::Notify;

use super::OutputControl;
use crate::state::lock;

/// Lets the start cue finish before system output is muted.
const MUTE_DELAY: Duration = Duration::from_millis(250);

/// Serializes muting, unmuting and the marker across recordings: a recording that starts while
/// the previous one is still unmuting neither takes that mute for the user's nor loses its marker
/// to the other's cleanup.
static OUTPUT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The muting of one recording, from its start until [`Muting::end`].
#[derive(Debug)]
pub(crate) struct Muting<O: OutputControl> {
    output: O,
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    slot: Mutex<Slot>,
    cue_played: Notify,
    marker: Option<PathBuf>,
}

#[derive(Debug, Default)]
struct Slot {
    /// What this recording muted; `None` until the mute lands, or when output was muted already.
    muted: Option<MutedOutput>,
    ended: bool,
}

impl<O: OutputControl> Muting<O> {
    /// Mutes in the background once the start cue played, so the microphone never waits for it.
    pub(crate) fn start(output: O, marker: Option<PathBuf>) -> Self {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot::default()),
            cue_played: Notify::new(),
            marker,
        });
        let task_output = output.clone();
        let task_shared = Arc::clone(&shared);
        tokio::spawn(async move { task_shared.mute(&task_output).await });
        Self { output, shared }
    }

    /// The start cue is playing; muting waits for it to be heard.
    pub(crate) fn cue_played(&self) {
        self.shared.cue_played.notify_one();
    }

    /// Unmutes what the recording muted; a mute still in flight unmutes itself when it lands.
    pub(crate) async fn end(self) {
        let muted = {
            let mut slot = lock(&self.shared.slot);
            slot.ended = true;
            slot.muted.take()
        };
        // A mute still waiting for the cue wakes, sees the end and leaves output alone.
        self.shared.cue_played.notify_one();
        if let Some(muted) = muted {
            let _output = OUTPUT.lock().await;
            if unmute(&self.output, muted).await {
                self.shared.remove_marker();
            }
        }
    }
}

impl Shared {
    async fn mute<O: OutputControl>(&self, output: &O) {
        self.cue_played.notified().await;
        tokio::time::sleep(MUTE_DELAY).await;
        let _output = OUTPUT.lock().await;
        if lock(&self.slot).ended {
            return;
        }
        let muted = match output.mute_output().await {
            Ok(Some(muted)) => muted,
            Ok(None) => {
                tracing::info!("output was muted already; leaving it muted");
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "could not mute output");
                return;
            }
        };
        // The recording may have ended while the platform was answering; its end found nothing
        // to unmute then, so the unmute happens here.
        let ended = {
            let mut slot = lock(&self.slot);
            if !slot.ended {
                self.save_marker(&muted);
                slot.muted = Some(muted.clone());
            }
            slot.ended
        };
        if ended {
            unmute(output, muted).await;
        }
    }

    fn save_marker(&self, muted: &MutedOutput) {
        let Some(path) = &self.marker else { return };
        let written = serde_json::to_vec(muted)
            .map_err(|error| error.to_string())
            .and_then(|json| std::fs::write(path, json).map_err(|error| error.to_string()));
        if let Err(error) = written {
            tracing::warn!(%error, "could not record the mute for crash recovery");
        }
    }

    fn remove_marker(&self) {
        if let Some(path) = &self.marker {
            remove_marker(path);
        }
    }
}

/// Unmutes `muted`; false when the platform failed, true also when the device is gone, since a
/// device that is no longer there has nothing left to unmute.
async fn unmute<O: OutputControl>(output: &O, muted: MutedOutput) -> bool {
    let device = muted.device.clone();
    match output.unmute_output(muted).await {
        Ok(true) => true,
        Ok(false) => {
            tracing::info!(device, "the muted output device is gone; nothing to unmute");
            true
        }
        Err(error) => {
            tracing::warn!(%error, device, "could not unmute output");
            false
        }
    }
}

fn remove_marker(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, "could not remove the mute marker"),
    }
}

/// What a recording that never ended (crash, quit) left muted, read from its marker when the app
/// starts and before any new recording can write one.
#[derive(Debug, Default)]
pub(crate) struct PendingRecovery {
    muted: Mutex<Option<MutedOutput>>,
}

impl PendingRecovery {
    /// Takes the marker: reads and removes it. An unreadable marker is dropped, since nothing in
    /// it could be unmuted.
    pub(crate) fn load(marker: Option<&Path>) -> Self {
        let muted = marker.and_then(|path| {
            let bytes = std::fs::read(path).ok()?;
            remove_marker(path);
            match serde_json::from_slice::<MutedOutput>(&bytes) {
                Ok(muted) => Some(muted),
                Err(error) => {
                    tracing::warn!(%error, "ignoring an unreadable mute marker");
                    None
                }
            }
        });
        Self {
            muted: Mutex::new(muted),
        }
    }

    /// Unmutes the pending device; on failure it stays pending for the next helper start.
    pub(crate) async fn apply<O: OutputControl>(&self, output: &O) {
        let Some(muted) = lock(&self.muted).take() else {
            return;
        };
        tracing::info!(?muted, "unmuting what an interrupted recording muted");
        let _output = OUTPUT.lock().await;
        if !unmute(output, muted.clone()).await {
            *lock(&self.muted) = Some(muted);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use sv_domain::{AppError, AppResult};

    use super::*;

    const SPEAKERS: &str = "BuiltInSpeakerDevice";
    const AIRPODS: &str = "AirPods";
    const HDMI: &str = "HDMI";
    /// Two browser tabs and a music app, all playing at once.
    const SOURCES: [&str; 3] = ["chrome tab 1", "chrome tab 2", "Music"];

    #[derive(Debug, Clone, Copy)]
    struct Device {
        muted: bool,
        volume: f32,
        /// False for outputs without a mute control (some HDMI and USB devices).
        has_mute: bool,
    }

    /// A sound system: output devices, the default one, the sources playing into it, and every
    /// call in order.
    #[derive(Debug)]
    struct World {
        devices: BTreeMap<&'static str, Device>,
        default: &'static str,
        calls: Vec<String>,
    }

    #[derive(Debug, Clone)]
    struct Fake {
        world: Arc<Mutex<World>>,
        /// How long `mute_output` takes to answer.
        mute_delay: Duration,
    }

    impl Fake {
        /// Speakers (with a mute control) as the default output, AirPods and an HDMI output
        /// without a mute control also connected.
        fn new(speakers_muted: bool) -> Self {
            let device = |muted, volume, has_mute| Device {
                muted,
                volume,
                has_mute,
            };
            let devices = BTreeMap::from([
                (SPEAKERS, device(speakers_muted, 0.37, true)),
                (AIRPODS, device(false, 0.6, true)),
                (HDMI, device(false, 0.37, false)),
            ]);
            Self {
                world: Arc::new(Mutex::new(World {
                    devices,
                    default: SPEAKERS,
                    calls: Vec::new(),
                })),
                mute_delay: Duration::ZERO,
            }
        }

        fn device(&self, name: &str) -> Device {
            lock(&self.world).devices[name]
        }

        fn set_default(&self, name: &'static str) {
            lock(&self.world).default = name;
        }

        /// The sources the listener hears: all of them unless the default output is silent.
        fn audible(&self) -> Vec<&'static str> {
            let world = lock(&self.world);
            let output = world.devices[world.default];
            if output.muted || output.volume == 0.0 {
                Vec::new()
            } else {
                SOURCES.to_vec()
            }
        }

        fn calls(&self) -> Vec<String> {
            lock(&self.world).calls.clone()
        }
    }

    impl OutputControl for Fake {
        async fn mute_output(&self) -> AppResult<Option<MutedOutput>> {
            tokio::time::sleep(self.mute_delay).await;
            let mut world = lock(&self.world);
            let name = world.default;
            world.calls.push(format!("mute {name}"));
            let device = world.devices.get_mut(name).expect("default device exists");
            if device.has_mute {
                let was_muted = std::mem::replace(&mut device.muted, true);
                return Ok((!was_muted).then(|| MutedOutput {
                    device: name.to_owned(),
                    volume: None,
                }));
            }
            if device.volume == 0.0 {
                return Ok(None);
            }
            let volume = std::mem::replace(&mut device.volume, 0.0);
            Ok(Some(MutedOutput {
                device: name.to_owned(),
                volume: Some(volume),
            }))
        }

        async fn unmute_output(&self, muted: MutedOutput) -> AppResult<bool> {
            let mut world = lock(&self.world);
            world.calls.push(format!("unmute {}", muted.device));
            let Some(device) = world.devices.get_mut(muted.device.as_str()) else {
                return Ok(false);
            };
            match muted.volume {
                Some(volume) => device.volume = volume,
                None => device.muted = false,
            }
            Ok(true)
        }
    }

    static MARKERS: AtomicUsize = AtomicUsize::new(0);

    fn marker() -> PathBuf {
        let n = MARKERS.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("sv-mute-{}-{n}.json", std::process::id()))
    }

    /// Waits until `output` has seen `count` calls (muting runs in the background).
    async fn calls_reach(output: &Fake, count: usize) {
        let started = Instant::now();
        while output.calls().len() < count {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "{:?}",
                output.calls()
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[tokio::test]
    async fn every_source_is_silent_while_recording_and_heard_again_after() {
        let output = Fake::new(false);
        assert_eq!(output.audible(), SOURCES);
        let muting = Muting::start(output.clone(), None);
        muting.cue_played();
        calls_reach(&output, 1).await;
        assert!(output.audible().is_empty(), "a source is still heard");
        muting.end().await;
        assert_eq!(output.audible(), SOURCES);
        assert_eq!(
            output.calls(),
            ["mute BuiltInSpeakerDevice", "unmute BuiltInSpeakerDevice"]
        );
        let speakers = output.device(SPEAKERS);
        assert!(!speakers.muted);
        assert!((speakers.volume - 0.37).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn output_is_muted_only_after_the_start_cue() {
        let output = Fake::new(false);
        let muting = Muting::start(output.clone(), None);
        tokio::time::sleep(MUTE_DELAY * 2).await;
        assert!(
            output.calls().is_empty(),
            "muted before the start cue was heard"
        );
        muting.cue_played();
        calls_reach(&output, 1).await;
        assert!(output.audible().is_empty());
        muting.end().await;
        assert_eq!(output.audible(), SOURCES);
    }

    #[tokio::test]
    async fn output_the_user_had_muted_stays_muted() {
        let output = Fake::new(true);
        let muting = Muting::start(output.clone(), None);
        muting.cue_played();
        calls_reach(&output, 1).await;
        muting.end().await;
        assert_eq!(output.calls(), ["mute BuiltInSpeakerDevice"]);
        assert!(output.device(SPEAKERS).muted);
    }

    #[tokio::test]
    async fn a_device_without_mute_gets_its_exact_volume_back() {
        let output = Fake::new(false);
        output.set_default(HDMI);
        let muting = Muting::start(output.clone(), None);
        muting.cue_played();
        calls_reach(&output, 1).await;
        assert!(output.audible().is_empty());
        assert_eq!(output.device(HDMI).volume, 0.0);
        muting.end().await;
        assert!((output.device(HDMI).volume - 0.37).abs() < f32::EPSILON);
        assert_eq!(output.audible(), SOURCES);
    }

    #[tokio::test]
    async fn the_muted_device_is_unmuted_after_the_output_was_switched() {
        let output = Fake::new(false);
        let muting = Muting::start(output.clone(), None);
        muting.cue_played();
        calls_reach(&output, 1).await;
        // AirPods connect mid-recording and become the default output.
        output.set_default(AIRPODS);
        muting.end().await;
        assert_eq!(
            output.calls(),
            ["mute BuiltInSpeakerDevice", "unmute BuiltInSpeakerDevice"]
        );
        assert!(!output.device(SPEAKERS).muted);
        assert!(!output.device(AIRPODS).muted);
        assert_eq!(output.audible(), SOURCES);
    }

    #[tokio::test]
    async fn a_recording_cancelled_before_the_cue_never_mutes() {
        let output = Fake::new(false);
        let muting = Muting::start(output.clone(), None);
        muting.end().await;
        tokio::time::sleep(MUTE_DELAY * 2).await;
        assert!(output.calls().is_empty());
        assert_eq!(output.audible(), SOURCES);
    }

    #[tokio::test]
    async fn a_recording_cancelled_while_the_mute_is_in_flight_unmutes() {
        let path = marker();
        let mut output = Fake::new(false);
        output.mute_delay = Duration::from_millis(100);
        let muting = Muting::start(output.clone(), Some(path.clone()));
        muting.cue_played();
        tokio::time::sleep(MUTE_DELAY + Duration::from_millis(20)).await;
        muting.end().await;
        calls_reach(&output, 2).await;
        assert_eq!(
            output.calls(),
            ["mute BuiltInSpeakerDevice", "unmute BuiltInSpeakerDevice"]
        );
        assert_eq!(output.audible(), SOURCES);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn quick_repeated_recordings_end_audible_at_the_original_volume() {
        let path = marker();
        let mut output = Fake::new(false);
        output.mute_delay = Duration::from_millis(30);
        for held in [0, 120, 260, 290, 400] {
            let muting = Muting::start(output.clone(), Some(path.clone()));
            muting.cue_played();
            tokio::time::sleep(Duration::from_millis(held)).await;
            muting.end().await;
        }
        tokio::time::sleep(MUTE_DELAY * 2).await;
        let speakers = output.device(SPEAKERS);
        assert!(!speakers.muted, "{:?}", output.calls());
        assert!((speakers.volume - 0.37).abs() < f32::EPSILON);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn the_marker_holds_the_mute_until_it_is_undone() {
        let path = marker();
        let output = Fake::new(false);
        let muting = Muting::start(output.clone(), Some(path.clone()));
        muting.cue_played();
        calls_reach(&output, 1).await;
        let started = Instant::now();
        while !path.exists() {
            assert!(started.elapsed() < Duration::from_secs(5));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let saved: MutedOutput = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.device, SPEAKERS);
        muting.end().await;
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn a_crashed_recording_is_unmuted_at_the_next_launch() {
        let path = marker();
        // The recording muted the speakers, then the app died mid-recording.
        let mut output = Fake::new(true);
        let crashed = MutedOutput {
            device: SPEAKERS.to_owned(),
            volume: None,
        };
        std::fs::write(&path, serde_json::to_vec(&crashed).unwrap()).unwrap();

        let pending = PendingRecovery::load(Some(&path));
        assert!(
            !path.exists(),
            "the marker is taken before any new recording"
        );
        pending.apply(&output).await;
        assert_eq!(output.calls(), ["unmute BuiltInSpeakerDevice"]);
        assert_eq!(output.audible(), SOURCES);

        // Applied once: a later helper restart changes nothing.
        output = Fake::new(true);
        pending.apply(&output).await;
        assert!(output.calls().is_empty());
    }

    #[tokio::test]
    async fn a_failed_recovery_stays_pending_and_no_marker_means_nothing_to_do() {
        #[derive(Debug, Clone)]
        struct Down;
        impl OutputControl for Down {
            async fn mute_output(&self) -> AppResult<Option<MutedOutput>> {
                Err(AppError::engine("helper down"))
            }
            async fn unmute_output(&self, _muted: MutedOutput) -> AppResult<bool> {
                Err(AppError::engine("helper down"))
            }
        }
        let path = marker();
        let crashed = MutedOutput {
            device: HDMI.to_owned(),
            volume: Some(0.37),
        };
        std::fs::write(&path, serde_json::to_vec(&crashed).unwrap()).unwrap();
        let pending = PendingRecovery::load(Some(&path));
        pending.apply(&Down).await;
        let output = Fake::new(false);
        lock(&output.world).devices.get_mut(HDMI).unwrap().volume = 0.0;
        pending.apply(&output).await;
        assert_eq!(output.calls(), ["unmute HDMI"]);
        assert!((output.device(HDMI).volume - 0.37).abs() < f32::EPSILON);

        let none = PendingRecovery::load(Some(&marker()));
        none.apply(&output).await;
        assert_eq!(output.calls(), ["unmute HDMI"]);
    }

    #[tokio::test]
    async fn a_device_gone_by_the_end_counts_as_unmuted() {
        let path = marker();
        let output = Fake::new(false);
        output.set_default(AIRPODS);
        let muting = Muting::start(output.clone(), Some(path.clone()));
        muting.cue_played();
        calls_reach(&output, 1).await;
        // The AirPods disconnect mid-recording; output falls back to the speakers.
        {
            let mut world = lock(&output.world);
            world.devices.remove(AIRPODS);
            world.default = SPEAKERS;
        }
        muting.end().await;
        assert_eq!(output.calls(), ["mute AirPods", "unmute AirPods"]);
        assert!(!path.exists(), "a gone device leaves nothing pending");
        assert_eq!(output.audible(), SOURCES);
    }
}
