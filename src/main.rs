use crate::api::{Humble, LibraryItem};
use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::Shell;
use futures::{StreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::io;
use std::process::ExitCode;
use std::time::Duration;

mod api;
mod cookies;

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Default)]
enum OutputFormat {
    Json,
    Csv,

    #[default]
    Text,
}

/// Humble bundle keys
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// humblebundle.com Cookie header (`name=value; ...`), must include ory_session_*.
    /// If omitted, humblebundle.com cookies are read from Firefox.
    #[arg(short, long)]
    token: Option<String>,

    /// format to output data in
    #[arg(short, long, default_value_t, value_enum)]
    format: OutputFormat,

    /// Print shell completions for the given shell to stdout and exit
    #[arg(long, value_enum, value_name = "SHELL")]
    completions: Option<Shell>,
}

fn print_completions(shell: Shell) {
    let mut command = Args::command();
    let name = command.get_name().to_string();
    clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
}

#[derive(Serialize, Debug)]
struct UnclaimedKey {
    key: String,
    platform: String,
    bundle: String,
    url: String,
}

fn spinner(message: &'static str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{prefix:.bold.dim}{spinner} {wide_msg} {pos}/{len}")
            .unwrap()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message(message);
    pb
}

fn collect_keys(item: &LibraryItem, out: &mut Vec<UnclaimedKey>) {
    let bundle = item
        .sub_title
        .clone()
        .unwrap_or_else(|| item.title.clone());
    for key in item.unclaimed_keys() {
        out.push(UnclaimedKey {
            key: key.title.clone(),
            platform: key.platform.clone(),
            bundle: bundle.clone(),
            url: item.url(),
        });
    }
}

async fn unclaimed_keys(token: &str) -> api::Result<Vec<UnclaimedKey>> {
    let humble = Humble::new(token)?;

    let pb = spinner("Loading library");
    let action_id = humble.library_action_id().await?;
    let items = humble.library(&action_id).await?;

    let (groups, items): (Vec<_>, Vec<_>) = items.into_iter().partition(LibraryItem::is_group);

    let mut keys = Vec::new();
    for item in &items {
        collect_keys(item, &mut keys);
    }

    pb.set_length(groups.len() as u64);
    pb.set_message("Checking Humble Choice months");
    let groups: Vec<api::Result<LibraryItem>> = stream::iter(&groups)
        .map(|group| async {
            let detail = humble.group(&group.id).await;
            pb.inc(1);
            detail
        })
        .buffered(8)
        .collect()
        .await;
    pb.finish_and_clear();

    for group in groups {
        collect_keys(&group?, &mut keys);
    }

    Ok(keys)
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();

    if let Some(shell) = args.completions {
        print_completions(shell);
        return ExitCode::SUCCESS;
    }

    let Some(token) = args.token.clone().or_else(cookies::load) else {
        eprintln!(
            "no humblebundle.com cookies found; pass --token or log in to humblebundle.com in Firefox"
        );
        return ExitCode::FAILURE;
    };

    let keys = match unclaimed_keys(&token).await {
        Ok(keys) => keys,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    match args.format {
        OutputFormat::Json => {
            serde_json::to_writer(io::stdout(), &keys).unwrap();
        }
        OutputFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(io::stdout());
            for key in keys {
                wtr.serialize(key).unwrap();
            }
            wtr.flush().unwrap();
        }
        OutputFormat::Text => {
            for (i, key) in keys.iter().enumerate() {
                println!("{}. {} ({}) - {}", i + 1, key.key, key.platform, key.bundle);
            }
            println!("\n{} unclaimed keys!", keys.len());
        }
    }

    ExitCode::SUCCESS
}
