use crate::mode::{get_agent, run_agent};
use crate::providers::Compatibility;

pub async fn run(
    model: &str,
    input: Option<&str>,
    compatibility: Compatibility,
    api_base_url: Option<&str>,
) -> anyhow::Result<()> {
    let agent = get_agent(model, compatibility, api_base_url)?;

    run_agent(agent, input).await
}
