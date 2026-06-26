#[derive(Debug)]
pub enum Compatibility {
    /// Ollama API compatibility
    Ollama,
    /// OpenAI API compatibility (useful for integrate with OpenAI API-compatible providers)
    OpenAI,
    /// Mistral-rs integration compatibility (conveniently runs as an OpenAI API-compatible provider)
    MistralRS,
}

pub async fn general(
    model: &str,
    input: Option<&str>,
    compatibility: &Compatibility,
    api_base_url: Option<&str>,
) -> anyhow::Result<()> {
    dbg!("general", model, input, compatibility, api_base_url);
    Ok(())
}

pub async fn coding(
    model: &str,
    input: Option<&str>,
    compatibility: &Compatibility,
    api_base_url: Option<&str>,
) -> anyhow::Result<()> {
    dbg!("coding", model, input, compatibility, api_base_url);
    Ok(())
}

pub async fn acp() -> anyhow::Result<()> {
    dbg!("acp");
    Ok(())
}
