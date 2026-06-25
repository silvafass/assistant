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
    providers::ollama,
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
            .post(self.api_base_url.join("/api/generate")?)
            .json(&json!({
                "model": &payload.model,
                "prompt": &payload.prompt,
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
                    let chunk: Value = serde_json::from_slice(&chunk).unwrap();

                    if let Some(thinking) = chunk["thinking"].as_str()
                        && !thinking.is_empty()
                    {
                        if !is_reasoning {
                            is_reasoning = true;
                            let event =
                                client::StreamedReponseContent::ContentEvent(StartReasoning);
                            tx.send(event).unwrap();
                        }
                        let chunk_content = client::StreamedReponseContent::ChunkReasoning {
                            content: thinking.to_string(),
                        };
                        tx.send(chunk_content).unwrap();
                    } else if let Some(response) = chunk["response"].as_str()
                        && !response.is_empty()
                    {
                        if is_reasoning {
                            is_reasoning = false;
                            let event = client::StreamedReponseContent::ContentEvent(StopReasoning);
                            tx.send(event).unwrap();
                        }
                        let chunk_content = client::StreamedReponseContent::ChunkText {
                            content: response.to_string(),
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
            .post(self.api_base_url.join("/api/generate")?)
            .json(&json!({
                "model": &payload.model,
                "prompt": &payload.prompt,
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
                reasoning: response["thinking"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                text: response["response"].as_str().unwrap().to_string(),
            })
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
            .post(self.api_base_url.join("/api/chat")?)
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
                    let chunk: Value = serde_json::from_slice(&chunk).unwrap();

                    if let (Some(role), Some(thinking)) = (
                        chunk["message"]["role"].as_str(),
                        chunk["message"]["thinking"].as_str(),
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
                        chunk["message"]["role"].as_str(),
                        chunk["message"]["content"].as_str(),
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
            .post(self.api_base_url.join("/api/chat")?)
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
                reasoning: response["message"]["thinking"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                text: response["message"]["content"].as_str().unwrap().to_string(),
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
            api_base_url: Url::from_str(ollama::DEFAULT_API_BASE_URL).unwrap(),
            inner: reqwest::Client::new(),
        }
    }
}

impl Client {
    pub fn agent_builder(self, model: &str) -> AgentBuilder<ollama::Client> {
        AgentBuilder::from_client(self).model(model)
    }
}
