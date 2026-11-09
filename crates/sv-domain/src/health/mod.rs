// FilePath: crates/sv-domain/src/health/mod.rs
//! The app's health: every condition that stops or degrades dictation, each with the words the
//! user reads and the actions that fix it.
//!
//! The app records what it observes in [`HealthFacts`] (the microphone check, the last capture,
//! permissions, the engine helper, the voice model, the cloud provider, shortcuts) and
//! [`evaluate`] turns them into [`Health`]. That one derivation feeds the banner in the main
//! window, the tray, the overlay, the refusal of a dictation that cannot work and the log, so
//! they always give the same reason. The UI renders `kind`, `title`, `message` and `actions`
//! as they are and never inspects the text.

mod input;
mod text;

use serde::{Deserialize, Serialize};

pub use self::input::{CaptureProblem, InputState};
pub use self::text::{NO_AUDIO_AFTER_SECS, SILENT_AFTER_SECS};
use crate::{AppError, EngineState, EngineStatus, PermissionStatus, Permissions};

/// The operating system whose words, settings panes and helper tools the texts name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    MacOs,
    Linux,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            Platform::MacOs
        }
    }
}

/// How much an issue gets in the way. Issues are listed most severe first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Dictation cannot work; a start is refused with this issue's reason.
    Blocking,
    /// Dictation works, but worse or not at all in some cases.
    Warning,
    /// Worth knowing; nothing is broken.
    Info,
}

/// What the user can do about an issue, offered as buttons in the order given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueAction {
    /// The system's sound input settings (macOS Sound → Input, GNOME Sound).
    OpenSoundSettings,
    /// macOS Privacy & Security → Microphone.
    OpenMicrophonePrivacy,
    /// Ask macOS for microphone access (its one-time prompt).
    RequestMicrophone,
    /// macOS Privacy & Security → Accessibility.
    OpenAccessibility,
    /// Settings → Dictation → Input device.
    ChooseMicrophone,
    /// Check the microphone and permissions again now.
    Recheck,
    /// Restart the engine helper.
    RestartEngine,
    /// Download the issue's `model`.
    DownloadModel,
    /// Settings → Transcription, where models are chosen, deleted and downloaded.
    OpenModelSettings,
    /// Settings → Transcription → the cloud provider's key.
    OpenProviderSettings,
    /// Settings → Dictation → shortcuts.
    OpenShortcutSettings,
    /// The troubleshooting guide's section on this issue.
    OpenHelp,
}

/// Why a cloud transcription request failed, as far as the app can act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFault {
    /// The provider rejected the key (HTTP 401 or 403).
    Unauthorized,
    /// The provider refused more requests for now (HTTP 429).
    RateLimited,
    /// The provider could not be reached (no network, DNS, TLS).
    Offline,
    /// The provider did not answer in time.
    TimedOut,
}

/// One condition, ready to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    #[serde(flatten)]
    pub kind: IssueKind,
    pub severity: Severity,
    /// A few words, for the banner heading, the tray and the overlay.
    pub title: String,
    /// What is wrong and the one thing that fixes it, in plain sentences.
    pub message: String,
    pub actions: Vec<IssueAction>,
}

/// Every condition the app reports. Variants carry what the texts and actions need.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum IssueKind {
    NoMicrophone,
    NoSoundServer,
    SelectedMicrophoneMissing {
        requested: String,
        device: String,
    },
    MicrophoneDisconnected {
        device: String,
    },
    MicrophoneNoAudio {
        device: String,
    },
    MicrophoneSilent {
        device: String,
    },
    MicrophoneBusy {
        device: String,
    },
    MicrophoneFailed {
        reason: String,
    },
    MicrophoneDenied,
    MicrophoneNotAsked,
    AccessibilityDenied,
    EngineDown {
        reason: String,
    },
    EngineNotResponding {
        reason: String,
    },
    ModelMissing {
        model: String,
        name: String,
    },
    ModelDownloading {
        model: String,
        name: String,
        percent: u8,
    },
    ModelUnavailable {
        model: String,
        name: String,
        reason: String,
    },
    ModelFailed {
        model: String,
        name: String,
        reason: String,
    },
    ProviderKeyMissing {
        provider: String,
    },
    ProviderKeyInvalid {
        provider: String,
    },
    ProviderRateLimited {
        provider: String,
    },
    ProviderOffline {
        provider: String,
    },
    ShortcutUnavailable {
        shortcut: String,
    },
    WaylandShortcuts,
}

