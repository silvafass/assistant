use std::io::{IsTerminal, Read, Write};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use rustyline::{DefaultEditor, error::ReadlineError};
use serde_json::{Value, json};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Compatibility {
    Ollama,
    OpenAI,
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

    let client = reqwest::Client::new();

    if let Some(prompt) = prompt {
        match args.compatibility {
            Compatibility::Ollama => {
                let payload = json!({
                    "model": &args.model,
                    "stream": stream_mode,
                    "prompt": prompt,
                });

                let mut response = client
                    .post("http://localhost:11434/api/generate")
                    .json(&payload)
                    .send()
                    .await?;

                if stream_mode {
                    while let Some(chunk) = response.chunk().await? {
                        let chunk: Value = serde_json::from_slice(&chunk)?;
                        print!("{}", chunk["response"].as_str().unwrap());
                        std::io::stdout().flush()?
                    }
                } else {
                    let response = response.json::<Value>().await?;
                    println!("{}", response["response"].as_str().unwrap())
                }
            }
            Compatibility::OpenAI => {
                let payload = json!({
                    "model": &args.model,
                    "stream": stream_mode,
                    "input": prompt,
                });

                let mut response = client
                    .post("http://localhost:11434/v1/responses")
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
                            continue;
                        };

                        let chunk: Value = serde_json::from_slice(chunk)?;

                        print!("{}", chunk["delta"].as_str().unwrap());

                        std::io::stdout().flush()?
                    }
                } else {
                    let response = response.json::<Value>().await?;
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
                        "model": &args.model,
                        "stream": stream_mode,
                        "messages": messages,
                    });

                    let mut response = client
                        .post("http://localhost:11434/api/chat")
                        .json(&payload)
                        .send()
                        .await?;

                    if stream_mode {
                        let mut role: Option<String> = None;
                        let mut content = String::new();

                        while let Some(chunk) = response.chunk().await? {
                            let chunk: Value = serde_json::from_slice(&chunk)?;
                            content.push_str(chunk["message"]["content"].as_str().unwrap());
                            print!("{}", chunk["message"]["content"].as_str().unwrap());
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
                        println!("{}", response["message"]["content"].as_str().unwrap());

                        messages.push(json!({
                            "role": response["message"]["role"],
                            "content": response["message"]["content"]
                        }));
                    }
                }
                Compatibility::OpenAI => {
                    messages.push(json!({
                        "role": "user",
                        "content": prompt
                    }));

                    let payload = json!({
                        "model": &args.model,
                        "stream": stream_mode,
                        "messages": messages,
                    });

                    let mut response = client
                        .post("http://localhost:11434/v1/chat/completions")
                        .json(&payload)
                        .send()
                        .await?;

                    if stream_mode {
                        let mut role: Option<String> = None;
                        let mut content = String::new();

                        while let Some(chunk) = response.chunk().await? {
                            let chunk = if chunk.starts_with(b"data: ")
                                && chunk.ends_with(b"data: [DONE]\n\n")
                            {
                                &chunk[..chunk.len() - b"data: [DONE]\n\n".len()][b"data: ".len()..]
                            } else if chunk.starts_with(b"data: ") {
                                &chunk[b"data: ".len()..]
                            } else {
                                continue;
                            };

                            let chunk: Value = serde_json::from_slice(chunk)?;
                            content.push_str(
                                chunk["choices"][0]["delta"]["content"].as_str().unwrap(),
                            );
                            print!(
                                "{}",
                                chunk["choices"][0]["delta"]["content"].as_str().unwrap()
                            );
                            std::io::stdout().flush()?;

                            if role.is_none() {
                                role = Some(chunk["choices"][0]["delta"]["role"].to_string());
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
