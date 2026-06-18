use std::io::{IsTerminal, Read};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use rustyline::{DefaultEditor, error::ReadlineError};
use serde_json::json;

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

    let (base_url, model) = if args.compatibility == Compatibility::MistralRS {
        (MISTRALRS_API_BASE_URL, "default")
    } else {
        (args.api_base_url.as_str(), &args.model[..])
    };

    let prompt = if let Some(input) = args.input {
        Some(input)
    } else if !std::io::stdin().is_terminal() {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        Some(buffer)
    } else {
        None
    };

    let stream_mode = !&args.no_stream;

    if let Some(prompt) = prompt {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "base_url": base_url,
                "model": model,
                "prompt": prompt,
                "stream_mode": stream_mode
            }))?,
        );
        match args.compatibility {
            Compatibility::Ollama => {}
            Compatibility::OpenAI | Compatibility::MistralRS => {}
        }
    } else {
        let mut command = clap::Command::default()
            .disable_help_subcommand(true)
            .help_template("Commands:\n{subcommands}")
            .subcommand(clap::Command::new("/help").alias("/?").about("Print help"))
            .subcommand(clap::Command::new("/quit").alias("/q").about("Exit"));

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

            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "base_url": base_url,
                    "model": model,
                    "prompt": prompt,
                    "stream_mode": stream_mode
                }))?,
            );
            match args.compatibility {
                Compatibility::Ollama => {}
                Compatibility::OpenAI | Compatibility::MistralRS => {}
            }

            println!();
        }
    }

    Ok(())
}