/// Everything wrong right now, most severe first; empty when all is well.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub issues: Vec<Issue>,
}

impl Health {
    /// The issue that refuses a dictation, if any.
    pub fn blocking(&self) -> Option<&Issue> {
        self.issues
            .iter()
            .find(|issue| issue.severity == Severity::Blocking)
    }
}

/// The selected voice model as the engine reports it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModelFact {
    /// Not known (the engine is not up yet); no claim either way.
    #[default]
    Unknown,
    Ready,
    Missing {
        model: String,
        name: String,
    },
    Downloading {
        model: String,
        name: String,
        percent: u8,
    },
    /// The engine cannot run it on this machine (OS too old, Apple Intelligence off).
    Unavailable {
        model: String,
        name: String,
        reason: String,
    },
    /// It is downloaded but the last transcription with it failed (did not load, damaged).
    Failed {
        model: String,
        name: String,
        reason: String,
    },
}

/// Where transcription goes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProviderFact {
    /// On-device models only.
    #[default]
    Local,
    /// A cloud provider with the user's key, falling back to the on-device model on failure.
    Cloud {
        provider: String,
        has_key: bool,
        fault: Option<ProviderFault>,
    },
}

/// What the app last observed; [`evaluate`] derives the issues from it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HealthFacts {
    pub input: InputState,
    /// What went wrong with the latest recording; cleared by a recording that works and by a
    /// change in the connected microphones.
    pub capture: Option<CaptureProblem>,
    pub permissions: Option<Permissions>,
    pub engine: EngineStatus,
    /// The engine runs but its latest request timed out, in the client's words.
    pub engine_stalled: Option<String>,
    pub model: ModelFact,
    pub provider: ProviderFact,
    /// The shortcut another app holds, when registering it failed.
    pub shortcut_taken: Option<String>,
    /// Linux on Wayland: apps cannot register global shortcuts there.
    pub wayland_session: bool,
}

/// Derives every issue from the facts, most severe first.
pub fn evaluate(facts: &HealthFacts, platform: Platform) -> Health {
    let mut kinds = Vec::new();
    input_issues(facts, &mut kinds);
    if let Some(permissions) = facts.permissions {
        permission_issues(permissions, platform, &mut kinds);
    }
    let local = runs_locally(&facts.provider);
    match facts.engine.state {
        EngineState::Down => {
            let reason = facts
                .engine
                .reason
                .clone()
                .unwrap_or_else(|| "The speech engine is not running".to_owned());
            kinds.push((IssueKind::EngineDown { reason }, local));
        }
        EngineState::Running => {
            if let Some(reason) = &facts.engine_stalled {
                kinds.push((
                    IssueKind::EngineNotResponding {
                        reason: reason.clone(),
                    },
                    false,
                ));
            }
            if let Some(kind) = model_issue(&facts.model) {
                let blocks = local && !matches!(kind, IssueKind::ModelFailed { .. });
                kinds.push((kind, blocks));
            }
        }
        EngineState::Starting => {}
    }
    if let ProviderFact::Cloud {
        provider,
        has_key,
        fault,
    } = &facts.provider
    {
        let provider = provider.clone();
        let kind = match (has_key, fault) {
            (false, _) => Some(IssueKind::ProviderKeyMissing { provider }),
            (true, Some(ProviderFault::Unauthorized)) => {
                Some(IssueKind::ProviderKeyInvalid { provider })
            }
            (true, Some(ProviderFault::RateLimited)) => {
                Some(IssueKind::ProviderRateLimited { provider })
            }
            (true, Some(ProviderFault::Offline | ProviderFault::TimedOut)) => {
                Some(IssueKind::ProviderOffline { provider })
            }
            (true, None) => None,
        };
        kinds.extend(kind.map(|kind| (kind, false)));
    }
    if let Some(shortcut) = &facts.shortcut_taken {
        kinds.push((
            IssueKind::ShortcutUnavailable {
                shortcut: shortcut.clone(),
            },
            false,
        ));
    }
    if facts.wayland_session && platform == Platform::Linux {
        kinds.push((IssueKind::WaylandShortcuts, false));
    }

    // The same issue can come from two facts (a refused capture and the permission check); it
    // is listed once and blocks when either says so.
    let mut unique: Pending = Vec::with_capacity(kinds.len());
    for (kind, blocks) in kinds {
        match unique.iter_mut().find(|(seen, _)| *seen == kind) {
            Some(entry) => entry.1 |= blocks,
            None => unique.push((kind, blocks)),
        }
    }
    let mut issues: Vec<Issue> = unique
        .into_iter()
        .map(|(kind, blocks)| text::issue(kind, blocks, platform))
        .collect();
    // Stable, so issues of one severity keep the order above (microphone first).
    issues.sort_by_key(|issue| issue.severity);
    Health { issues }
}

