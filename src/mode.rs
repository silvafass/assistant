use crate::providers::Compatibility;

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
