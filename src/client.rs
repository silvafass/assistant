use bytes::Bytes;
use futures::stream::BoxStream;

pub enum ContentEvent {
    StartReasoning,
    StopReasoning,
    StartTexting,
    StopTexting,
}

pub enum StreamedReponseContent {
    ChunkReasoning { content: String },
    ChunkText { content: String },
    ChunkMessageReasoning { role: String, content: String },
    ChunkMessageText { role: String, content: String },
    MessageText { role: String, content: String },
    ContentEvent(ContentEvent),
    Content { reasoning: String, text: String },
}

pub enum Chunk {
    Some(Result<Bytes, anyhow::Error>),
    None,
}

pub struct ResponseContent {
    pub reasoning: String,
    pub text: String,
}

pub struct PromptPayload {
    pub model: String,
    pub prompt: String,
}

pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub trait Client {
    fn prompt(
        &self,
        payload: PromptPayload,
    ) -> impl std::future::Future<Output = anyhow::Result<ResponseContent>> + Send;

    fn prompt_stream(
        &self,
        payload: PromptPayload,
    ) -> impl std::future::Future<
        Output = anyhow::Result<BoxStream<'static, anyhow::Result<StreamedReponseContent>>>,
    > + Send;

    fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> impl std::future::Future<Output = anyhow::Result<ResponseContent>> + Send;

    fn chat_stream(
        &self,
        model: &str,
        messages: &[ChatMessage],
    ) -> impl std::future::Future<
        Output = anyhow::Result<BoxStream<'static, anyhow::Result<StreamedReponseContent>>>,
    > + Send;
}
