use anyhow::{bail, Context, Result};
use clap::Parser;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::flag;
use std::env;
use std::fs;
use std::os::fd::{IntoRawFd, RawFd};
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("porrocket only supports Linux and macOS.");

#[derive(Parser, Debug)]
#[command(name = "porrocket")]
#[command(about = "Redirect TCP port bindings to Unix sockets", long_about = None)]
struct Args {
    /// Port number to intercept
    #[arg(
        short,
        long,
        required_unless_present = "listenfd",
        conflicts_with = "listenfd"
    )]
    port: Option<u16>,

    /// Bind the Unix socket here and pass it to the command through systemd socket activation
    /// (fd 3, `LISTEN_FDS=1`, `LISTEN_PID`), then exec the command in place of porrocket.
    /// For servers that accept an inherited listener, such as `wasmtime serve --systemd-listenfd`;
    /// no hook library is loaded.
    #[arg(long)]
    listenfd: bool,

    /// Unix socket path to redirect to
    #[arg(short = 'u', long)]
    socket: PathBuf,

    /// Command to execute
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

fn get_hook_library_path() -> Result<PathBuf> {
    // Get the path to the current executable
    let exe_path = env::current_exe().context("Failed to get current executable path")?;
    let exe_dir = exe_path
        .parent()
        .context("Failed to get executable directory")?;

    // Look for the hook library in the same directory
    #[cfg(target_os = "linux")]
    let lib_name = "libporrocket_hook.so";
    #[cfg(target_os = "macos")]
    let lib_name = "libporrocket_hook.dylib";

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

/// First descriptor of the systemd socket-activation protocol (`SD_LISTEN_FDS_START`).
const LISTEN_FDS_START: RawFd = 3;

/// Binds `socket_path`, places the listener on fd 3 and execs `command` with the systemd
/// socket-activation environment. `exec` keeps this process's PID, which is what makes
/// `LISTEN_PID` match in the command. Only returns on failure.
fn exec_with_listenfd(socket_path: &PathBuf, command: &[String]) -> Result<()> {
    if socket_path.exists() {
        fs::remove_file(socket_path)
            .with_context(|| format!("Failed to remove stale socket {}", socket_path.display()))?;
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("Failed to bind {}", socket_path.display()))?;
    let fd = listener.into_raw_fd();

    if fd != LISTEN_FDS_START {
        // dup2 leaves FD_CLOEXEC clear on the new descriptor.
        if unsafe { libc::dup2(fd, LISTEN_FDS_START) } < 0 {
            bail!("dup2 onto fd 3 failed: {}", std::io::Error::last_os_error());
        }
        unsafe { libc::close(fd) };
    } else {
        // std opens sockets close-on-exec; the listener has to survive the exec.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } < 0 {
            bail!(
                "Failed to clear FD_CLOEXEC on fd 3: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    let (cmd, cmd_args) = command.split_first().context("No command specified")?;
    let err = Command::new(cmd)
        .args(cmd_args)
        .env("LISTEN_FDS", "1")
        .env("LISTEN_PID", std::process::id().to_string())
        .env("LISTEN_FDNAMES", "porrocket")
        .exec();
    cleanup_socket(socket_path);
    Err(err).with_context(|| format!("Failed to exec {cmd}"))
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate that we have a command to run
    if args.command.is_empty() {
        bail!("No command specified");
    }

    if args.listenfd {
        return exec_with_listenfd(&args.socket, &args.command);
    }
    let port = args.port.context("--port is required without --listenfd")?;

    // Get the hook library path
    let hook_lib = get_hook_library_path()
        .context("Failed to locate hook library")?;

    // Setup signal handlers for cleanup
    let term = Arc::new(AtomicBool::new(false));
    let socket_path_for_signal = args.socket.clone();

    flag::register(SIGTERM, Arc::clone(&term))?;
    flag::register(SIGINT, Arc::clone(&term))?;

    // Extract command and arguments
    let (cmd, cmd_args) = args.command.split_first().unwrap();

    // Prepare the command with environment variables
    let mut command = Command::new(cmd);
    command.args(cmd_args);

    // Set environment variables for the hook
    command.env("PORROCKET_PORT", port.to_string());
    command.env("PORROCKET_SOCKET", args.socket.to_str().unwrap_or(""));

    // Marker file so we can detect whether the hook actually loaded. On macOS,
    // dyld silently strips DYLD_INSERT_LIBRARIES for restricted binaries.
    let marker_path = env::temp_dir().join(format!("porrocket-loaded-{}", std::process::id()));
    let _ = fs::remove_file(&marker_path);
    command.env("PORROCKET_MARKER", &marker_path);

    // Inject the hook library. Linux uses LD_PRELOAD; macOS uses
    // DYLD_INSERT_LIBRARIES (do NOT force flat namespace — the hook relies on
    // dyld interposing, which is two-level-namespace safe).
    #[cfg(target_os = "linux")]
    command.env("LD_PRELOAD", hook_lib.to_str().unwrap());
    #[cfg(target_os = "macos")]
    command.env("DYLD_INSERT_LIBRARIES", hook_lib.to_str().unwrap());

    // Execute the command
    let status = command
        .status()
        .context("Failed to execute command")?;

    // Warn if the hook never loaded (marker file was never created).
    let hook_loaded = marker_path.exists();
    let _ = fs::remove_file(&marker_path);
    if !hook_loaded {
        eprintln!("[porrocket] WARNING: the hook library was never loaded.");
        #[cfg(target_os = "macos")]
        eprintln!(
            "[porrocket] On macOS this usually means DYLD_INSERT_LIBRARIES was stripped \
             because the target is a restricted binary (SIP-protected path, hardened \
             runtime with library validation, or setuid). Use a non-restricted binary, \
             e.g. a Homebrew/dev-installed interpreter."
        );
        #[cfg(target_os = "linux")]
        eprintln!("[porrocket] Check that the target uses standard libc socket APIs.");
    }

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
