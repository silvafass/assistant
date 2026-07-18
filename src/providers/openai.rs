use std::str::FromStr;

use anyhow::anyhow;
use futures::{
    StreamExt,
    stream::{self, BoxStream},
};
use reqwest::Url;
use serde_json::{Value, json};

use crate::{
    agent::AgentBuilder,
    client::{
        self, ChatMessage, Chunk, Client,
        ContentEvent::{StartReasoning, StopReasoning},
        StreamedReponseContent,
    },
    providers::openai,
};

pub const DEFAULT_API_BASE_URL: &str = "http://localhost:11434";

#[derive(Debug)]
pub struct OpenAiClient {
    api_base_url: Url,
    inner: reqwest::Client,
}

impl Client for OpenAiClient {
    async fn prompt_stream(
        &self,
        payload: client::PromptPayload,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamedReponseContent>>> {
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
            let mut is_reasoning = false;
            let stream = response
                .bytes_stream()
                .flat_map(move |chunk_result| match chunk_result {
                    Ok(chunk_bytes) => {
                        const REASONING_SUMMARY_TEXT_EVENT: &[u8; 51] =
                            b"event: response.reasoning_summary_text.delta\ndata: ";
                        const OUTPUT_TEXT_EVENT: &[u8; 40] =
                            b"event: response.output_text.delta\ndata: ";
                        let chunks = if chunk_bytes.starts_with(REASONING_SUMMARY_TEXT_EVENT) {
                            let chunk_bytes = &chunk_bytes[REASONING_SUMMARY_TEXT_EVENT.len()..];
                            let chunk: Value = serde_json::from_slice(chunk_bytes).unwrap();
                            let chunk_content = client::StreamedReponseContent::ChunkReasoning {
                                content: chunk["delta"].as_str().unwrap().to_string(),
                            };
                            if !is_reasoning {
                                is_reasoning = true;
                                let event =
                                    client::StreamedReponseContent::ContentEvent(StartReasoning);
                                vec![Ok(event), Ok(chunk_content)]
                            } else {
                                vec![Ok(chunk_content)]
                            }
                        } else if chunk_bytes.starts_with(OUTPUT_TEXT_EVENT) {
                            let chunk_bytes = &chunk_bytes[OUTPUT_TEXT_EVENT.len()..];
                            let chunk: Value = serde_json::from_slice(chunk_bytes).unwrap();
                            let chunk_content = client::StreamedReponseContent::ChunkText {
                                content: chunk["delta"].as_str().unwrap().to_string(),
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
                    Err(_) => stream::iter(vec![Err(anyhow!("Unespected result!"))]),
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
            let mut message_rule: Option<String> = None;
            let mut message_content = String::new();
            let mut is_reasoning = false;

            let stream = response
                .bytes_stream()
                .filter_map(|chunk_result| async move {
                    match chunk_result {
                        Ok(chunk_bytes) => {
                            if &chunk_bytes[..] == b"data: [DONE]\n\n" {
                                None
                            } else if chunk_bytes.starts_with(b"data: ")
                                && chunk_bytes.ends_with(b"data: [DONE]\n\n")
                            {
                                let chunk_bytes =
                                    &chunk_bytes[..chunk_bytes.len() - b"data: [DONE]\n\n".len()];
                                let chunk_bytes = &chunk_bytes[b"data: ".len()..];
                                Some(Ok(chunk_bytes.to_owned()))
                            } else if chunk_bytes.starts_with(b"data: ") {
                                let chunk_bytes = &chunk_bytes[b"data: ".len()..];
                                Some(Ok(chunk_bytes.to_owned()))
                            } else {
                                None
                            }
                        }
                        Err(_) => Some(Err(anyhow!("Unespected result!"))),
                    }
                })
                .map(|chunk_result| match chunk_result {
                    Ok(chunk_bytes) => Chunk::Some(Ok(chunk_bytes.to_owned().into())),
                    Err(error) => Chunk::Some(Err(error)),
                })
                .chain(stream::once(async { Chunk::None }))
                .flat_map(move |chunk_result| match chunk_result {
                    Chunk::Some(Ok(chunk_bytes)) => {
                        let chunk: Value = serde_json::from_slice(&chunk_bytes).unwrap();

                        let chunks = if let (Some(role), Some(thinking)) = (
                            chunk["choices"][0]["delta"]["role"].as_str(),
                            chunk["choices"][0]["delta"]["reasoning"].as_str(),
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
                            chunk["choices"][0]["delta"]["role"].as_str(),
                            chunk["choices"][0]["delta"]["content"].as_str(),
                        ) && !response.is_empty()
                        {
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
                    Chunk::Some(Err(_)) => stream::iter(vec![Err(anyhow!("Unespected result!"))]),
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

impl From<&str> for OpenAiClient {
    fn from(api_base_url: &str) -> Self {
        OpenAiClient {
            api_base_url: Url::from_str(api_base_url).unwrap(),
            inner: reqwest::Client::new(),
        }
    }
}

impl From<String> for OpenAiClient {
    fn from(api_base_url: String) -> Self {
        Self::from(api_base_url.as_str())
    }
}

impl Default for OpenAiClient {
    fn default() -> Self {
        OpenAiClient {
            api_base_url: Url::from_str(openai::DEFAULT_API_BASE_URL).unwrap(),
            inner: reqwest::Client::new(),
        }
    }
}

impl OpenAiClient {
    pub fn agent_builder(self) -> AgentBuilder<OpenAiClient> {
        AgentBuilder::from_client(self)
    }
}
