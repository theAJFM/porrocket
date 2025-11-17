use anyhow::{bail, Context, Result};
use clap::Parser;
use signal_hook::consts::{SIGINT, SIGKILL, SIGTERM};
use signal_hook::flag;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "porrocket")]
#[command(about = "Redirect TCP port bindings to Unix sockets", long_about = None)]
struct Args {
    /// Port number to intercept
    #[arg(short, long)]
    port: u16,

    /// Unix socket path to redirect to
    #[arg(short = 'u', long)]
    socket: PathBuf,

    /// Command to execute
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

fn get_hook_library_path() -> Result<PathBuf> {
    // Only Linux is supported
    #[cfg(not(target_os = "linux"))]
    compile_error!("porrocket only supports Linux. macOS and other platforms are not supported due to security restrictions.");

    // Get the path to the current executable
    let exe_path = env::current_exe().context("Failed to get current executable path")?;
    let exe_dir = exe_path
        .parent()
        .context("Failed to get executable directory")?;

    // Look for the hook library in the same directory
    let lib_name = "libporrocket_hook.so";

    let lib_path = exe_dir.join(lib_name);
    if !lib_path.exists() {
        bail!("Hook library not found at: {}", lib_path.display());
    }

    Ok(lib_path)
}

fn cleanup_socket(socket_path: &PathBuf) {
    if socket_path.exists() {
        match fs::remove_file(socket_path) {
            Ok(_) => eprintln!("[porrocket] Cleaned up socket file: {}", socket_path.display()),
            Err(e) => eprintln!("[porrocket] Failed to clean up socket file: {}", e),
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate that we have a command to run
    if args.command.is_empty() {
        bail!("No command specified");
    }

    // Get the hook library path
    let hook_lib = get_hook_library_path()
        .context("Failed to locate hook library")?;

    // Setup signal handlers for cleanup
    let term = Arc::new(AtomicBool::new(false));
    let socket_path_for_signal = args.socket.clone();

    flag::register(SIGTERM, Arc::clone(&term))?;
    flag::register(SIGINT, Arc::clone(&term))?;
    flag::register(SIGKILL, Arc::clone(&term))?;

    // Extract command and arguments
    let (cmd, cmd_args) = args.command.split_first().unwrap();

    // Prepare the command with environment variables
    let mut command = Command::new(cmd);
    command.args(cmd_args);

    // Set environment variables for the hook
    command.env("PORROCKET_PORT", args.port.to_string());
    command.env("PORROCKET_SOCKET", args.socket.to_str().unwrap_or(""));

    // Set LD_PRELOAD to inject our hook library
    command.env("LD_PRELOAD", hook_lib.to_str().unwrap());

    // Execute the command
    let status = command
        .status()
        .context("Failed to execute command")?;

    // Clean up socket file after child exits
    cleanup_socket(&args.socket);

    // Check if we received a signal during execution
    if term.load(Ordering::Relaxed) {
        cleanup_socket(&socket_path_for_signal);
        std::process::exit(130); // Standard exit code for SIGINT
    }

    // Exit with the same status code as the child process
    std::process::exit(status.code().unwrap_or(1));
}
