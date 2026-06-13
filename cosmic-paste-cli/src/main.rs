use std::io::{self, Read};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use cosmic_paste_core::dbus::client::CosmicPasteProxy;
use cosmic_paste_core::{BUS_NAME, DAEMON_VERSION, OBJECT_PATH};

const EXIT_USAGE: u8 = 1;
const EXIT_DAEMON: u8 = 2;
const EXIT_NOT_FOUND: u8 = 3;

#[derive(Parser)]
#[command(
    name = "cosmic-paste",
    version,
    about = "COSMIC clipboard manager command-line client"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List clipboard history entries.
    History(HistoryArgs),
    /// Select a history entry (uuid or index with --index).
    Select(SelectArgs),
    /// Add text to history (reads stdin when text is omitted).
    Add {
        text: Option<String>,
    },
    /// Enable or disable clipboard tracking.
    Track {
        #[arg(value_enum)]
        mode: TrackMode,
    },
    /// Select the previous (older) history item.
    Prev,
    /// Select the next (newer) history item.
    Next,
    /// Open the panel history popup (requires applet in panel).
    ShowHistory,
    /// Print client and daemon versions.
    Version,
    /// Clear the named history (default: history).
    Empty {
        #[arg(long, default_value = "history")]
        history: String,
    },
    /// Restart the daemon (reloads persisted state).
    DaemonReexec,
}

#[derive(clap::Args)]
struct HistoryArgs {
    /// Prefix lines with zero-based index.
    #[arg(long)]
    use_index: bool,
    /// List oldest first.
    #[arg(long)]
    reverse: bool,
    /// One entry per line.
    #[arg(long)]
    oneline: bool,
    /// Print full text instead of display preview.
    #[arg(long)]
    raw: bool,
    /// NUL-separated output (for scripting).
    #[arg(long)]
    zero: bool,
}

#[derive(clap::Args)]
struct SelectArgs {
    target: String,
    /// Treat target as zero-based history index.
    #[arg(long)]
    index: bool,
}

#[derive(Clone, ValueEnum)]
enum TrackMode {
    Start,
    Stop,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("cosmic-paste: {err}");
            ExitCode::from(err.code())
        }
    }
}

struct CliError {
    code: u8,
    message: String,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_USAGE,
            message: message.into(),
        }
    }

    fn daemon(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_DAEMON,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_NOT_FOUND,
            message: message.into(),
        }
    }

    fn code(self) -> u8 {
        self.code
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

type Result<T> = std::result::Result<T, CliError>;

async fn proxy<'a>(connection: &'a zbus::Connection) -> Result<CosmicPasteProxy<'a>> {
    CosmicPasteProxy::builder(connection)
        .destination(BUS_NAME)
        .map_err(|err| CliError::daemon(format!("invalid bus name: {err}")))?
        .path(OBJECT_PATH)
        .map_err(|err| CliError::daemon(format!("invalid object path: {err}")))?
        .build()
        .await
        .map_err(|err| CliError::daemon(format!("daemon unavailable ({BUS_NAME}): {err}")))
}

async fn session() -> Result<zbus::Connection> {
    zbus::Connection::session()
        .await
        .map_err(|err| CliError::daemon(format!("session bus unavailable: {err}")))
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::History(args) => cmd_history(&args).await,
        Commands::Select(args) => cmd_select(&args).await,
        Commands::Add { text } => cmd_add(text).await,
        Commands::Track { mode } => cmd_track(mode).await,
        Commands::Prev => cmd_offset(-1).await,
        Commands::Next => cmd_offset(1).await,
        Commands::ShowHistory => cmd_show_history().await,
        Commands::Version => cmd_version().await,
        Commands::Empty { history } => cmd_empty(&history).await,
        Commands::DaemonReexec => cmd_daemon_reexec().await,
    }
}

async fn cmd_history(args: &HistoryArgs) -> Result<()> {
    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    let mut entries = proxy.get_history().await.map_err(map_method_error)?;
    if args.reverse {
        entries.reverse();
    }

    let separator = if args.zero { '\0' } else { '\n' };
    let mut out = String::new();
    for (index, (uuid, display)) in entries.iter().enumerate() {
        if !out.is_empty() {
            out.push(separator);
        }
        if args.use_index {
            out.push_str(&index.to_string());
            out.push_str(if args.oneline || args.zero { ": " } else { "\t" });
        }
        if args.raw || args.oneline || args.zero {
            out.push_str(display);
        } else {
            out.push_str(uuid);
            out.push('\t');
            out.push_str(display);
        }
    }
    if !out.is_empty() || !args.zero {
        println!("{out}");
    }
    Ok(())
}

