use std::io::Write;

use anyhow::Result;
use serde_json::{Value, json};

#[tokio::main]
async fn main() -> Result<()> {
    let client = reqwest::Client::new();

    let payload = json!({
        "model": "gemma4",
        "prompt": "Why is the sky blue?",
        "stream": true,
    });

    let mut response = client
        .post("http://localhost:11434/api/generate")
        .json(&payload)
        .send()
        .await?;

    if payload["stream"].as_bool().unwrap() {
        while let Some(chunk) = response.chunk().await? {
            let chunk: Value = serde_json::from_slice(&chunk)?;
            print!("{}", chunk["response"].as_str().unwrap());
            std::io::stdout().flush()?
        }
    } else {
        let response = response.json::<Value>().await?;
        println!("{}", response["response"].as_str().unwrap())
    }

    Ok(())
}
