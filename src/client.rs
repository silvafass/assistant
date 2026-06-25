use futures::Stream;

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
        Output = anyhow::Result<impl Stream<Item = StreamedReponseContent> + Unpin>,
    > + Send;

    fn chat(
        &self,
        model: &str,
        prompt: &str,
        messages: &mut Vec<ChatMessage>,
    ) -> impl std::future::Future<Output = anyhow::Result<ResponseContent>> + Send;

    fn chat_stream(
        &self,
        model: &str,
        prompt: &str,
        messages: &mut Vec<ChatMessage>,
    ) -> impl std::future::Future<
        Output = anyhow::Result<impl Stream<Item = StreamedReponseContent> + Unpin>,
    > + Send;
}