async fn cmd_select(args: &SelectArgs) -> Result<()> {
    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    if args.index {
        let index: usize = args
            .target
            .parse()
            .map_err(|_| CliError::usage("index must be a non-negative integer"))?;
        let history = proxy.get_history().await.map_err(map_method_error)?;
        let Some((uuid, _)) = history.get(index) else {
            return Err(CliError::not_found(format!(
                "no history entry at index {index}"
            )));
        };
        proxy.select(uuid).await.map_err(map_select_error)?;
    } else {
        proxy.select(&args.target).await.map_err(map_select_error)?;
    }
    Ok(())
}

async fn cmd_add(text: Option<String>) -> Result<()> {
    let text = match text {
        Some(text) => text,
        None => {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|err| CliError::usage(format!("failed to read stdin: {err}")))?;
            buffer
        }
    };
    if text.is_empty() {
        return Err(CliError::usage("refusing to add empty text"));
    }
    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    proxy.add(&text).await.map_err(map_method_error)?;
    Ok(())
}

async fn cmd_track(mode: TrackMode) -> Result<()> {
    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    let tracking = matches!(mode, TrackMode::Start);
    proxy.track(tracking).await.map_err(map_method_error)?;
    Ok(())
}

async fn cmd_offset(offset: i32) -> Result<()> {
    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    proxy
        .select_at_offset(offset)
        .await
        .map_err(map_navigation_error)?;
    Ok(())
}

async fn cmd_show_history() -> Result<()> {
    cosmic_paste_core::show_history_trigger::signal();
    cosmic_paste_core::dbus::applet_activation::activate_show_history().await;

    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    let applet_present = proxy.applet_present().await.map_err(map_method_error)?;
    proxy.show_history().await.map_err(map_method_error)?;
    if !applet_present {
        eprintln!(
            "ShowHistory sent, but the panel applet is not active.\n\
             Add COSMIC Paste in Settings → Desktop → Panel → Applets, then restart cosmic-panel."
        );
    }
    Ok(())
}

async fn cmd_version() -> Result<()> {
    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    let daemon_version = proxy.version().await.map_err(map_method_error)?;
    println!("cosmic-paste {DAEMON_VERSION}");
    println!("daemon {daemon_version}");
    Ok(())
}

async fn cmd_empty(history: &str) -> Result<()> {
    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    proxy
        .empty_history(history)
        .await
        .map_err(map_method_error)?;
    Ok(())
}

async fn cmd_daemon_reexec() -> Result<()> {
    let connection = session().await?;
    let proxy = proxy(&connection).await?;
    proxy.reexecute().await.map_err(map_method_error)?;
    Ok(())
}

fn map_method_error(err: zbus::Error) -> CliError {
    if is_daemon_unavailable(&err) {
        return CliError::daemon(err.to_string());
    }
    CliError::usage(err.to_string())
}

fn map_select_error(err: zbus::Error) -> CliError {
    let message = err.to_string();
    if is_daemon_unavailable(&err) {
        return CliError::daemon(message);
    }
    if message.contains("not found") {
        return CliError::not_found(message);
    }
    CliError::usage(message)
}

fn map_navigation_error(err: zbus::Error) -> CliError {
    let message = err.to_string();
    if is_daemon_unavailable(&err) {
        return CliError::daemon(message);
    }
    if message.contains("boundary") || message.contains("empty") {
        return CliError::not_found(message);
    }
    CliError::usage(message)
}

fn is_daemon_unavailable(err: &zbus::Error) -> bool {
    match err {
        zbus::Error::InputOutput(_) => true,
        zbus::Error::FDO(fdo_err) => matches!(
            fdo_err.as_ref(),
            zbus::fdo::Error::ServiceUnknown(_)
                | zbus::fdo::Error::NameHasNoOwner(_)
                | zbus::fdo::Error::NoReply(_)
        ),
        _ => false,
    }
}