/// Cloud transcription without a key falls back to the on-device model every time.
fn runs_locally(provider: &ProviderFact) -> bool {
    match provider {
        ProviderFact::Local => true,
        ProviderFact::Cloud { has_key, .. } => !has_key,
    }
}

/// Issues are pushed as `(kind, blocks_dictation)`; the texts decide the rest.
type Pending = Vec<(IssueKind, bool)>;

fn input_issues(facts: &HealthFacts, kinds: &mut Pending) {
    match &facts.input {
        InputState::NoSoundServer => kinds.push((IssueKind::NoSoundServer, true)),
        InputState::NoDevice => kinds.push((IssueKind::NoMicrophone, true)),
        InputState::FellBack { requested, device } => kinds.push((
            IssueKind::SelectedMicrophoneMissing {
                requested: requested.clone(),
                device: device.clone(),
            },
            false,
        )),
        InputState::Unknown | InputState::Ready { .. } => {}
    }
    if let Some(problem) = &facts.capture {
        kinds.push(capture_issue(problem));
    }
}

/// The issue a capture problem shows, and whether it refuses the next dictation.
fn capture_issue(problem: &CaptureProblem) -> (IssueKind, bool) {
    match problem {
        CaptureProblem::NoDevice => (IssueKind::NoMicrophone, true),
        CaptureProblem::PermissionDenied => (IssueKind::MicrophoneDenied, true),
        CaptureProblem::Busy { device } => (
            IssueKind::MicrophoneBusy {
                device: device.clone(),
            },
            false,
        ),
        CaptureProblem::Disconnected { device } => (
            IssueKind::MicrophoneDisconnected {
                device: device.clone(),
            },
            false,
        ),
        CaptureProblem::NoAudio { device } => (
            IssueKind::MicrophoneNoAudio {
                device: device.clone(),
            },
            false,
        ),
        CaptureProblem::Silent { device } => (
            IssueKind::MicrophoneSilent {
                device: device.clone(),
            },
            false,
        ),
        CaptureProblem::Failed { reason, .. } => (
            IssueKind::MicrophoneFailed {
                reason: reason.clone(),
            },
            false,
        ),
    }
}

fn permission_issues(permissions: Permissions, platform: Platform, kinds: &mut Pending) {
    // Linux has no microphone permission; its helper reports it granted.
    if platform == Platform::MacOs {
        match permissions.microphone {
            PermissionStatus::Denied | PermissionStatus::Restricted => {
                kinds.push((IssueKind::MicrophoneDenied, true));
            }
            PermissionStatus::NotDetermined => kinds.push((IssueKind::MicrophoneNotAsked, false)),
            PermissionStatus::Granted => {}
        }
    }
    // On Linux "accessibility" means a tool that can press Ctrl+V was found.
    if permissions.accessibility != PermissionStatus::Granted {
        kinds.push((IssueKind::AccessibilityDenied, false));
    }
}

fn model_issue(model: &ModelFact) -> Option<IssueKind> {
    match model.clone() {
        ModelFact::Unknown | ModelFact::Ready => None,
        ModelFact::Missing { model, name } => Some(IssueKind::ModelMissing { model, name }),
        ModelFact::Downloading {
            model,
            name,
            percent,
        } => Some(IssueKind::ModelDownloading {
            model,
            name,
            percent,
        }),
        ModelFact::Unavailable {
            model,
            name,
            reason,
        } => Some(IssueKind::ModelUnavailable {
            model,
            name,
            reason,
        }),
        ModelFact::Failed {
            model,
            name,
            reason,
        } => Some(IssueKind::ModelFailed {
            model,
            name,
            reason,
        }),
    }
}

