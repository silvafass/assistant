use std::str::FromStr;

use anyhow::{Ok, anyhow};
use futures::StreamExt;
use reqwest::Url;
use serde_json::{Value, json};

use crate::{
    agent::AgentBuilder,
    client::{
        self, ChatMessage,
        ContentEvent::{StartReasoning, StopReasoning},
    },
    providers::openai,
};

pub const DEFAULT_API_BASE_URL: &str = "http://localhost:11434";

#[derive(Debug)]
pub struct Client {
    api_base_url: Url,
    inner: reqwest::Client,
}

impl client::Client for Client {
    async fn prompt_stream(
        &self,
        payload: client::PromptPayload,
    ) -> anyhow::Result<impl futures::prelude::Stream<Item = client::StreamedReponseContent>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let mut response = self
            .inner
            .post(self.api_base_url.join("/v1/responses")?)
            .json(&json!({
                "model": &payload.model,
                "input": &payload.prompt,
                "stream": true,
            }))
            .send()
            .await?;

        if response.error_for_status_ref().is_err() {
            let error_response: serde_json::Value = response.json().await?;
            Err(anyhow!("API error: {error_response}"))
        } else {
            tokio::spawn(async move {
                let mut is_reasoning = false;
                while let Some(chunk) = response.chunk().await.unwrap() {
                    const REASONING_SUMMARY_TEXT_EVENT: &[u8; 51] =
                        b"event: response.reasoning_summary_text.delta\ndata: ";
                    const OUTPUT_TEXT_EVENT: &[u8; 40] =
                        b"event: response.output_text.delta\ndata: ";

                    if chunk.starts_with(REASONING_SUMMARY_TEXT_EVENT) {
                        if !is_reasoning {
                            is_reasoning = true;
                            let event =
                                client::StreamedReponseContent::ContentEvent(StartReasoning);
                            tx.send(event).unwrap();
                        }
                        let chunk = &chunk[REASONING_SUMMARY_TEXT_EVENT.len()..];
                        let chunk: Value = serde_json::from_slice(chunk).unwrap();
                        let chunk_content = client::StreamedReponseContent::ChunkReasoning {
                            content: chunk["delta"].as_str().unwrap().to_string(),
                        };
                        tx.send(chunk_content).unwrap();
                    } else if chunk.starts_with(OUTPUT_TEXT_EVENT) {
                        if is_reasoning {
                            is_reasoning = false;
                            let event = client::StreamedReponseContent::ContentEvent(StopReasoning);
                            tx.send(event).unwrap();
                        }
                        let chunk = &chunk[OUTPUT_TEXT_EVENT.len()..];
                        let chunk: Value = serde_json::from_slice(chunk).unwrap();
                        let chunk_content = client::StreamedReponseContent::ChunkText {
                            content: chunk["delta"].as_str().unwrap().to_string(),
                        };
                        tx.send(chunk_content).unwrap();
                    };
                }
            });

            let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

            Ok(stream)
        }
    }

    async fn prompt(
        &self,
        payload: client::PromptPayload,
    ) -> anyhow::Result<client::ResponseContent> {
        let response = self
            .inner
            .post(self.api_base_url.join("/v1/responses")?)
            .json(&json!({
                "model": &payload.model,
                "input": &payload.prompt,
                "stream": true,
            }))
            .send()
            .await?;

        if response.error_for_status_ref().is_err() {
            let error_response: serde_json::Value = response.json().await?;
            Err(anyhow!("API error: {error_response}"))
        } else {
            let response = response.json::<Value>().await?;
            let (reasoning, text) = response["output"].as_array().unwrap().iter().fold(
                (String::new(), String::new()),
                |(reasoning, text), output| {
                    if output["type"] == "reasoning" {
                        (
                            reasoning + output["summary"][0]["text"].as_str().unwrap(),
                            text,
                        )
                    } else if output["type"] == "message" {
                        (
                            reasoning,
                            text + output["content"][0]["text"].as_str().unwrap(),
                        )
                    } else {
                        (reasoning, text)
                    }
                },
            );
            Ok(client::ResponseContent { reasoning, text })
        }
    }

    async fn chat_stream(
        &self,
        model: &str,
        prompt: &str,
        messages: &mut Vec<client::ChatMessage>,
    ) -> anyhow::Result<impl futures::prelude::Stream<Item = client::StreamedReponseContent>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let mut inner_messages: Vec<Value> = messages
            .iter_mut()
            .map(|message| {
                json!({
                    "role": &message.role,
                    "content": &message.content
                })
            })
            .collect();

        inner_messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        let mut response = self
            .inner
            .post(self.api_base_url.join("/v1/chat/completions")?)
            .json(&json!({
                "model": model,
                "messages": inner_messages,
                "stream": true,
            }))
            .send()
            .await?;

        if response.error_for_status_ref().is_err() {
            let error_response: serde_json::Value = response.json().await?;
            Err(anyhow!("API error: {error_response}"))
        } else {
            tokio::spawn(async move {
                let mut message_rule: Option<String> = None;
                let mut message_content = String::new();
                let mut is_reasoning = false;
                while let Some(chunk) = response.chunk().await.unwrap() {
                    let chunk = if &chunk[..] == b"data: [DONE]\n\n" {
                        continue;
                    } else if chunk.starts_with(b"data: ") && chunk.ends_with(b"data: [DONE]\n\n") {
                        &chunk[..chunk.len() - b"data: [DONE]\n\n".len()][b"data: ".len()..]
                    } else if chunk.starts_with(b"data: ") {
                        &chunk[b"data: ".len()..]
                    } else {
                        continue;
                    };

                    let chunk: Value = serde_json::from_slice(chunk).unwrap();

                    if let (Some(role), Some(thinking)) = (
                        chunk["choices"][0]["delta"]["role"].as_str(),
                        chunk["choices"][0]["delta"]["reasoning"].as_str(),
                    ) && !thinking.is_empty()
                    {
                        if !is_reasoning {
                            is_reasoning = true;
                            let event =
                                client::StreamedReponseContent::ContentEvent(StartReasoning);
                            tx.send(event).unwrap();
                        }
                        let chunk = client::StreamedReponseContent::ChunkMessageReasoning {
                            role: role.to_string(),
                            content: thinking.to_string(),
                        };

                        tx.send(chunk).unwrap();
                    } else if let (Some(role), Some(response)) = (
                        chunk["choices"][0]["delta"]["role"].as_str(),
                        chunk["choices"][0]["delta"]["content"].as_str(),
                    ) && !response.is_empty()
                    {
                        if is_reasoning {
                            is_reasoning = false;
                            let event = client::StreamedReponseContent::ContentEvent(StopReasoning);
                            tx.send(event).unwrap();
                        }
                        if message_rule.is_none() {
                            message_rule = Some(role.to_string());
                        }
                        message_content.push_str(response);

                        let chunk = client::StreamedReponseContent::ChunkMessageText {
                            role: role.to_string(),
                            content: response.to_string(),
                        };
                        tx.send(chunk).unwrap();
                    };
                }

                let message_text = client::StreamedReponseContent::MessageText {
                    role: message_rule.unwrap(),
                    content: message_content,
                };
                tx.send(message_text).unwrap();
            });

            let stream =
                tokio_stream::wrappers::UnboundedReceiverStream::new(rx).inspect(|message| {
                    if let client::StreamedReponseContent::MessageText { role, content } = message {
                        messages.push(ChatMessage {
                            role: role.clone(),
                            content: content.clone(),
                        });
                    }
                });

            Ok(stream)
        }
    }

    async fn chat(
        &self,
        model: &str,
        prompt: &str,
        messages: &mut Vec<ChatMessage>,
    ) -> anyhow::Result<client::ResponseContent> {
        let mut inner_messages: Vec<Value> = messages
            .iter_mut()
            .map(|message| {
                json!({
                    "role": &message.role,
                    "content": &message.content
                })
            })
            .collect();

        inner_messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        let response = self
            .inner
            .post(self.api_base_url.join("/v1/chat/completions")?)
            .json(&json!({
                "model": model,
                "messages": inner_messages,
                "stream": false,
            }))
            .send()
            .await?;

        if response.error_for_status_ref().is_err() {
            let error_response: serde_json::Value = response.json().await?;
            Err(anyhow!("API error: {error_response}"))
        } else {
            let response = response.json::<Value>().await?;
            Ok(client::ResponseContent {
                reasoning: response["choices"][0]["message"]["reasoning"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                text: response["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            })
        }
    }
}

impl From<&str> for Client {
    fn from(api_base_url: &str) -> Self {
        Client {
            api_base_url: Url::from_str(api_base_url).unwrap(),
            inner: reqwest::Client::new(),
        }
    }
}

impl From<String> for Client {
    fn from(api_base_url: String) -> Self {
        Self::from(api_base_url.as_str())
    }
}

impl Default for Client {
    fn default() -> Self {
        Client {
            api_base_url: Url::from_str(openai::DEFAULT_API_BASE_URL).unwrap(),
            inner: reqwest::Client::new(),
        }
    }
}

impl Client {
    pub fn agent_builder(self, model: &str) -> AgentBuilder<openai::Client> {
        AgentBuilder::from_client(self).model(model)
    }
}
