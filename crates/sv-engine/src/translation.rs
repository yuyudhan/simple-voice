// FilePath: crates/sv-engine/src/translation.rs
//! Local translation on the helper's dedicated translation model (`madlad400-3b-mt`).

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use sv_domain::{AppError, AppResult};

use crate::client::{EngineClient, ProgressCallback};
use crate::protocol::{params, text};

/// A meeting's worth of lines on a 3B model, including a cold model load.
const TRANSLATE: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Deserialize)]
struct Translated {
    texts: Vec<String>,
}

impl EngineClient {
    /// Translates each text into `target_language` (the model's language tag, normally an ISO
    /// 639-1 code such as `hi`). Returns one translation per text in the same order; empty texts
    /// stay empty. Progress is the share of texts done; dropping the future cancels it.
    pub async fn translate(
        &self,
        model: &str,
        target_language: &str,
        texts: &[String],
        on_progress: ProgressCallback,
    ) -> AppResult<Vec<String>> {
        let inputs = texts
            .iter()
            .map(|line| Value::from(line.as_str()))
            .collect();
        let params = params([
            ("model", text(model)),
            ("targetLanguage", text(target_language)),
            ("texts", Some(Value::Array(inputs))),
        ]);
        let result: Translated = self
            .request_cancellable("translate", params, Some(on_progress), TRANSLATE)
            .await?;
        if result.texts.len() != texts.len() {
            return Err(AppError::Engine(format!(
                "The translation model returned {} texts for {} inputs",
                result.texts.len(),
                texts.len()
            )));
        }
        Ok(result.texts)
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
            let id = request["id"].as_u64();
            self.seen.lock().push(request);
            let Some(id) = id else { return Ok(()) };
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
    async fn translate_sends_texts_in_order_and_reads_them_back() {
        let (client, transport) =
            scripted(r#"{"id":$ID,"ok":true,"result":{"texts":["नमस्ते","",""]}}"#);
        let texts = vec!["Hello".to_owned(), String::new(), "  ".to_owned()];
        let out = client
            .translate("madlad400-3b-mt", "hi", &texts, Box::new(|_, _| {}))
            .await
            .unwrap();
        assert_eq!(out, vec!["नमस्ते".to_owned(), String::new(), String::new()]);
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["cmd"], "translate");
        assert_eq!(request["targetLanguage"], "hi");
        assert_eq!(request["texts"], serde_json::json!(["Hello", "", "  "]));
    }

    #[tokio::test]
    async fn translate_rejects_a_reply_with_the_wrong_number_of_texts() {
        let (client, _transport) = scripted(r#"{"id":$ID,"ok":true,"result":{"texts":["uno"]}}"#);
        let texts = vec!["one".to_owned(), "two".to_owned()];
        let result = client
            .translate("madlad400-3b-mt", "es", &texts, Box::new(|_, _| {}))
            .await;
        assert!(matches!(result, Err(AppError::Engine(_))));
    }

    #[tokio::test]
    async fn translate_surfaces_unsupported_languages() {
        let (client, _transport) = scripted(
            r#"{"id":$ID,"ok":false,"error":"madlad400-3b-mt does not support the language `xx`"}"#,
        );
        let result = client
            .translate(
                "madlad400-3b-mt",
                "xx",
                &["hi".to_owned()],
                Box::new(|_, _| {}),
            )
            .await;
        assert!(matches!(result, Err(AppError::Engine(message)) if message.contains("`xx`")));
    }
}