impl CaptureProblem {
    /// The issue the user sees for this problem, worded for `platform`.
    pub fn issue(&self, platform: Platform) -> Issue {
        let (kind, blocks) = capture_issue(self);
        text::issue(kind, blocks, platform)
    }
}

impl From<CaptureProblem> for AppError {
    fn from(problem: CaptureProblem) -> AppError {
        AppError::Audio(problem.issue(Platform::current()).sentence())
    }
}

impl Issue {
    /// Title and message as one line, for logs, errors and the command line.
    pub fn sentence(&self) -> String {
        format!("{}. {}", self.title, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue_kinds(facts: &HealthFacts, platform: Platform) -> Vec<IssueKind> {
        evaluate(facts, platform)
            .issues
            .into_iter()
            .map(|issue| issue.kind)
            .collect()
    }

    fn granted() -> Permissions {
        Permissions {
            microphone: PermissionStatus::Granted,
            accessibility: PermissionStatus::Granted,
            speech: PermissionStatus::Granted,
            system_audio: PermissionStatus::Granted,
        }
    }

    fn running() -> HealthFacts {
        HealthFacts {
            input: InputState::Ready {
                device: "MacBook Pro Microphone".to_owned(),
            },
            permissions: Some(granted()),
            engine: EngineStatus::running(),
            model: ModelFact::Ready,
            ..HealthFacts::default()
        }
    }

    #[test]
    fn a_healthy_app_has_no_issues() {
        assert_eq!(evaluate(&running(), Platform::MacOs), Health::default());
        assert_eq!(
            evaluate(&HealthFacts::default(), Platform::Linux),
            Health::default()
        );
    }

    #[test]
    fn no_input_device_blocks_dictation_with_the_sound_settings_fix() {
        let facts = HealthFacts {
            input: InputState::NoDevice,
            ..running()
        };
        let health = evaluate(&facts, Platform::MacOs);
        let blocking = health.blocking().cloned();
        let Some(issue) = blocking else {
            panic!("no blocking issue for a missing microphone: {health:?}");
        };
        assert_eq!(issue.kind, IssueKind::NoMicrophone);
        assert_eq!(issue.title, "No microphone connected");
        assert_eq!(
            issue.actions,
            vec![IssueAction::OpenSoundSettings, IssueAction::Recheck]
        );
        assert!(
            issue.message.contains("System Settings → Sound → Input"),
            "{}",
            issue.message
        );
        let linux = evaluate(&facts, Platform::Linux);
        assert!(
            linux.issues[0].message.contains("Settings → Sound"),
            "{}",
            linux.issues[0].message
        );
        assert!(!linux.issues[0].message.contains("System Settings"));
    }

    #[test]
    fn a_capture_that_found_no_device_reports_the_same_issue_once() {
        let facts = HealthFacts {
            input: InputState::NoDevice,
            capture: Some(CaptureProblem::NoDevice),
            ..running()
        };
        assert_eq!(
            issue_kinds(&facts, Platform::MacOs),
            vec![IssueKind::NoMicrophone]
        );
    }

    #[test]
    fn linux_without_a_sound_server_says_so() {
        let facts = HealthFacts {
            input: InputState::NoSoundServer,
            ..running()
        };
        let health = evaluate(&facts, Platform::Linux);
        assert_eq!(
            health.blocking().map(|issue| &issue.kind),
            Some(&IssueKind::NoSoundServer)
        );
        assert!(health.issues[0].message.contains("PipeWire"));
    }

    #[test]
    fn a_missing_selected_microphone_names_the_fallback_without_blocking() {
        let facts = HealthFacts {
            input: InputState::FellBack {
                requested: "AirPods Pro".to_owned(),
                device: "MacBook Pro Microphone".to_owned(),
            },
            ..running()
        };
        let health = evaluate(&facts, Platform::MacOs);
        assert!(health.blocking().is_none());
        let issue = &health.issues[0];
        assert_eq!(issue.severity, Severity::Info);
        assert!(issue.message.contains("“AirPods Pro”"), "{}", issue.message);
        assert!(
            issue.message.contains("“MacBook Pro Microphone”"),
            "{}",
            issue.message
        );
        assert_eq!(issue.actions, vec![IssueAction::ChooseMicrophone]);
    }

    #[test]
    fn capture_problems_warn_with_the_device_name() {
        let device = "USB Mic".to_owned();
        let cases = [
            (
                CaptureProblem::Disconnected {
                    device: device.clone(),
                },
                "Microphone disconnected",
            ),
            (
                CaptureProblem::NoAudio {
                    device: device.clone(),
                },
                "No audio from USB Mic",
            ),
            (
                CaptureProblem::Silent {
                    device: device.clone(),
                },
                "USB Mic sends only silence",
            ),
            (
                CaptureProblem::Busy {
                    device: device.clone(),
                },
                "Microphone in use by another app",
            ),
        ];
        for (problem, title) in cases {
            let facts = HealthFacts {
                capture: Some(problem.clone()),
                ..running()
            };
            let health = evaluate(&facts, Platform::MacOs);
            assert!(health.blocking().is_none(), "{problem:?}");
            assert_eq!(health.issues.len(), 1, "{problem:?}");
            assert_eq!(health.issues[0].title, title);
            assert_eq!(health.issues[0].severity, Severity::Warning);
            assert!(health.issues[0]
                .actions
                .contains(&IssueAction::ChooseMicrophone));
        }
    }

    #[test]
    fn permissions_block_or_warn_on_macos_only() {
        let denied = HealthFacts {
            permissions: Some(Permissions {
                microphone: PermissionStatus::Denied,
                accessibility: PermissionStatus::Denied,
                ..granted()
            }),
            ..running()
        };
        let health = evaluate(&denied, Platform::MacOs);
        assert_eq!(
            issue_kinds(&denied, Platform::MacOs),
            vec![IssueKind::MicrophoneDenied, IssueKind::AccessibilityDenied]
        );
        assert_eq!(health.issues[0].title, "Microphone access is off");
        assert_eq!(
            health.issues[0].actions,
            vec![IssueAction::OpenMicrophonePrivacy]
        );
        assert_eq!(
            health.issues[1].actions,
            vec![IssueAction::OpenAccessibility]
        );

        // Linux has no microphone permission; "accessibility" there is a paste tool.
        let linux = evaluate(&denied, Platform::Linux);
        assert_eq!(linux.issues.len(), 1);
        assert_eq!(linux.issues[0].kind, IssueKind::AccessibilityDenied);
        assert!(linux.issues[0].message.contains("ydotool"));
        assert_eq!(linux.issues[0].actions, vec![IssueAction::OpenHelp]);

        let not_asked = HealthFacts {
            permissions: Some(Permissions {
                microphone: PermissionStatus::NotDetermined,
                ..granted()
            }),
            ..running()
        };
        let health = evaluate(&not_asked, Platform::MacOs);
        assert_eq!(health.issues[0].kind, IssueKind::MicrophoneNotAsked);
        assert_eq!(
            health.issues[0].actions,
            vec![IssueAction::RequestMicrophone]
        );
        assert!(health.blocking().is_none());
    }

    #[test]
    fn a_refused_capture_and_a_denied_permission_are_one_issue() {
        let facts = HealthFacts {
            capture: Some(CaptureProblem::PermissionDenied),
            permissions: Some(Permissions {
                microphone: PermissionStatus::Denied,
                ..granted()
            }),
            ..running()
        };
        assert_eq!(
            issue_kinds(&facts, Platform::MacOs),
            vec![IssueKind::MicrophoneDenied]
        );
    }

    #[test]
    fn a_down_engine_blocks_local_dictation_but_not_cloud_dictation() {
        let down = HealthFacts {
            engine: EngineStatus::down("The speech engine stopped (signal 9)".to_owned()),
            model: ModelFact::Unknown,
            ..running()
        };
        let health = evaluate(&down, Platform::MacOs);
        let issue = health.blocking().cloned();
        assert_eq!(
            issue.as_ref().map(|issue| issue.kind.clone()),
            Some(IssueKind::EngineDown {
                reason: "The speech engine stopped (signal 9)".to_owned()
            })
        );
        assert_eq!(
            issue.map(|issue| issue.actions),
            Some(vec![IssueAction::RestartEngine])
        );

        let cloud = HealthFacts {
            provider: ProviderFact::Cloud {
                provider: "Groq".to_owned(),
                has_key: true,
                fault: None,
            },
            ..down
        };
        let health = evaluate(&cloud, Platform::MacOs);
        assert!(health.blocking().is_none());
        assert_eq!(health.issues[0].severity, Severity::Warning);
    }

    #[test]
    fn a_stalled_engine_offers_a_restart() {
        let facts = HealthFacts {
            engine_stalled: Some(
                "The speech engine did not answer transcribe within 2 min".to_owned(),
            ),
            ..running()
        };
        let health = evaluate(&facts, Platform::MacOs);
        assert_eq!(health.issues[0].title, "Speech engine not responding");
        assert_eq!(health.issues[0].actions, vec![IssueAction::RestartEngine]);
        assert!(health.blocking().is_none());
    }

    #[test]
    fn model_states_block_until_the_model_is_ready() {
        let model = "whisper-turbo".to_owned();
        let name = "Whisper Turbo".to_owned();
        let missing = HealthFacts {
            model: ModelFact::Missing {
                model: model.clone(),
                name: name.clone(),
            },
            ..running()
        };
        let health = evaluate(&missing, Platform::MacOs);
        let issue = &health.issues[0];
        assert_eq!(issue.severity, Severity::Blocking);
        assert_eq!(issue.title, "Whisper Turbo isn't downloaded");
        assert_eq!(
            issue.actions,
            vec![IssueAction::DownloadModel, IssueAction::OpenModelSettings]
        );

        let downloading = HealthFacts {
            model: ModelFact::Downloading {
                model: model.clone(),
                name: name.clone(),
                percent: 42,
            },
            ..running()
        };
        let health = evaluate(&downloading, Platform::MacOs);
        assert_eq!(health.issues[0].severity, Severity::Blocking);
        assert!(health.issues[0].message.contains("42%"));

        let failed = HealthFacts {
            model: ModelFact::Failed {
                model: model.clone(),
                name: name.clone(),
                reason: "whisper-turbo did not load; delete it and download it again".to_owned(),
            },
            ..running()
        };
        let health = evaluate(&failed, Platform::MacOs);
        assert_eq!(health.issues[0].severity, Severity::Warning);
        assert_eq!(health.issues[0].title, "Whisper Turbo failed");

        // With a working cloud key, a missing local model only matters as the fallback.
        let cloud = HealthFacts {
            provider: ProviderFact::Cloud {
                provider: "Groq".to_owned(),
                has_key: true,
                fault: None,
            },
            ..missing
        };
        assert!(evaluate(&cloud, Platform::MacOs).blocking().is_none());
    }

    #[test]
    fn cloud_faults_name_the_fix() {
        let with = |has_key: bool, fault: Option<ProviderFault>| HealthFacts {
            provider: ProviderFact::Cloud {
                provider: "Groq".to_owned(),
                has_key,
                fault,
            },
            ..running()
        };
        let first = |facts: HealthFacts| evaluate(&facts, Platform::MacOs).issues.remove(0);
        let missing = first(with(false, None));
        assert_eq!(missing.title, "Groq API key missing");
        assert_eq!(missing.actions, vec![IssueAction::OpenProviderSettings]);
        assert_eq!(
            first(with(true, Some(ProviderFault::Unauthorized))).title,
            "Groq rejected the API key"
        );
        assert_eq!(
            first(with(true, Some(ProviderFault::RateLimited))).title,
            "Groq rate limit reached"
        );
        assert_eq!(
            first(with(true, Some(ProviderFault::Offline))).title,
            "Can't reach Groq"
        );
        assert_eq!(
            first(with(true, Some(ProviderFault::TimedOut))).title,
            "Can't reach Groq"
        );
        assert!(evaluate(&with(true, None), Platform::MacOs)
            .issues
            .is_empty());
    }

    #[test]
    fn shortcut_and_wayland_issues() {
        let facts = HealthFacts {
            shortcut_taken: Some("Alt+Space".to_owned()),
            wayland_session: true,
            ..running()
        };
        let linux = evaluate(&facts, Platform::Linux);
        assert_eq!(
            linux
                .issues
                .iter()
                .map(|issue| issue.kind.clone())
                .collect::<Vec<_>>(),
            vec![
                IssueKind::ShortcutUnavailable {
                    shortcut: "Alt+Space".to_owned()
                },
                IssueKind::WaylandShortcuts
            ]
        );
        assert_eq!(linux.issues[1].severity, Severity::Info);
        assert!(linux.issues[1].message.contains("simple-voice --toggle"));
        // macOS has no Wayland session; the flag means nothing there.
        assert_eq!(evaluate(&facts, Platform::MacOs).issues.len(), 1);
    }

    #[test]
    fn issues_are_ordered_blocking_first() {
        let facts = HealthFacts {
            input: InputState::NoDevice,
            shortcut_taken: Some("Alt+Space".to_owned()),
            permissions: Some(Permissions {
                accessibility: PermissionStatus::Denied,
                ..granted()
            }),
            ..running()
        };
        let severities: Vec<Severity> = evaluate(&facts, Platform::MacOs)
            .issues
            .iter()
            .map(|issue| issue.severity)
            .collect();
        assert_eq!(
            severities,
            vec![Severity::Blocking, Severity::Warning, Severity::Warning]
        );
    }

    #[test]
    fn every_issue_has_a_title_a_message_and_an_action() {
        let device = || "Mic".to_owned();
        let model = || "m".to_owned();
        let kinds = [
            IssueKind::NoMicrophone,
            IssueKind::NoSoundServer,
            IssueKind::SelectedMicrophoneMissing {
                requested: device(),
                device: device(),
            },
            IssueKind::MicrophoneDisconnected { device: device() },
            IssueKind::MicrophoneNoAudio { device: device() },
            IssueKind::MicrophoneSilent { device: device() },
            IssueKind::MicrophoneBusy { device: device() },
            IssueKind::MicrophoneFailed { reason: device() },
            IssueKind::MicrophoneDenied,
            IssueKind::MicrophoneNotAsked,
            IssueKind::AccessibilityDenied,
            IssueKind::EngineDown { reason: device() },
            IssueKind::EngineNotResponding { reason: device() },
            IssueKind::ModelMissing {
                model: model(),
                name: model(),
            },
            IssueKind::ModelDownloading {
                model: model(),
                name: model(),
                percent: 1,
            },
            IssueKind::ModelUnavailable {
                model: model(),
                name: model(),
                reason: model(),
            },
            IssueKind::ModelFailed {
                model: model(),
                name: model(),
                reason: model(),
            },
            IssueKind::ProviderKeyMissing { provider: model() },
            IssueKind::ProviderKeyInvalid { provider: model() },
            IssueKind::ProviderRateLimited { provider: model() },
            IssueKind::ProviderOffline { provider: model() },
            IssueKind::ShortcutUnavailable { shortcut: model() },
            IssueKind::WaylandShortcuts,
        ];
        for platform in [Platform::MacOs, Platform::Linux] {
            for kind in &kinds {
                let issue = text::issue(kind.clone(), false, platform);
                assert!(!issue.title.is_empty(), "{kind:?}");
                assert!(issue.message.ends_with('.'), "{kind:?}: {}", issue.message);
                assert!(!issue.actions.is_empty(), "{kind:?}");
            }
        }
    }

    #[test]
    fn issues_serialize_flat_for_the_ui() {
        let issue = CaptureProblem::Disconnected {
            device: "USB Mic".to_owned(),
        }
        .issue(Platform::MacOs);
        let json = serde_json::to_value(&issue).unwrap_or_default();
        assert_eq!(json["kind"], "microphone_disconnected");
        assert_eq!(json["device"], "USB Mic");
        assert_eq!(json["severity"], "warning");
        assert_eq!(
            json["actions"],
            serde_json::json!(["choose_microphone", "open_sound_settings"])
        );
        let back: Issue = serde_json::from_value(json).unwrap_or_else(|_| issue.clone());
        assert_eq!(back, issue);
    }

    #[test]
    fn a_capture_problem_becomes_the_same_error_text() {
        let error = AppError::from(CaptureProblem::NoDevice);
        let issue = CaptureProblem::NoDevice.issue(Platform::current());
        assert_eq!(error.to_string(), issue.sentence());
        assert!(error.to_string().starts_with("No microphone connected. "));
    }
}
