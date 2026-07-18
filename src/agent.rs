use std::fmt::Debug;

use anyhow::Ok;
use futures::{Stream, TryStreamExt, stream::BoxStream};

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
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamedReponseContent>>> {
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
    ) -> anyhow::Result<impl Stream<Item = anyhow::Result<StreamedReponseContent>>> {
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: input.to_string(),
        });

        self.client
            .chat_stream(&self.model, messages)
            .await
            .map(|stream| {
                stream.inspect_ok(|chunk| {
                    if let StreamedReponseContent::MessageText { role, content } = chunk {
                        messages.push(ChatMessage {
                            role: role.clone(),
                            content: content.clone(),
                        });
                    };
                })
            })
    }

    pub async fn chat(
        &self,
        input: &str,
        messages: &mut Vec<ChatMessage>,
    ) -> anyhow::Result<ResponseContent> {
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: input.to_string(),
        });

        self.client
            .chat(&self.model, messages)
            .await
            .inspect(|chunk| {
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: chunk.text.clone(),
                });
            })
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
