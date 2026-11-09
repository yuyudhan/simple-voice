// FilePath: engine-linux/src/linux/input_watch.rs
//! `input_devices_changed` on Linux, the counterpart of the Swift helper's Core Audio watch.
//! PulseAudio (and PipeWire's pulse server) announces every source that comes or goes and every
//! change of the default source on `pactl subscribe`. Once a burst of those settles, the helper
//! looks at the microphone the way the app does (`sv_audio::check_input`: a speaker monitor is
//! not a microphone) and sends the event only when the answer changed: a microphone plugged in,
//! the last one pulled out, only the speaker monitor left, the sound server gone. A volume change
//! or a recording opening the source changes nothing, so the helper stays quiet then. The app
//! checks again on the event instead of at its next poll, so its banner follows at once.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use sv_domain::InputState;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::output::Output;
use super::{log, tools};

const EVENT: &str = "input_devices_changed";
/// One plug arrives as several events (the card, its source, then the server's new default);
/// events this close together are one change, as in the Swift helper.
const SETTLE: Duration = Duration::from_millis(300);
/// The wait before subscribing again after the sound server went away (restarted or stopped).
const RESUBSCRIBE_AFTER: Duration = Duration::from_secs(10);

/// Looks for the microphone; blocking, so it runs off the async threads.
type Check = Arc<dyn Fn() -> InputState + Send + Sync>;

/// Watches the sound server for the life of the process. Without `pactl` there is nothing to
/// subscribe to; the app's own poll still notices changes a few seconds later.
pub(crate) fn spawn(output: Arc<Output>) {
    if !tools::installed("pactl") {
        log::info("pactl not found; microphone changes are noticed by the app's poll");
        return;
    }
    tokio::spawn(async move {
        let check: Check = Arc::new(|| sv_audio::check_input(None));
        let notify = move || output.event(EVENT);
        let mut watch = Watch {
            last: look(&check).await,
        };
        loop {
            match subscribe() {
                Ok(mut child) => {
                    if let Some(stdout) = child.stdout.take() {
                        follow(BufReader::new(stdout), SETTLE, &mut watch, &check, &notify).await;
                    }
                    if let Err(error) = child.wait().await {
                        log::error(&format!("waiting for pactl subscribe: {error}"));
                    }
                }
                Err(error) => {
                    log::error(&format!("cannot run pactl subscribe: {error}"));
                    return;
                }
            }
            tokio::time::sleep(RESUBSCRIBE_AFTER).await;
        }
    });
}

fn subscribe() -> std::io::Result<tokio::process::Child> {
    Command::new("pactl")
        .arg("subscribe")
        // The event lines are translated otherwise.
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
}

/// The last answer the app was told about.
#[derive(Debug)]
struct Watch {
    last: InputState,
}

impl Watch {
    /// Notifies when `now` differs from the last answer. A check that could not tell (`Unknown`)
    /// claims nothing and keeps the last answer.
    fn report(&mut self, now: InputState, notify: &(dyn Fn() + Sync)) {
        if now == InputState::Unknown || now == self.last {
            return;
        }
        self.last = now;
        notify();
    }
}

/// Reads `pactl subscribe` lines until the subscription ends, checking the microphone each time
/// a burst of source, server or card events has been quiet for `settle`, and once more at the
/// end (the server going away is itself a change).
async fn follow<R: AsyncBufRead + Unpin>(
    reader: R,
    settle: Duration,
    watch: &mut Watch,
    check: &Check,
    notify: &(dyn Fn() + Sync),
) {
    let mut lines = reader.lines();
    let mut pending = false;
    loop {
        let next = if pending {
            match tokio::time::timeout(settle, lines.next_line()).await {
                Ok(next) => next,
                Err(_settled) => {
                    pending = false;
                    watch.report(look(check).await, notify);
                    continue;
                }
            }
        } else {
            lines.next_line().await
        };
        match next {
            Ok(Some(line)) => pending |= is_input_event(&line),
            Ok(None) => break,
            Err(error) => {
                log::error(&format!("reading pactl subscribe: {error}"));
                break;
            }
        }
    }
    watch.report(look(check).await, notify);
}

async fn look(check: &Check) -> InputState {
    let check = Arc::clone(check);
    tokio::task::spawn_blocking(move || check())
        .await
        .unwrap_or_default()
}

