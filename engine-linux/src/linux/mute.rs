// FilePath: engine-linux/src/linux/mute.rs
//! Muting the speakers while recording on Linux (the app's `output_mute` boundary): the default
//! sink is muted through PulseAudio's `pactl`, which PipeWire answers too, so every app and tab is
//! silent at once without being paused. Unmuting names the sink that was muted, so a default
//! output that changed meanwhile (headphones plugged in) is left alone.

use serde_json::{json, Value};

use super::tools;

/// Mutes the default sink. `device` is its name; `wasMuted` says the user had muted it already,
/// in which case nothing changed.
pub(crate) async fn mute_output() -> Result<Value, String> {
    let sink = tools::run_ok("pactl", &["get-default-sink"]).await?;
    let sink = sink.trim();
    if sink.is_empty() {
        return Err("no default audio output".to_owned());
    }
    let state = tools::run_ok("pactl", &["get-sink-mute", sink]).await?;
    let was_muted = parse_mute(&state)
        .ok_or_else(|| format!("pactl reported an unknown mute state: {}", state.trim()))?;
    if !was_muted {
        tools::run_ok("pactl", &["set-sink-mute", sink, "1"]).await?;
    }
    Ok(json!({ "device": sink, "wasMuted": was_muted }))
}

/// Unmutes `sink`, or puts back `volume` (0.0–1.0) when that is how it was silenced. `unmuted`
/// is false when the sink is gone.
pub(crate) async fn unmute_output(sink: &str, volume: Option<f32>) -> Result<Value, String> {
    let sinks = tools::run_ok("pactl", &["list", "short", "sinks"]).await?;
    if !sink_names(&sinks).any(|name| name == sink) {
        return Ok(json!({ "unmuted": false }));
    }
    match volume {
        Some(volume) => {
            let percent = format!("{}%", (volume.clamp(0.0, 1.0) * 100.0).round());
            tools::run_ok("pactl", &["set-sink-volume", sink, &percent]).await?;
        }
        None => {
            tools::run_ok("pactl", &["set-sink-mute", sink, "0"]).await?;
        }
    }
    Ok(json!({ "unmuted": true }))
}

fn parse_mute(output: &str) -> Option<bool> {
    match output.trim().strip_prefix("Mute:")?.trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// Sink names from `pactl list short sinks` (tab-separated: index, name, driver, ...).
fn sink_names(listing: &str) -> impl Iterator<Item = &str> {
    listing.lines().filter_map(|line| line.split('\t').nth(1))
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
    fn reads_sink_names() {
        let listing = "47\talsa_output.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tRUNNING\n\
                       52\tbluez_output.AC_80_0A.1\tPipeWire\ts16le 2ch 48000Hz\tSUSPENDED\n";
        let names: Vec<&str> = sink_names(listing).collect();
        assert_eq!(
            names,
            [
                "alsa_output.pci-0000_00_1f.3.analog-stereo",
                "bluez_output.AC_80_0A.1"
            ]
        );
        assert_eq!(sink_names("").count(), 0);
    }
}
