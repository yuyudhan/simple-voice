// FilePath: crates/sv-domain/src/health/text.rs
//! The words and actions of every issue: the single source of what the banner, the tray, the
//! overlay, refused dictations and the log say.

use super::{Issue, IssueAction, IssueKind, Platform, Severity};

/// Seconds an open microphone may deliver no audio at all before the recording stops with
/// "No audio from …". Bluetooth and USB devices deliver their first buffer well within a second.
pub const NO_AUDIO_AFTER_SECS: u64 = 2;
/// Seconds of pure digital silence (every sample exactly zero) before a recording reports the
/// microphone as muted. A quiet room is never exactly zero; a mute switch or a zero input volume
/// is.
pub const SILENT_AFTER_SECS: u64 = 3;

/// Builds the issue for `kind`. `blocks` marks it as the reason a dictation is refused.
pub(super) fn issue(kind: IssueKind, blocks: bool, platform: Platform) -> Issue {
    let (title, message, actions, severity) = words(&kind, platform);
    Issue {
        kind,
        severity: if blocks { Severity::Blocking } else { severity },
        title,
        message,
        actions,
    }
}

fn machine(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOs => "this Mac",
        Platform::Linux => "this computer",
    }
}

fn sound_input(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOs => "System Settings → Sound → Input",
        Platform::Linux => "Settings → Sound → Input",
    }
}

/// A reason quoted inside a sentence, without its own closing punctuation.
fn clause(reason: &str) -> &str {
    reason.trim().trim_end_matches(['.', '!', '?', '…'])
}

type Words = (String, String, Vec<IssueAction>, Severity);

