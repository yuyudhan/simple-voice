// FilePath: crates/sv-engine/src/output.rs
//! Muting system output around a dictation (`mute_output`, `unmute_output`).

use serde::Deserialize;
use serde_json::Value;
use sv_domain::{AppResult, MutedOutput};

use crate::client::EngineClient;
use crate::protocol::{params, QUICK};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MuteReply {
    device: String,
    was_muted: bool,
    #[serde(default)]
    volume: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct Unmuted {
    unmuted: bool,
}

impl EngineClient {
    /// Mutes the default output device. `None` when it was muted already: nothing to undo.
    pub async fn mute_output(&self) -> AppResult<Option<MutedOutput>> {
        let reply: MuteReply = self.request("mute_output", Value::Null, QUICK).await?;
        Ok((!reply.was_muted).then_some(MutedOutput {
            device: reply.device,
            volume: reply.volume,
        }))
    }

    /// Unmutes the device an earlier [`Self::mute_output`] muted, whichever output is the default
    /// now. Returns false when that device is gone (unplugged, disconnected).
    pub async fn unmute_output(&self, muted: &MutedOutput) -> AppResult<bool> {
        let params = params([
            ("device", Some(Value::from(muted.device.as_str()))),
            ("volume", muted.volume.map(Value::from)),
        ]);
        let reply: Unmuted = self.request("unmute_output", params, QUICK).await?;
        Ok(reply.unmuted)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use super::*;
    use crate::client::Transport;

    /// Answers every request with a canned reply and records the request.
    struct Scripted {
        client: EngineClient,
        reply: String,
        seen: Mutex<Vec<Value>>,
    }

    impl Transport for Scripted {
        fn send_line(&self, line: String) -> AppResult<()> {
            let request: Value = serde_json::from_str(&line).unwrap();
            let id = request["id"].as_u64().unwrap();
            self.seen.lock().push(request);
            let client = self.client.clone();
            let reply = self.reply.replace("$ID", &id.to_string());
            tokio::spawn(async move { client.on_stdout_line(&reply) });
            Ok(())
        }
    }

    fn scripted(reply: &str) -> (EngineClient, Arc<Scripted>) {
        let client = EngineClient::new();
        let transport = Arc::new(Scripted {
            client: client.clone(),
            reply: reply.to_owned(),
            seen: Mutex::new(Vec::new()),
        });
        client.connect(transport.clone());
        (client, transport)
    }

    #[tokio::test]
    async fn mute_output_names_the_muted_device_or_nothing_when_already_muted() {
        let (client, transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"device":"BuiltInSpeakerDevice","wasMuted":false}}"#,
        );
        let muted = client.mute_output().await.unwrap();
        assert_eq!(
            muted,
            Some(MutedOutput {
                device: "BuiltInSpeakerDevice".to_owned(),
                volume: None,
            })
        );
        assert_eq!(transport.seen.lock()[0]["cmd"], "mute_output");

        let (client, _transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"device":"hdmi","wasMuted":false,"volume":0.37}}"#,
        );
        let muted = client.mute_output().await.unwrap().unwrap();
        assert_eq!(muted.volume, Some(0.37));

        let (client, _transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"device":"BuiltInSpeakerDevice","wasMuted":true}}"#,
        );
        assert_eq!(client.mute_output().await.unwrap(), None);
    }

    #[tokio::test]
    async fn unmute_output_sends_the_device_and_the_volume_to_put_back() {
        let (client, transport) = scripted(r#"{"id":$ID,"ok":true,"result":{"unmuted":true}}"#);
        let muted = MutedOutput {
            device: "hdmi".to_owned(),
            volume: Some(0.5),
        };
        assert!(client.unmute_output(&muted).await.unwrap());
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["cmd"], "unmute_output");
        assert_eq!(request["device"], "hdmi");
        assert_eq!(request["volume"], 0.5);

        let (client, transport) = scripted(r#"{"id":$ID,"ok":true,"result":{"unmuted":false}}"#);
        let muted = MutedOutput {
            device: "AirPods".to_owned(),
            volume: None,
        };
        assert!(!client.unmute_output(&muted).await.unwrap());
        assert!(transport.seen.lock()[0].get("volume").is_none());
    }
}
