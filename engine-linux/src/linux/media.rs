// FilePath: engine-linux/src/linux/media.rs
//! Quiet while recording on Linux (the `media_control` boundary): players are paused and
//! resumed over MPRIS through playerctl, and system output is muted through PulseAudio's
//! `pactl`, which PipeWire answers too. Only players that were playing are paused, and only those
//! still paused are resumed, so nothing the user changed meanwhile is toggled.

use serde_json::{json, Value};

use super::tools;

const PLAYING: &str = "Playing";
const PAUSED: &str = "Paused";
/// Our own cue sounds play while a recording starts; they are not media to pause.
const OWN_BINARIES: &[&str] = &["simple-voice", "simple-voice-engine"];

/// Pauses what is playing. `paused` lists the players to resume later; `unpausable` means audio
/// plays that MPRIS cannot pause (the caller mutes instead); `nothing_playing` needs nothing.
pub(crate) async fn pause_media() -> Result<Value, String> {
    let playing = players_in(PLAYING).await;
    let mut paused = Vec::new();
    for player in &playing {
        let stopped = tools::run_ok("playerctl", &["-p", player, "pause"])
            .await
            .is_ok()
            && status(player).await.as_deref() != Some(PLAYING);
        if stopped {
            paused.push(player.clone());
        }
    }
    let (outcome, players) = if !paused.is_empty() {
        ("paused", paused)
    } else if !playing.is_empty() {
        ("unpausable", playing)
    } else if other_audio_playing().await {
        ("unpausable", Vec::new())
    } else {
        ("nothing_playing", Vec::new())
    };
    Ok(json!({ "outcome": outcome, "players": players }))
}

/// Resumes `players` that are still paused.
pub(crate) async fn resume_media(players: &[String]) -> Result<Value, String> {
    let mut resumed = false;
    for player in players {
        if status(player).await.as_deref() == Some(PAUSED) {
            resumed |= tools::run_ok("playerctl", &["-p", player, "play"])
                .await
                .is_ok();
        }
    }
    Ok(json!({ "resumed": resumed }))
}

/// Mutes or unmutes the default output; the result says whether it was muted before.
pub(crate) async fn set_output_muted(muted: bool) -> Result<Value, String> {
    let before = tools::run_ok("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]).await?;
    let previous = parse_mute(&before)
        .ok_or_else(|| format!("pactl reported an unknown mute state: {}", before.trim()))?;
    let value = if muted { "1" } else { "0" };
    tools::run_ok("pactl", &["set-sink-mute", "@DEFAULT_SINK@", value]).await?;
    Ok(json!({ "previous": previous }))
}

async fn players_in(wanted: &str) -> Vec<String> {
    if !tools::installed("playerctl") {
        return Vec::new();
    }
    let Ok(list) = tools::run_ok("playerctl", &["-l"]).await else {
        return Vec::new();
    };
    let mut matching = Vec::new();
    for player in list.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if status(player).await.as_deref() == Some(wanted) {
            matching.push(player.to_owned());
        }
    }
    matching
}

async fn status(player: &str) -> Option<String> {
    tools::run_ok("playerctl", &["-p", player, "status"])
        .await
        .ok()
        .map(|status| status.trim().to_owned())
}

/// Whether any app other than Simple Voice has an uncorked output stream.
async fn other_audio_playing() -> bool {
    match tools::run_ok("pactl", &["list", "sink-inputs"]).await {
        Ok(listing) => streams_playing(&listing),
        Err(_) => false,
    }
}

fn streams_playing(listing: &str) -> bool {
    listing.split("Sink Input #").skip(1).any(|block| {
        let running = block.lines().any(|line| line.trim() == "Corked: no");
        let ours = block.lines().any(|line| {
            line.trim()
                .strip_prefix("application.process.binary = ")
                .is_some_and(|binary| OWN_BINARIES.contains(&binary.trim_matches('"')))
        });
        running && !ours
    })
}

fn parse_mute(output: &str) -> Option<bool> {
    match output.trim().strip_prefix("Mute:")?.trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pactl_mute_state() {
        assert_eq!(parse_mute("Mute: yes\n"), Some(true));
        assert_eq!(parse_mute("Mute: no"), Some(false));
        assert_eq!(parse_mute("Stummschaltung: ja"), None);
    }

    #[test]
    fn counts_only_other_apps_playing() {
        let ours = "Sink Input #12\n\tCorked: no\n\tProperties:\n\t\tapplication.process.binary = \"simple-voice\"\n";
        let corked =
            "Sink Input #13\n\tCorked: yes\n\t\tapplication.process.binary = \"firefox\"\n";
        let playing = "Sink Input #14\n\tCorked: no\n\t\tapplication.process.binary = \"vlc\"\n";
        assert!(!streams_playing(ours));
        assert!(!streams_playing(&format!("{ours}{corked}")));
        assert!(streams_playing(&format!("{ours}{corked}{playing}")));
        assert!(!streams_playing(""));
    }
}
