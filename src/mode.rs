use anyhow::bail;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::io::Write;
use std::io::{IsTerminal, Read};
use tokio_stream::StreamExt;

use crate::client;
use crate::client::ContentEvent::{StartReasoning, StopReasoning};
use crate::{
    agent::{Agent, AgentBuilder},
    providers::{ClientBuilder, Compatibility},
};

pub async fn general(
    model: &str,
    input: Option<&str>,
    compatibility: Compatibility,
    api_base_url: Option<&str>,
) -> anyhow::Result<()> {
    let agent = get_agent(model, compatibility, api_base_url)?;

    run_agent(agent, input).await
}

pub async fn coding(
    model: &str,
    input: Option<&str>,
    compatibility: Compatibility,
    api_base_url: Option<&str>,
) -> anyhow::Result<()> {
    let agent = get_agent(model, compatibility, api_base_url)?;

    run_agent(agent, input).await
}

pub async fn acp() -> anyhow::Result<()> {
    dbg!("acp");
    Ok(())
}

fn get_agent(
    model: &str,
    compatibility: Compatibility,
    api_base_url: Option<&str>,
) -> anyhow::Result<Agent> {
    let agent = match (&compatibility, api_base_url) {
        (Compatibility::Ollama, Some(api_base_url)) => {
            let client = ClientBuilder::new(compatibility)
                .api_base_url(api_base_url)
                .build()?;
            AgentBuilder::from_client(client).model(model).build()?
        }
        (Compatibility::Ollama, None) => {
            let client = ClientBuilder::new(compatibility).build()?;
            AgentBuilder::from_client(client).model(model).build()?
        }
        (Compatibility::OpenAI, Some(api_base_url)) => {
            let client = ClientBuilder::new(Compatibility::OpenAI)
                .api_base_url(api_base_url)
                .build()?;
            AgentBuilder::from_client(client).model(model).build()?
        }
        (Compatibility::OpenAI, None) => {
            let client = ClientBuilder::new(Compatibility::OpenAI).build()?;
            AgentBuilder::from_client(client).model(model).build()?
        }
        (Compatibility::MistralRS, None) => {
            let client = ClientBuilder::new(Compatibility::MistralRS).build()?;
            AgentBuilder::from_client(client).model("default").build()?
        }
        (compatibility, api_base_url) => bail!(
            "Unsupported args values: compatibility: {:?}, api_base_url: {:?} ",
            compatibility,
            api_base_url
        ),
    };

    Ok(agent)
}

async fn run_agent(agent: Agent, input: Option<&str>) -> anyhow::Result<()> {
    let prompt = if let Some(input) = input {
        Some(input.to_string())
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
