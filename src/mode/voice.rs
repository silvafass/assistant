use crate::mode::get_agent;
use crate::providers::Compatibility;

pub async fn run(
    model: &str,
    _input: Option<&str>,
    compatibility: Compatibility,
    api_base_url: Option<&str>,
) -> anyhow::Result<()> {
    let _agent = get_agent(model, compatibility, api_base_url)?;

    Ok(())
}