fn words(kind: &IssueKind, platform: Platform) -> Words {
    use IssueAction as A;
    use Severity::{Info, Warning};
    let mac = platform == Platform::MacOs;
    let here = machine(platform);
    match kind {
        IssueKind::NoMicrophone => (
            "No microphone connected".to_owned(),
            format!(
                "Simple Voice can't hear you: {} no microphone it can record from. Connect one \
                 (USB, a headset or AirPods) or choose an input in {}. Dictation works again as \
                 soon as one appears.",
                if mac {
                    "this Mac has"
                } else {
                    "the sound server offers"
                },
                sound_input(platform)
            ),
            vec![A::OpenSoundSettings, A::Recheck],
            Warning,
        ),
        IssueKind::NoSoundServer => (
            "No sound server running".to_owned(),
            "Simple Voice records through PipeWire or PulseAudio, and neither runs in this \
             session. Start it with “systemctl --user start pipewire pipewire-pulse” (or log out \
             and back in), then check again."
                .to_owned(),
            vec![A::OpenHelp, A::Recheck],
            Warning,
        ),
        IssueKind::SelectedMicrophoneMissing { requested, device } => (
            "Selected microphone not connected".to_owned(),
            format!(
                "“{requested}” isn't connected, so Simple Voice records from “{device}”. \
                 Reconnect it or choose another input."
            ),
            vec![A::ChooseMicrophone],
            Info,
        ),
        IssueKind::MicrophoneDisconnected { device } => (
            "Microphone disconnected".to_owned(),
            format!(
                "“{device}” went away while you were recording. Simple Voice kept what it heard \
                 before that. Reconnect it or choose another input."
            ),
            vec![A::ChooseMicrophone, A::OpenSoundSettings],
            Warning,
        ),
        IssueKind::MicrophoneNoAudio { device } => (
            format!("No audio from {device}"),
            format!(
                "The microphone opened but sent no sound at all for {NO_AUDIO_AFTER_SECS} \
                 seconds. Check that it is switched on, or choose another input."
            ),
            vec![A::ChooseMicrophone, A::OpenSoundSettings],
            Warning,
        ),
        IssueKind::MicrophoneSilent { device } => (
            format!("{device} sends only silence"),
            format!(
                "It recorded pure digital silence for {SILENT_AFTER_SECS} seconds, which usually \
                 means it is muted: a mute switch, or the input volume at zero in {}. Unmute it \
                 or choose another input.",
                sound_input(platform)
            ),
            vec![A::OpenSoundSettings, A::ChooseMicrophone],
            Warning,
        ),
        IssueKind::MicrophoneBusy { device } => (
            "Microphone in use by another app".to_owned(),
            format!(
                "“{device}” is held by another app. Close that app or choose another input, \
                 then try again."
            ),
            vec![A::ChooseMicrophone, A::Recheck],
            Warning,
        ),
        IssueKind::MicrophoneFailed { reason } => (
            "Microphone couldn't start".to_owned(),
            format!(
                "The audio system said: {}. Choose another input, or check again.",
                clause(reason)
            ),
            vec![A::ChooseMicrophone, A::Recheck],
            Warning,
        ),
        IssueKind::MicrophoneDenied if mac => (
            "Microphone access is off".to_owned(),
            "macOS doesn't allow Simple Voice to use the microphone. Turn on Simple Voice in \
             System Settings → Privacy & Security → Microphone; dictation works as soon as you \
             do."
            .to_owned(),
            vec![A::OpenMicrophonePrivacy],
            Warning,
        ),
        IssueKind::MicrophoneDenied => (
            "Microphone access is off".to_owned(),
            "The system refused access to the microphone. Check that no sandbox or policy keeps \
             Simple Voice from the sound server."
                .to_owned(),
            vec![A::OpenHelp, A::Recheck],
            Warning,
        ),
        IssueKind::MicrophoneNotAsked => (
            "Allow microphone access".to_owned(),
            "Simple Voice needs the microphone to hear you. macOS asks once; choose Allow when \
             it does."
                .to_owned(),
            vec![A::RequestMicrophone],
            Warning,
        ),
        IssueKind::AccessibilityDenied if mac => (
            "Pasting is off".to_owned(),
            "Without Accessibility access Simple Voice can't paste into other apps, so your text \
             is copied instead (press ⌘V). Turn on Simple Voice in System Settings → Privacy & \
             Security → Accessibility."
                .to_owned(),
            vec![A::OpenAccessibility],
            Warning,
        ),
        IssueKind::AccessibilityDenied => (
            "Pasting needs a helper tool".to_owned(),
            "No tool that can press Ctrl+V was found, so your text is copied instead (press \
             Ctrl+V). Install xdotool (X11), or ydotool with access to /dev/uinput (Wayland)."
                .to_owned(),
            vec![A::OpenHelp],
            Warning,
        ),
        IssueKind::EngineDown { reason } => (
            "Speech engine stopped".to_owned(),
            format!(
                "{}. Simple Voice restarts it on its own; restart it now if it doesn't come back.",
                clause(reason)
            ),
            vec![A::RestartEngine],
            Warning,
        ),
        IssueKind::EngineNotResponding { reason } => (
            "Speech engine not responding".to_owned(),
            format!(
                "{}. Restart the engine if dictations keep failing.",
                clause(reason)
            ),
            vec![A::RestartEngine],
            Warning,
        ),
        IssueKind::ModelMissing { name, .. } => (
            format!("{name} isn't downloaded"),
            format!(
                "Dictation needs the {name} voice model on {here}. Download it once, or choose \
                 another model."
            ),
            vec![A::DownloadModel, A::OpenModelSettings],
            Info,
        ),
        IssueKind::ModelDownloading { name, percent, .. } => (
            format!("Downloading {name}"),
            format!("{percent}% done. Dictation works as soon as the download finishes."),
            vec![A::OpenModelSettings],
            Info,
        ),
        IssueKind::ModelUnavailable { name, reason, .. } => (
            format!("{name} can't run here"),
            format!("{}. Choose another voice model.", clause(reason)),
            vec![A::OpenModelSettings],
            Warning,
        ),
        IssueKind::ModelFailed { name, reason, .. } => (
            format!("{name} failed"),
            format!(
                "The last dictation with it failed: {}. If this repeats, delete the model in \
                 Settings → Transcription and download it again.",
                clause(reason)
            ),
            vec![A::OpenModelSettings, A::RestartEngine],
            Warning,
        ),
        IssueKind::ProviderKeyMissing { provider } => (
            format!("{provider} API key missing"),
            format!(
                "Transcription is set to {provider}, which needs your API key. Add it in \
                 Settings → Transcription; until then Simple Voice transcribes on {here}."
            ),
            vec![A::OpenProviderSettings],
            Warning,
        ),
        IssueKind::ProviderKeyInvalid { provider } => (
            format!("{provider} rejected the API key"),
            format!(
                "{provider} refused the saved key (revoked or mistyped), so Simple Voice \
                 transcribed on {here} instead. Paste a valid key in Settings → Transcription."
            ),
            vec![A::OpenProviderSettings],
            Warning,
        ),
        IssueKind::ProviderRateLimited { provider } => (
            format!("{provider} rate limit reached"),
            format!(
                "{provider} refuses more requests from your key for now, so Simple Voice \
                 transcribes on {here} meanwhile. Wait a minute, or check your plan's limits."
            ),
            vec![A::OpenProviderSettings],
            Warning,
        ),
        IssueKind::ProviderOffline { provider } => (
            format!("Can't reach {provider}"),
            format!(
                "Simple Voice couldn't connect to {provider} in time, so it transcribed on {here} \
                 instead. Check your internet connection."
            ),
            vec![A::OpenProviderSettings],
            Warning,
        ),
        IssueKind::ShortcutUnavailable { shortcut } => (
            format!("Shortcut {shortcut} isn't working"),
            format!(
                "Another app already uses {shortcut}, so Simple Voice can't listen for it. \
                 Choose a different shortcut."
            ),
            vec![A::OpenShortcutSettings],
            Warning,
        ),
        IssueKind::WaylandShortcuts => (
            "Set up a system shortcut".to_owned(),
            "This Wayland desktop doesn't let apps register global shortcuts. Add a custom \
             shortcut in Settings → Keyboard that runs “simple-voice --toggle”."
                .to_owned(),
            vec![A::OpenHelp],
            Info,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quoted_reason_loses_its_own_full_stop() {
        assert_eq!(
            clause("The speech engine stopped (signal 9)."),
            "The speech engine stopped (signal 9)"
        );
        assert_eq!(clause("  gone…  "), "gone");
        let issue = issue(
            IssueKind::EngineDown {
                reason: "The speech engine stopped.".to_owned(),
            },
            true,
            Platform::MacOs,
        );
        assert_eq!(
            issue.message,
            "The speech engine stopped. Simple Voice restarts it on its own; restart it now if it \
             doesn't come back."
        );
        assert_eq!(issue.severity, Severity::Blocking);
    }

    #[test]
    fn the_silence_thresholds_appear_in_the_words() {
        let device = "USB Mic".to_owned();
        let silent = issue(
            IssueKind::MicrophoneSilent {
                device: device.clone(),
            },
            false,
            Platform::MacOs,
        );
        assert!(silent
            .message
            .contains(&format!("{SILENT_AFTER_SECS} seconds")));
        let none = issue(
            IssueKind::MicrophoneNoAudio { device },
            false,
            Platform::Linux,
        );
        assert!(none
            .message
            .contains(&format!("{NO_AUDIO_AFTER_SECS} seconds")));
    }
}
