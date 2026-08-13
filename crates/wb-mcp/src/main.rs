//! `wb-mcp <world-folder>` — serve a world over stdio.
//!
//! **Nothing may print to stdout but protocol frames.** stdout is the transport; a
//! stray `println!` corrupts the stream and the client fails with a parse error that
//! names nothing useful. Every message here goes to stderr, which the client shows in
//! its own logs.

use std::path::PathBuf;
use std::process::ExitCode;

use rmcp::ServiceExt;
use rmcp::transport::stdio;
use wb_mcp::WorldServer;

const USAGE: &str = "\
wb-mcp — expose a Worldbuilder world to an MCP client

    wb-mcp <world-folder>

The folder is the one containing `world.yaml`. It may also be given as
WORLDBUILDER_WORLD in the environment, which is how most MCP clients pass config.

Register with Claude Code:

    claude mcp add worldbuilder -- wb-mcp /path/to/my-world
";

#[tokio::main]
async fn main() -> ExitCode {
    let root = match world_root() {
        Some(root) => root,
        None => {
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    let server = match WorldServer::open(&root) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("wb-mcp: cannot open world at {}: {e}", root.display());
            return ExitCode::FAILURE;
        }
    };

    eprintln!("wb-mcp: serving {} over stdio", root.display());

    let service = match server.serve(stdio()).await {
        Ok(service) => service,
        Err(e) => {
            eprintln!("wb-mcp: transport failed to start: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Returns when the client disconnects, which is an ordinary shutdown, not a failure.
    if let Err(e) = service.waiting().await {
        eprintln!("wb-mcp: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn world_root() -> Option<PathBuf> {
    let from_args = std::env::args().nth(1).filter(|a| !a.starts_with('-'));
    from_args
        .or_else(|| std::env::var("WORLDBUILDER_WORLD").ok())
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}
