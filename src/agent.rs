use std::fmt::Debug;

use anyhow::Ok;
use futures::Stream;

use crate::client::{ChatMessage, Client, PromptPayload, ResponseContent, StreamedReponseContent};

#[derive(Debug, Default)]
pub struct Agent<C>
where
    C: Client,
{
    client: C,
    model: String,
}

impl<C: Client> Agent<C> {
    pub async fn prompt_stream(
        &self,
        input: &str,
    ) -> anyhow::Result<impl Stream<Item = StreamedReponseContent>> {
        let payload = PromptPayload {
            model: self.model.to_string(),
            prompt: input.to_string(),
        };
        self.client.prompt_stream(payload).await
    }

    pub async fn prompt(&self, input: &str) -> anyhow::Result<ResponseContent> {
        let payload = PromptPayload {
            model: self.model.to_string(),
            prompt: input.to_string(),
        };
        self.client.prompt(payload).await
    }

    pub async fn chat_stream(
        &self,
        input: &str,
        messages: &mut Vec<ChatMessage>,
    ) -> anyhow::Result<impl Stream<Item = StreamedReponseContent>> {
        self.client.chat_stream(&self.model, input, messages).await
    }

    pub async fn chat(
        &self,
        input: &str,
        messages: &mut Vec<ChatMessage>,
    ) -> anyhow::Result<ResponseContent> {
        self.client.chat(&self.model, input, messages).await
    }
}

#[derive(Default)]
pub struct AgentBuilder<C>
where
    C: Client,
{
    client: C,
    model: Option<String>,
}

impl<C: Client> AgentBuilder<C> {
    pub fn from_client(client: C) -> Self {
        AgentBuilder {
            client,
            model: None,
        }
    }

    pub fn model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    pub fn build(self) -> anyhow::Result<Agent<C>> {
        let agent = Agent {
            client: self.client,
            model: self.model.unwrap(),
        };

        Ok(agent)
    }

    pub fn build_and_run<F, Fut>(self, handler: F) -> Fut
    where
        F: FnOnce(Agent<C>) -> Fut,
        Fut: Future<Output = anyhow::Result<()>>,
    {
        let agent = self.build().unwrap();

        handler(agent)
    }
}
