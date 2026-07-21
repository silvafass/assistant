use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    general_args: GeneralArgs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run in general-purpose mode [Default mode]
    General(GeneralArgs),
    /// Run in coding-purpose mode
    Coding(CodingArgs),
    /// Run in integration-purpose mode via Agent Client Protocal server (Code editor integrations)
    Acp,
}

#[derive(Args, Debug)]
pub struct GeneralArgs {
    #[command(flatten)]
    shared: GlobalOpts,
}

#[derive(Args, Debug)]
pub struct CodingArgs {
    #[command(flatten)]
    shared: GlobalOpts,
}

#[derive(Args, Debug)]
struct GlobalOpts {
    /// Model name
    #[arg(short, long, default_value_t = String::from("gemma4"))]
    model: String,

    /// To receive the prompt
    #[arg(short, long)]
    input: Option<String>,

    /// The API compatibility to use
    #[arg(short, long, value_enum, default_value_t = Compatibility::Ollama)]
    compatibility: Compatibility,

    /// Provider API base URL
    #[arg(short, long)]
    api_base_url: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum Compatibility {
    /// Ollama API compatibility
    Ollama,
    /// OpenAI API compatibility (useful for integrate with OpenAI API-compatible providers)
    OpenAI,
    /// Mistral-rs integration compatibility (conveniently runs as an OpenAI API-compatible provider)
    MistralRS,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let get_args = |args: GlobalOpts| {
        let compatibility = match args.compatibility {
            Compatibility::Ollama => assistant::providers::Compatibility::Ollama,
            Compatibility::OpenAI => assistant::providers::Compatibility::OpenAI,
            Compatibility::MistralRS => assistant::providers::Compatibility::MistralRS,
        };
        (args.model, args.input, compatibility, args.api_base_url)
    };

    match cli.command {
        Some(Commands::General(general_args)) => {
            let (model, input, compatibility, api_base_url) = get_args(general_args.shared);
            assistant::mode::general(
                &model,
                input.as_deref(),
                compatibility,
                api_base_url.as_deref(),
            )
            .await?;
        }
        Some(Commands::Coding(coding_args)) => {
            let (model, input, compatibility, api_base_url) = get_args(coding_args.shared);
            assistant::mode::coding(
                &model,
                input.as_deref(),
                compatibility,
                api_base_url.as_deref(),
            )
            .await?;
        }
        Some(Commands::Acp) => {
            assistant::mode::acp().await?;
        }
        None => {
            let (model, input, compatibility, api_base_url) = get_args(cli.general_args.shared);
            assistant::mode::general(
                &model,
                input.as_deref(),
                compatibility,
                api_base_url.as_deref(),
            )
            .await?;
        }
    };

    Ok(())
}
