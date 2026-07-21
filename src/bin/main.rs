use std::io::Write;
use std::io::{IsTerminal, Read};

use anyhow::{Result, bail};
use assistant::agent::AgentBuilder;
use assistant::client::ContentEvent::{StartReasoning, StopReasoning};
use assistant::providers::ClientBuilder;
use assistant::{client, providers};
use clap::{Parser, ValueEnum};
use rustyline::{DefaultEditor, error::ReadlineError};
use tokio_stream::StreamExt;

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

    /// API base URL
    #[arg(short, long)]
    api_base_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let agent = match (&args.compatibility, &args.api_base_url) {
        (Compatibility::Ollama, Some(api_base_url)) => {
            let client = ClientBuilder::new(providers::Compatibility::Ollama)
                .api_base_url(api_base_url)
                .build()?;
            AgentBuilder::from_client(client)
                .model(&args.model)
                .build()?
        }
        (Compatibility::Ollama, None) => {
            let client = ClientBuilder::new(providers::Compatibility::Ollama).build()?;
            AgentBuilder::from_client(client)
                .model(&args.model)
                .build()?
        }
        (Compatibility::OpenAI, Some(api_base_url)) => {
            let client = ClientBuilder::new(providers::Compatibility::OpenAI)
                .api_base_url(api_base_url)
                .build()?;
            AgentBuilder::from_client(client)
                .model(&args.model)
                .build()?
        }
        (Compatibility::OpenAI, None) => {
            let client = ClientBuilder::new(providers::Compatibility::OpenAI).build()?;
            AgentBuilder::from_client(client)
                .model(&args.model)
                .build()?
        }
        (Compatibility::MistralRS, None) => {
            let client = ClientBuilder::new(providers::Compatibility::MistralRS).build()?;
            AgentBuilder::from_client(client).model("default").build()?
        }
        (compatibility, api_base_url) => bail!(
            "Unsupported args values: compatibility: {:?}, api_base_url: {:?} ",
            compatibility,
            api_base_url
        ),
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

    if let Some(prompt) = &prompt {
        let mut stream = agent.prompt_stream(prompt).await?;
        while let Some(chunk) = stream.next().await {
            match &chunk? {
                client::StreamedReponseContent::ContentEvent(StartReasoning) => {
                    println!("[Thinking...]")
                }
                client::StreamedReponseContent::ChunkReasoning { content } => {
                    print!("{}", content);
                    std::io::stdout().flush()?
                }
                client::StreamedReponseContent::ContentEvent(StopReasoning) => {
                    println!("\n[...Thought complete]")
                }
                client::StreamedReponseContent::ChunkText { content } => {
                    print!("{}", content);
                    std::io::stdout().flush()?
                }
                _ => continue,
            }
        }
    } else {
        let mut command = clap::Command::default()
            .disable_help_subcommand(true)
            .help_template("Commands:\n{subcommands}")
            .subcommand(clap::Command::new("/help").alias("/?").about("Print help"))
            .subcommand(clap::Command::new("/quit").alias("/q").about("Exit"));

        let mut messages: Vec<client::ChatMessage> = vec![];

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
            let mut stream = agent.chat_stream(&prompt, &mut messages).await?;
            while let Some(chunk) = stream.next().await {
                match &chunk? {
                    client::StreamedReponseContent::ContentEvent(StartReasoning) => {
                        println!("[Thinking...]");
                    }
                    client::StreamedReponseContent::ChunkMessageReasoning { content, .. } => {
                        print!("{}", content);
                    }
                    client::StreamedReponseContent::ContentEvent(StopReasoning) => {
                        println!("\n[...Thought complete]");
                    }
                    client::StreamedReponseContent::ChunkMessageText { content, .. } => {
                        print!("{}", content);
                    }
                    _ => continue,
                };
                std::io::stdout().flush()?;
            }

            println!();
        }
    }

    Ok(())
}
