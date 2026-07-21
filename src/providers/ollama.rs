use std::str::FromStr;

use anyhow::anyhow;
use futures::{
    StreamExt,
    stream::{self, BoxStream},
};
use reqwest::Url;
use serde_json::{Value, json};

use crate::{
    client::{
        self, ChatMessage, Chunk, Client,
        ContentEvent::{StartReasoning, StopReasoning},
        StreamedReponseContent,
    },
    providers::ollama,
};

pub const DEFAULT_API_BASE_URL: &str = "http://localhost:11434";

#[derive(Debug)]
pub struct OllamaClient {
    api_base_url: Url,
    inner: reqwest::Client,
}

impl Client for OllamaClient {
    async fn prompt_stream(
        &self,
        payload: client::PromptPayload,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamedReponseContent>>> {
        let response = self
            .inner
            .post(self.api_base_url.join("/api/generate")?)
            .json(&json!({
                "model": &payload.model,
                "prompt": &payload.prompt,
                "stream": true,
                "options": {
                    "num_ctx": 32_000
                },
            }))
            .send()
            .await?;

        if response.error_for_status_ref().is_err() {
            let error_response: serde_json::Value = response.json().await?;
            Err(anyhow!("API error: {error_response}"))
        } else {
            let mut is_reasoning = false;
            let stream = response
                .bytes_stream()
                .flat_map(move |chunk_result| match chunk_result {
                    Ok(chunk_bytes) => {
                        let chunk: Value = serde_json::from_slice(&chunk_bytes).unwrap();
                        let chunks = if let Some(thinking) = chunk["thinking"].as_str()
                            && !thinking.is_empty()
                        {
                            let chunk_content = client::StreamedReponseContent::ChunkReasoning {
                                content: thinking.to_string(),
                            };
                            if !is_reasoning {
                                is_reasoning = true;
                                let event =
                                    client::StreamedReponseContent::ContentEvent(StartReasoning);
                                vec![Ok(event), Ok(chunk_content)]
                            } else {
                                vec![Ok(chunk_content)]
                            }
                        } else if let Some(response) = chunk["response"].as_str() {
                            let chunk_content = client::StreamedReponseContent::ChunkText {
                                content: response.to_string(),
                            };
                            if is_reasoning {
                                is_reasoning = false;
                                let event =
                                    client::StreamedReponseContent::ContentEvent(StopReasoning);
                                vec![Ok(event), Ok(chunk_content)]
                            } else {
                                vec![Ok(chunk_content)]
                            }
                        } else {
                            vec![Err(anyhow!("Unespected result!"))]
                        };
                        stream::iter(chunks)
                    }
                    Err(_) => stream::iter(vec![Err(anyhow!("Unespected error!"))]),
                });

            Ok(stream.boxed())
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
                "options": {
                    "num_ctx": 32_000
                },
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
        messages: &[ChatMessage],
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamedReponseContent>>> {
        let inner_messages: Vec<Value> = messages
            .iter()
            .map(|message| {
                json!({
                    "role": &message.role,
                    "content": &message.content
                })
            })
            .collect();

        let response = self
            .inner
            .post(self.api_base_url.join("/api/chat")?)
            .json(&json!({
                "model": model,
                "messages": inner_messages,
                "stream": true,
                "options": {
                    "num_ctx": 32_000
                },
            }))
            .send()
            .await?;

        if response.error_for_status_ref().is_err() {
            let error_response: serde_json::Value = response.json().await?;
            Err(anyhow!("API error: {error_response}"))
        } else {
            let mut message_rule: Option<String> = None;
            let mut message_content = String::new();
            let mut is_reasoning = false;

            let stream = response
                .bytes_stream()
                .map(|chunk_result| match chunk_result {
                    Ok(chunk_bytes) => Chunk::Some(Ok(chunk_bytes)),
                    Err(error) => Chunk::Some(Err(error.into())),
                })
                .chain(stream::once(async { Chunk::None }))
                .flat_map(move |chunk_result| match chunk_result {
                    Chunk::Some(Ok(chunk_bytes)) => {
                        let chunk: Value = serde_json::from_slice(&chunk_bytes).unwrap();
                        let chunks = if let (Some(role), Some(thinking)) = (
                            chunk["message"]["role"].as_str(),
                            chunk["message"]["thinking"].as_str(),
                        ) && !thinking.is_empty()
                        {
                            let chunk_content =
                                client::StreamedReponseContent::ChunkMessageReasoning {
                                    role: role.to_string(),
                                    content: thinking.to_string(),
                                };
                            if !is_reasoning {
                                is_reasoning = true;
                                let event =
                                    client::StreamedReponseContent::ContentEvent(StartReasoning);
                                vec![Ok(event), Ok(chunk_content)]
                            } else {
                                vec![Ok(chunk_content)]
                            }
                        } else if let (Some(role), Some(response)) = (
                            chunk["message"]["role"].as_str(),
                            chunk["message"]["content"].as_str(),
                        ) {
                            if message_rule.is_none() {
                                message_rule = Some(role.to_string());
                            }
                            message_content.push_str(response);

                            let chunk_content = client::StreamedReponseContent::ChunkMessageText {
                                role: role.to_string(),
                                content: response.to_string(),
                            };
                            if is_reasoning {
                                is_reasoning = false;
                                let event =
                                    client::StreamedReponseContent::ContentEvent(StopReasoning);
                                vec![Ok(event), Ok(chunk_content)]
                            } else {
                                vec![Ok(chunk_content)]
                            }
                        } else {
                            vec![Err(anyhow!("Unespected result!"))]
                        };
                        stream::iter(chunks)
                    }
                    Chunk::Some(Err(_)) => stream::iter(vec![Err(anyhow!("Unespected error!"))]),
                    Chunk::None => stream::iter(vec![Ok(StreamedReponseContent::MessageText {
                        role: message_rule.clone().unwrap(),
                        content: message_content.clone(),
                    })]),
                });

            Ok(stream.boxed())
        }
    }

    async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> anyhow::Result<client::ResponseContent> {
        let inner_messages: Vec<Value> = messages
            .iter()
            .map(|message| {
                json!({
                    "role": &message.role,
                    "content": &message.content
                })
            })
            .collect();

        let response = self
            .inner
            .post(self.api_base_url.join("/api/chat")?)
            .json(&json!({
                "model": model,
                "messages": inner_messages,
                "stream": false,
                "options": {
                    "num_ctx": 32_000
                },
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

impl From<&str> for OllamaClient {
    fn from(api_base_url: &str) -> Self {
        OllamaClient {
            api_base_url: Url::from_str(api_base_url).unwrap(),
            inner: reqwest::Client::new(),
        }
    }
}

impl From<String> for OllamaClient {
    fn from(api_base_url: String) -> Self {
        Self::from(api_base_url.as_str())
    }
}

impl Default for OllamaClient {
    fn default() -> Self {
        OllamaClient {
            api_base_url: Url::from_str(ollama::DEFAULT_API_BASE_URL).unwrap(),
            inner: reqwest::Client::new(),
        }
    }
}
