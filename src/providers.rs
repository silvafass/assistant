use futures::stream::BoxStream;

use crate::{
    client::{self, ChatMessage, Client, StreamedReponseContent},
    providers::{ollama::OllamaClient, openai::OpenAiClient},
};

pub mod ollama;
pub mod openai;

#[derive(Debug)]
pub enum Compatibility {
    /// Ollama API compatibility
    Ollama,
    /// OpenAI API compatibility (useful for integrate with OpenAI API-compatible providers)
    OpenAI,
    /// Mistral-rs integration compatibility (conveniently runs as an OpenAI API-compatible provider)
    MistralRS,
}

pub enum ClientProvider {
    Ollama(OllamaClient),
    OpenAi(OpenAiClient),
}

impl Client for ClientProvider {
    async fn prompt_stream(
        &self,
        payload: client::PromptPayload,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamedReponseContent>>> {
        match self {
            ClientProvider::Ollama(client) => client.prompt_stream(payload).await,
            ClientProvider::OpenAi(client) => client.prompt_stream(payload).await,
        }
    }

    async fn prompt(
        &self,
        payload: client::PromptPayload,
    ) -> anyhow::Result<client::ResponseContent> {
        match self {
            ClientProvider::Ollama(client) => client.prompt(payload).await,
            ClientProvider::OpenAi(client) => client.prompt(payload).await,
        }
    }

    async fn chat_stream(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamedReponseContent>>> {
        match self {
            ClientProvider::Ollama(client) => client.chat_stream(model, messages).await,
            ClientProvider::OpenAi(client) => client.chat_stream(model, messages).await,
        }
    }

    async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> anyhow::Result<client::ResponseContent> {
        match self {
            ClientProvider::Ollama(client) => client.chat(model, messages).await,
            ClientProvider::OpenAi(client) => client.chat(model, messages).await,
        }
    }
}

pub struct ClientBuilder {
    compatibility: Compatibility,
    api_base_url: Option<String>,
}

impl ClientBuilder {
    pub fn new(compatibility: Compatibility) -> Self {
        Self {
            compatibility,
            api_base_url: None,
        }
    }

    pub fn api_base_url(mut self, url: impl Into<String>) -> Self {
        self.api_base_url = Some(url.into());
        self
    }

    pub fn build(self) -> anyhow::Result<ClientProvider> {
        match self.compatibility {
            Compatibility::Ollama => {
                let api_base_url = self
                    .api_base_url
                    .unwrap_or(ollama::DEFAULT_API_BASE_URL.to_string());
                Ok(ClientProvider::Ollama(OllamaClient::from(api_base_url)))
            }
            Compatibility::OpenAI => {
                let api_base_url = self
                    .api_base_url
                    .unwrap_or(ollama::DEFAULT_API_BASE_URL.to_string());
                Ok(ClientProvider::OpenAi(OpenAiClient::from(api_base_url)))
            }
            Compatibility::MistralRS => {
                const MISTRALRS_API_BASE_URL: &str = "http://0.0.0.0:1234";
                let api_base_url = self
                    .api_base_url
                    .unwrap_or(MISTRALRS_API_BASE_URL.to_string());
                Ok(ClientProvider::OpenAi(OpenAiClient::from(api_base_url)))
            }
        }
    }
}
