#![deny(rustdoc::broken_intra_doc_links, rustdoc::bare_urls, rust_2018_idioms)]

use clap::{Parser, Subcommand};

#[derive(Clone, Parser)]
#[clap(version)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Subcommand)]
enum Commands {
    /// Render trace dump files in the terminal
    View {
        #[clap(flatten)]
        cmd: otelmeister::view::ViewCmd,
    },

    /// Convert trace dump files in the terminal from/to jaeger/otel
    Convert {
        #[clap(flatten)]
        cmd: otelmeister::ConvertCmd,
    },
}

fn main() -> anyhow::Result<()> {
    let Args { command } = Args::parse();
    match &command {
        Commands::View { cmd } => cmd.run()?,
        Commands::Convert { cmd } => cmd.run()?,
    }
    Ok(())
}
