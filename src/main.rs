use std::{
    io::{IsTerminal, Read, Write},
    str::FromStr,
};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use rustyline::{DefaultEditor, error::ReadlineError};
use serde_json::{Value, json};

const OLLAMA_API_BASE_URL: &str = "http://localhost:11434";
const MISTRALRS_API_BASE_URL: &str = "http://0.0.0.0:1234";

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Compatibility {
    Ollama,
    OpenAI,
    MistralRS,
}

/// Simple assistant program
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Model name
    #[arg(short, long, default_value_t = String::from("gemma4"))]
    model: String,

    /// Input
    #[arg(short, long)]
    input: Option<String>,

    /// API compatibility
    #[arg(short, long, value_enum, default_value_t = Compatibility::Ollama)]
    compatibility: Compatibility,

    /// Disable stream response
    #[arg(long, default_value_t = false)]
    no_stream: bool,

    /// API base URL
    #[arg(short, long, default_value_t = String::from(OLLAMA_API_BASE_URL))]
    api_base_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let stream_mode = !&args.no_stream;

    let prompt = if let Some(input) = args.input {
        Some(input)
    } else if !std::io::stdin().is_terminal() {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        Some(buffer)
    } else {
        None
    };

    let (base_url, model) = if args.compatibility == Compatibility::MistralRS {
        (reqwest::Url::from_str(MISTRALRS_API_BASE_URL)?, "default")
    } else {
        (reqwest::Url::from_str(&args.api_base_url)?, &args.model[..])
    };

    let client = reqwest::Client::new();

    if let Some(prompt) = prompt {
        match args.compatibility {
            Compatibility::Ollama => {
                let payload = json!({
                    "model": model,
                    "stream": stream_mode,
                    "prompt": prompt,
                });

                let mut response = client
                    .post(base_url.join("/api/generate")?)
                    .json(&payload)
                    .send()
                    .await?;

                if stream_mode {
                    let mut is_reasoning = false;
                    while let Some(chunk) = response.chunk().await? {
                        let chunk: Value = serde_json::from_slice(&chunk)?;

                        let chunk = if let Some(thinking) = chunk["thinking"].as_str()
                            && !thinking.is_empty()
                        {
                            if !is_reasoning {
                                is_reasoning = true;
                                println!("<reasoning>");
                            }
                            thinking
                        } else if let Some(response) = chunk["response"].as_str()
                            && !response.is_empty()
                        {
                            if is_reasoning {
                                is_reasoning = false;
                                println!("\n</reasoning>")
                            }
                            response
                        } else {
                            continue;
                        };
                        print!("{}", chunk);
                        std::io::stdout().flush()?
                    }
                } else {
                    let response = response.json::<Value>().await?;
                    println!(
                        "<reasoning>\n{}\n</reasoning>",
                        response["thinking"].as_str().unwrap_or_default()
                    );
                    println!("{}", response["response"].as_str().unwrap())
                }
            }
            Compatibility::OpenAI | Compatibility::MistralRS => {
                let payload = json!({
                    "model": model,
                    "stream": stream_mode,
                    "input": prompt,
                });

                let mut response = client
                    .post(base_url.join("/v1/responses")?)
                    .json(&payload)
                    .send()
                    .await?;

                if stream_mode {
                    let mut is_reasoning = false;
                    while let Some(chunk) = response.chunk().await? {
                        const REASONING_SUMMARY_TEXT_EVENT: &[u8; 51] =
                            b"event: response.reasoning_summary_text.delta\ndata: ";
                        const OUTPUT_TEXT_EVENT: &[u8; 40] =
                            b"event: response.output_text.delta\ndata: ";

                        let chunk = if chunk.starts_with(REASONING_SUMMARY_TEXT_EVENT) {
                            if !is_reasoning {
                                is_reasoning = true;
                                println!("<reasoning>");
                            }
                            &chunk[REASONING_SUMMARY_TEXT_EVENT.len()..]
                        } else if chunk.starts_with(OUTPUT_TEXT_EVENT) {
                            if is_reasoning {
                                is_reasoning = false;
                                println!("\n</reasoning>")
                            }
                            &chunk[OUTPUT_TEXT_EVENT.len()..]
                        } else {
                            if let Ok(chunk) = serde_json::from_slice::<Value>(&chunk) {
                                eprintln!("{chunk}");
                            }
                            continue;
                        };

                        let chunk: Value = serde_json::from_slice(chunk)?;

                        print!("{}", chunk["delta"].as_str().unwrap());

                        std::io::stdout().flush()?
                    }
                } else {
                    let response = response.json::<Value>().await?;
                    println!("<reasoning>");
                    for output in response["output"].as_array().unwrap() {
                        if output["type"] == "reasoning" {
                            println!("{}", output["summary"][0]["text"].as_str().unwrap())
                        }
                    }
                    println!("</reasoning>");
                    for output in response["output"].as_array().unwrap() {
                        if output["type"] == "message" {
                            println!("{}", output["content"][0]["text"].as_str().unwrap())
                        }
                    }
                }
            }
        }
    } else {
        let mut command = clap::Command::default()
            .disable_help_subcommand(true)
            .help_template("Commands:\n{subcommands}")
            .subcommand(clap::Command::new("/help").alias("/?").about("Print help"))
            .subcommand(clap::Command::new("/quit").alias("/q").about("Exit"));

        let mut messages: Vec<Value> = vec![];

        let mut rl = DefaultEditor::new()?;
        loop {
            let readline = rl.readline("❯ ");
            let prompt = match readline {
                Ok(line) => {
                    rl.add_history_entry(line.as_str())?;
                    line
                }
                Err(ReadlineError::Interrupted) => break,
                Err(ReadlineError::Eof) => break,
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            };

            if prompt.starts_with("/") {
                let matches = command.clone().try_get_matches_from(
                    format!(". {prompt}")
                        .split_whitespace()
                        .collect::<Vec<&str>>(),
                );
                match matches {
                    Ok(matches) => match matches.subcommand() {
                        Some(("/quit", ..)) => {
                            break;
                        }
                        Some(("/help", ..)) => {
                            command.print_long_help()?;
                        }
                        _ => {
                            println!("Not yet implemented!");
                        }
                    },
                    Err(err) => {
                        println!("{err}");
                    }
                }

                continue;
            }

            match args.compatibility {
                Compatibility::Ollama => {
                    messages.push(json!({
                        "role": "user",
                        "content": prompt
                    }));

                    let payload = json!({
                        "model": model,
                        "stream": stream_mode,
                        "messages": messages,
                    });

                    let mut response = client
                        .post(base_url.join("/api/chat")?)
                        .json(&payload)
                        .send()
                        .await?;

                    if stream_mode {
                        let mut role: Option<String> = None;
                        let mut content = String::new();
                        let mut is_reasoning = false;

                        while let Some(chunk) = response.chunk().await? {
                            let chunk: Value = serde_json::from_slice(&chunk)?;

                            let chunk_text = if let Some(thinking) =
                                chunk["message"]["thinking"].as_str()
                                && !thinking.is_empty()
                            {
                                if !is_reasoning {
                                    is_reasoning = true;
                                    println!("<reasoning>");
                                }
                                thinking
                            } else if let Some(content) = chunk["message"]["content"].as_str()
                                && !content.is_empty()
                            {
                                if is_reasoning {
                                    is_reasoning = false;
                                    println!("\n</reasoning>")
                                }
                                content
                            } else {
                                continue;
                            };

                            content.push_str(chunk_text);
                            print!("{}", chunk_text);
                            std::io::stdout().flush()?;

                            if role.is_none() {
                                role = Some(chunk["message"]["role"].to_string());
                            }
                        }

                        messages.push(json!({
                            "role": role,
                            "content": content
                        }));
                    } else {
                        let response = response.json::<Value>().await?;

                        println!(
                            "<reasoning>\n{}\n</reasoning>",
                            response["message"]["thinking"].as_str().unwrap()
                        );
                        println!("{}", response["message"]["content"].as_str().unwrap());

                        messages.push(json!({
                            "role": response["message"]["role"],
                            "content": response["message"]["content"]
                        }));
                    }
                }
                Compatibility::OpenAI | Compatibility::MistralRS => {
                    messages.push(json!({
                        "role": "user",
                        "content": prompt
                    }));

                    let payload = json!({
                        "model": model,
                        "stream": stream_mode,
                        "messages": messages,
                    });

                    let mut response = client
                        .post(base_url.join("/v1/chat/completions")?)
                        .json(&payload)
                        .send()
                        .await?;

                    if stream_mode {
                        let mut role: Option<String> = None;
                        let mut content = String::new();

                        let mut is_reasoning = false;

                        while let Some(chunk) = response.chunk().await? {
                            let chunk = if &chunk[..] == b"data: [DONE]\n\n" {
                                continue;
                            } else if chunk.starts_with(b"data: ")
                                && chunk.ends_with(b"data: [DONE]\n\n")
                            {
                                &chunk[..chunk.len() - b"data: [DONE]\n\n".len()][b"data: ".len()..]
                            } else if chunk.starts_with(b"data: ") {
                                &chunk[b"data: ".len()..]
                            } else {
                                continue;
                            };

                            let chunk: Value = serde_json::from_slice(chunk)?;

                            let chunk_text = if let Value::String(text) =
                                &chunk["choices"][0]["delta"]["reasoning"]
                            {
                                if !is_reasoning {
                                    is_reasoning = true;
                                    println!("<reasoning>");
                                }
                                text
                            } else if let Value::String(text) =
                                &chunk["choices"][0]["delta"]["content"]
                            {
                                if is_reasoning {
                                    is_reasoning = false;
                                    println!("\n</reasoning>")
                                }
                                text
                            } else {
                                if let Value::Object(chunk) = &chunk {
                                    eprintln!("{chunk:?}");
                                }
                                continue;
                            };

                            content.push_str(chunk_text);
                            print!("{}", chunk_text);
                            std::io::stdout().flush()?;

                            if role.is_none() {
                                role = Some(
                                    chunk["choices"][0]["delta"]["role"]
                                        .as_str()
                                        .unwrap()
                                        .to_string(),
                                );
                            }
                        }

                        messages.push(json!({
                            "role": role,
                            "content": content
                        }));
                    } else {
                        let response = response.json::<Value>().await?;
                        println!(
                            "{}",
                            response["choices"][0]["message"]["content"]
                                .as_str()
                                .unwrap()
                        );

                        messages.push(json!({
                            "role": response["choices"][0]["message"]["role"],
                            "content": response["choices"][0]["message"]["content"]
                        }));
                    }
                }
            }

            println!();
        }
    }

    Ok(())
}
