use std::io::Write;
use std::io::{IsTerminal, Read};

use anyhow::{Result, bail};
use assistant::agent::Agent;
use assistant::client;
use assistant::client::ContentEvent::{StartReasoning, StopReasoning};
use assistant::providers::{ollama, openai};
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

    /// Disable stream response
    #[arg(long, default_value_t = false)]
    no_stream: bool,

    /// API base URL
    #[arg(short, long)]
    api_base_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match (&args.compatibility, &args.api_base_url) {
        (Compatibility::Ollama, Some(api_base_url)) => {
            ollama::Client::from(api_base_url.as_str())
                .agent_builder(&args.model)
                .build_and_run(|agent| start_agent(agent, args))
                .await?;
        }
        (Compatibility::Ollama, None) => {
            ollama::Client::default()
                .agent_builder(&args.model)
                .build_and_run(|agent| start_agent(agent, args))
                .await?;
        }
        (Compatibility::OpenAI, Some(api_base_url)) => {
            openai::Client::from(api_base_url.as_str())
                .agent_builder(&args.model)
                .build_and_run(|agent| start_agent(agent, args))
                .await?;
        }
        (Compatibility::OpenAI, None) => {
            openai::Client::from(ollama::DEFAULT_API_BASE_URL)
                .agent_builder(&args.model)
                .build_and_run(|agent| start_agent(agent, args))
                .await?;
        }
        (Compatibility::MistralRS, None) => {
            const MISTRALRS_API_BASE_URL: &str = "http://0.0.0.0:1234";
            openai::Client::from(MISTRALRS_API_BASE_URL)
                .agent_builder("default")
                .build_and_run(|agent| start_agent(agent, args))
                .await?;
        }
        (compatibility, api_base_url) => bail!(
            "Unsupported args values: compatibility: {:?}, api_base_url: {:?} ",
            compatibility,
            api_base_url
        ),
    };

    Ok(())
}

async fn start_agent<C: client::Client>(agent: Agent<C>, args: Args) -> anyhow::Result<()> {
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

    if let Some(prompt) = &prompt {
        if stream_mode {
            let mut stream = agent.prompt_stream(prompt).await?;
            while let Some(chunk) = stream.next().await {
                match &chunk {
                    client::StreamedReponseContent::ContentEvent(StartReasoning) => {
                        println!("<reasoning>")
                    }
                    client::StreamedReponseContent::ChunkReasoning { content } => {
                        print!("{}", content);
                        std::io::stdout().flush()?
                    }
                    client::StreamedReponseContent::ContentEvent(StopReasoning) => {
                        println!("\n</reasoning>")
                    }
                    client::StreamedReponseContent::ChunkText { content } => {
                        print!("{}", content);
                        std::io::stdout().flush()?
                    }
                    _ => continue,
                }
            }
        } else {
            let content = agent.prompt(prompt).await?;
            println!("<reasoning>\n{}\n</reasoning>", content.reasoning,);
            println!("{}", content.text)
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

            if stream_mode {
                let mut stream = agent.chat_stream(&prompt, &mut messages).await?;
                while let Some(chunk) = stream.next().await {
                    match &chunk {
                        client::StreamedReponseContent::ContentEvent(StartReasoning) => {
                            println!("<reasoning>");
                        }
                        client::StreamedReponseContent::ChunkMessageReasoning {
                            content, ..
                        } => {
                            print!("{}", &content);
                        }
                        client::StreamedReponseContent::ContentEvent(StopReasoning) => {
                            println!("\n</reasoning>");
                        }
                        client::StreamedReponseContent::ChunkMessageText { content, .. } => {
                            print!("{}", &content);
                        }
                        _ => continue,
                    };
                    std::io::stdout().flush()?;
                }
            } else {
                let content = agent.chat(&prompt, &mut messages).await?;
                println!("<reasoning>\n{}\n</reasoning>", content.reasoning,);
                println!("{}", content.text)
            }

            println!();
        }
    }

    Ok(())
}