/// Whether a `pactl subscribe` line (`Event 'new' on source #52`) can change the microphone: a
/// source came, went or changed, the server's default source changed, or a card was plugged.
/// Recording streams (`source-output`) and the speakers' side are not inputs.
fn is_input_event(line: &str) -> bool {
    let Some((_, object)) = line.split_once(" on ") else {
        return false;
    };
    matches!(
        object.split_whitespace().next(),
        Some("source" | "server" | "card")
    )
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use sv_audio::{input_state, InputProbe};
    use tokio::io::AsyncWriteExt;

    use super::*;

    const HEADSET: &str = "Jabra EVOLVE 20 Mono";

    /// What `check_input` answers for a sound server offering `microphones` (speaker monitors
    /// are already left out of an `InputProbe`, so `&[]` is a PC with only the monitor).
    fn state(microphones: &[&str]) -> InputState {
        let probe = InputProbe {
            sound_server: true,
            devices: microphones.iter().map(|name| (*name).to_owned()).collect(),
            default: microphones.first().map(|name| (*name).to_owned()),
        };
        input_state(&probe, None)
    }

    /// A fake sound server: each check answers the next state, and counts.
    fn checks(states: Vec<InputState>) -> (Check, Arc<AtomicUsize>) {
        let states = Mutex::new(VecDeque::from(states));
        let count = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&count);
        let check: Check = Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            super::super::lock(&states)
                .pop_front()
                .unwrap_or(InputState::Unknown)
        });
        (check, count)
    }

    #[test]
    fn only_source_server_and_card_events_count() {
        for line in [
            "Event 'new' on source #52",
            "Event 'remove' on source #52",
            "Event 'change' on server #-1",
            "Event 'new' on card #3",
        ] {
            assert!(is_input_event(line), "{line}");
        }
        for line in [
            "Event 'new' on source-output #7",
            "Event 'change' on sink #47",
            "Event 'remove' on sink-input #9",
            "Event 'new' on client #12",
            "",
        ] {
            assert!(!is_input_event(line), "{line}");
        }
    }

    #[tokio::test]
    async fn plugging_a_microphone_into_a_monitor_only_pc_is_reported_once() {
        let (check, count) = checks(vec![state(&[HEADSET])]);
        let notified = AtomicUsize::new(0);
        let notify = || {
            notified.fetch_add(1, Ordering::SeqCst);
        };
        let mut watch = Watch { last: state(&[]) };
        assert_eq!(watch.last, InputState::NoDevice);
        let burst = b"Event 'new' on card #3\n\
            Event 'new' on source #52\n\
            Event 'new' on source-output #7\n\
            Event 'change' on server #-1\n";
        follow(&burst[..], SETTLE, &mut watch, &check, &notify).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "one look for the whole burst"
        );
        assert_eq!(notified.load(Ordering::SeqCst), 1);
        assert_eq!(
            watch.last,
            InputState::Ready {
                device: HEADSET.to_owned()
            }
        );
    }

    #[tokio::test]
    async fn a_microphone_pulled_out_or_a_server_gone_is_reported_and_volume_is_not() {
        let notified = AtomicUsize::new(0);
        let notify = || {
            notified.fetch_add(1, Ordering::SeqCst);
        };
        let mut watch = Watch {
            last: state(&[HEADSET]),
        };
        // A volume change on the headset: same answer, nothing to say.
        let (check, _) = checks(vec![state(&[HEADSET])]);
        follow(
            &b"Event 'change' on source #52\n"[..],
            SETTLE,
            &mut watch,
            &check,
            &notify,
        )
        .await;
        assert_eq!(notified.load(Ordering::SeqCst), 0);
        // Unplugged: only the speaker monitor is left.
        let (check, _) = checks(vec![state(&[])]);
        follow(
            &b"Event 'remove' on source #52\n"[..],
            SETTLE,
            &mut watch,
            &check,
            &notify,
        )
        .await;
        assert_eq!(notified.load(Ordering::SeqCst), 1);
        assert_eq!(watch.last, InputState::NoDevice);
        // A check that cannot tell claims nothing.
        let (check, _) = checks(vec![InputState::Unknown]);
        follow(
            &b"Event 'change' on server #-1\n"[..],
            SETTLE,
            &mut watch,
            &check,
            &notify,
        )
        .await;
        assert_eq!(notified.load(Ordering::SeqCst), 1);
        assert_eq!(watch.last, InputState::NoDevice);
        // The subscription ends because the sound server stopped.
        let (check, _) = checks(vec![InputState::NoSoundServer]);
        follow(&b""[..], SETTLE, &mut watch, &check, &notify).await;
        assert_eq!(notified.load(Ordering::SeqCst), 2);
        assert_eq!(watch.last, InputState::NoSoundServer);
    }

    #[tokio::test]
    async fn a_change_is_reported_while_the_subscription_runs() {
        let (check, count) = checks(vec![state(&[])]);
        let (notified, mut told) = tokio::sync::mpsc::unbounded_channel();
        let notify = move || {
            notified.send(()).ok();
        };
        let (mut server, reader) = tokio::io::duplex(256);
        let watcher = tokio::spawn(async move {
            let mut watch = Watch {
                last: state(&[HEADSET]),
            };
            follow(
                BufReader::new(reader),
                Duration::from_millis(20),
                &mut watch,
                &check,
                &notify,
            )
            .await;
        });
        server
            .write_all(b"Event 'remove' on source #52\n")
            .await
            .expect("write the event");
        let told = tokio::time::timeout(Duration::from_secs(10), told.recv()).await;
        assert_eq!(
            told.ok().flatten(),
            Some(()),
            "told before the subscription ends"
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(server);
        watcher
            .await
            .expect("the watcher ends with the subscription");
    }
}
