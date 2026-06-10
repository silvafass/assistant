use anyhow::Result;
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> Result<()> {
    let client = reqwest::Client::new();

    let payload = json!({
        "model": "gemma4",
        "prompt": "Why is the sky blue?",
        "stream": false,
    });

    let response = client
        .post("http://localhost:11434/api/generate")
        .json(&payload)
        .send()
        .await?
        .json::<Value>()
        .await?;

    dbg!(response);

    Ok(())
}
