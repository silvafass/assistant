use std::io::{IsTerminal, Read, Write};

use anyhow::Result;
use clap::Parser;
use rustyline::{DefaultEditor, error::ReadlineError};
use serde_json::{Value, json};

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
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

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
        let payload = json!({
            "model": &args.model,
            "stream": true,
            "prompt": prompt,
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
    } else {
        let mut command = clap::Command::default()
            .disable_help_subcommand(true)
            .help_template("Commands:\n{subcommands}")
            .subcommand(clap::Command::new("/help").alias("/?").about("Print help"))
            .subcommand(clap::Command::new("/quit").alias("/q").about("Exit"));

        let mut messages: Vec<Value> = vec![];

        let mut rl = DefaultEditor::new()?;
        loop {
            let readline = rl.readline(">> ");
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

            messages.push(json!({
                "role": "user",
                "content": prompt
            }));

            let payload = json!({
                "model": &args.model,
                "stream": true,
                "messages": messages,
            });

            let mut response = client
                .post("http://localhost:11434/api/chat")
                .json(&payload)
                .send()
                .await?;

            if payload["stream"].as_bool().unwrap() {
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

            println!();
        }
    }

    Ok(())
}
