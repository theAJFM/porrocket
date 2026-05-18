use libc::{c_int, sockaddr, sockaddr_in, sockaddr_un, socklen_t, AF_INET, AF_UNIX, SOCK_STREAM};
use std::collections::HashSet;
use std::mem;
use std::ptr;
use std::sync::{Mutex, Once};

// SO_DOMAIN is a Linux-only socket option (not in the libc crate, define it
// manually). macOS has no equivalent, so the getsockopt hook is Linux-only.
#[cfg(target_os = "linux")]
const SO_DOMAIN: c_int = 39;

static INIT: Once = Once::new();
static mut TARGET_PORT: u16 = 0;
static mut SOCKET_PATH: [u8; 108] = [0; 108]; // Max path length for Unix socket

// Track which socket FDs we've converted to Unix sockets
static CONVERTED_SOCKETS: Mutex<Option<HashSet<c_int>>> = Mutex::new(None);

fn track_converted_socket(fd: c_int) {
    let mut guard = CONVERTED_SOCKETS.lock().unwrap();
    if guard.is_none() {
        *guard = Some(HashSet::new());
    }
    guard.as_mut().unwrap().insert(fd);
}

fn is_converted_socket(fd: c_int) -> bool {
    let guard = CONVERTED_SOCKETS.lock().unwrap();
    guard.as_ref().map_or(false, |set| set.contains(&fd))
}

#[cfg(target_os = "linux")] // only the Linux close() hook untracks sockets
fn untrack_converted_socket(fd: c_int) {
    let mut guard = CONVERTED_SOCKETS.lock().unwrap();
    if let Some(set) = guard.as_mut() {
        set.remove(&fd);
    }
}

/// Initialize by reading environment variables
unsafe fn initialize() {
    INIT.call_once(|| {
        // Debug output
        libc::write(2, b"[porrocket] Initializing hook\n".as_ptr() as *const _, 30);

        // Read target port from environment
        if let Ok(port_str) = std::env::var("PORROCKET_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                TARGET_PORT = port;
                let msg = format!("[porrocket] Target port: {}\n", port);
                libc::write(2, msg.as_ptr() as *const _, msg.len());
            }
        }

        // Read socket path from environment
        if let Ok(path) = std::env::var("PORROCKET_SOCKET") {
            let bytes = path.as_bytes();
            let len = bytes.len().min(107); // Reserve 1 byte for null terminator
            SOCKET_PATH[..len].copy_from_slice(&bytes[..len]);
            SOCKET_PATH[len] = 0; // Null terminate
            let msg = format!("[porrocket] Socket path: {}\n", path);
            libc::write(2, msg.as_ptr() as *const _, msg.len());
        }

        // Drop a marker file so the porrocket launcher can detect whether the
        // hook was actually loaded into the target (on macOS, dyld silently
        // strips DYLD_INSERT_LIBRARIES for restricted/SIP-protected binaries).
        if let Ok(marker) = std::env::var("PORROCKET_MARKER") {
            let _ = std::fs::write(&marker, b"loaded");
        }
    });
}

// ---------------------------------------------------------------------------
// Original-function resolution
//
// Linux: the hook shadows libc symbols via LD_PRELOAD, so the genuine
// implementations are fetched with dlsym(RTLD_NEXT, ...). dlsym yields a raw
// pointer, so calling it skips the shadowed symbol.
//
// macOS: the hook is wired up through the dyld __interpose table. dyld does
// NOT interpose references made *from within the interposing image itself*
// (this is the documented DYLD_INTERPOSE contract), so a direct call to
// `libc::bind` here reaches the real libc. dlsym must NOT be used on macOS:
// dlsym(RTLD_NEXT, ...) returns the *interposed* symbol and would recurse.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
unsafe fn orig_bind(fd: c_int, addr: *const sockaddr, len: socklen_t) -> c_int {
    let sym = libc::dlsym(libc::RTLD_NEXT, b"bind\0".as_ptr() as *const _);
    if sym.is_null() {
        return -1;
    }
    let f: unsafe extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int = mem::transmute(sym);
    f(fd, addr, len)
}

#[cfg(target_os = "macos")]
unsafe fn orig_bind(fd: c_int, addr: *const sockaddr, len: socklen_t) -> c_int {
    libc::bind(fd, addr, len)
}

#[cfg(target_os = "linux")]
unsafe fn orig_getsockname(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> c_int {
    let sym = libc::dlsym(libc::RTLD_NEXT, b"getsockname\0".as_ptr() as *const _);
    if sym.is_null() {
        return -1;
    }
    let f: unsafe extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int = mem::transmute(sym);
    f(fd, addr, len)
}

#[cfg(target_os = "macos")]
unsafe fn orig_getsockname(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> c_int {
    libc::getsockname(fd, addr, len)
}

#[cfg(target_os = "linux")]
unsafe fn orig_getpeername(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> c_int {
    let sym = libc::dlsym(libc::RTLD_NEXT, b"getpeername\0".as_ptr() as *const _);
    if sym.is_null() {
        return -1;
    }
    let f: unsafe extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int = mem::transmute(sym);
    f(fd, addr, len)
}

#[cfg(target_os = "macos")]
unsafe fn orig_getpeername(fd: c_int, addr: *mut sockaddr, len: *mut socklen_t) -> c_int {
    libc::getpeername(fd, addr, len)
}

#[cfg(target_os = "linux")]
unsafe fn orig_close(fd: c_int) -> c_int {
    let sym = libc::dlsym(libc::RTLD_NEXT, b"close\0".as_ptr() as *const _);
    if sym.is_null() {
        return -1;
    }
    let f: unsafe extern "C" fn(c_int) -> c_int = mem::transmute(sym);
    f(fd)
}

// ---------------------------------------------------------------------------
// Shared hook implementations (platform-independent logic)
// ---------------------------------------------------------------------------

/// bind() hook: redirect a TCP bind on the target port to a Unix socket.
unsafe fn hook_bind(sockfd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int {
    initialize();

    libc::write(2, b"[porrocket] bind() intercepted\n".as_ptr() as *const _, 31);

    // Check if this is an IPv4 bind
    if !addr.is_null() && (*addr).sa_family == (AF_INET as u8).into() {
        let addr_in = addr as *const sockaddr_in;
        let port = u16::from_be((*addr_in).sin_port);

        let msg = format!("[porrocket] IPv4 bind on port {}\n", port);
        libc::write(2, msg.as_ptr() as *const _, msg.len());

        // Check if this matches our target port
        if port == TARGET_PORT && TARGET_PORT != 0 && SOCKET_PATH[0] != 0 {
            libc::write(
                2,
                b"[porrocket] Redirecting to Unix socket\n".as_ptr() as *const _,
                39,
            );

            // Create a new Unix domain socket
            let new_sockfd = libc::socket(AF_UNIX, SOCK_STREAM, 0);
            if new_sockfd < 0 {
                libc::write(
                    2,
                    b"[porrocket] Failed to create Unix socket\n".as_ptr() as *const _,
                    41,
                );
                return -1;
            }

            // Duplicate the new socket onto the old file descriptor
            if libc::dup2(new_sockfd, sockfd) < 0 {
                libc::write(
                    2,
                    b"[porrocket] Failed to dup2 socket\n".as_ptr() as *const _,
                    34,
                );
                libc::close(new_sockfd);
                return -1;
            }

            // Close the temporary socket fd
            libc::close(new_sockfd);

            // Track this socket as converted
            track_converted_socket(sockfd);

            // Create Unix socket address
            let mut unix_addr: sockaddr_un = mem::zeroed();
            unix_addr.sun_family = (AF_UNIX as u8).into();

            // Copy the path (clamp to the platform's sun_path size: 108 bytes
            // on Linux, 104 on macOS; reserve 1 byte for the null terminator).
            let socket_path_ptr = ptr::addr_of!(SOCKET_PATH) as *const u8;
            let max_path = unix_addr.sun_path.len() - 1;
            let path_len = (0..max_path)
                .find(|&i| *socket_path_ptr.add(i) == 0)
                .unwrap_or(max_path);
            ptr::copy_nonoverlapping(
                socket_path_ptr as *const _,
                unix_addr.sun_path.as_mut_ptr(),
                path_len,
            );

            // macOS sockaddr_un carries a length byte; Linux's does not.
            #[cfg(target_os = "macos")]
            {
                unix_addr.sun_len = mem::size_of::<sockaddr_un>() as u8;
            }

            // Remove existing socket file if it exists
            let _ = libc::unlink(socket_path_ptr as *const _);

            // Bind to Unix socket
            let unix_addr_len = mem::size_of::<sockaddr_un>() as socklen_t;
            let result = orig_bind(
                sockfd,
                &unix_addr as *const sockaddr_un as *const sockaddr,
                unix_addr_len,
            );

            if result == 0 {
                libc::write(
                    2,
                    b"[porrocket] Successfully bound to Unix socket\n".as_ptr() as *const _,
                    46,
                );
            } else {
                libc::write(
                    2,
                    b"[porrocket] Failed to bind to Unix socket\n".as_ptr() as *const _,
                    42,
                );
            }

            return result;
        }
    }

    // Not our target port, use original bind
    orig_bind(sockfd, addr, addrlen)
}

/// getsockname() hook: return fake TCP address info for converted sockets.
unsafe fn hook_getsockname(sockfd: c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> c_int {
    initialize();

    if is_converted_socket(sockfd) && !addr.is_null() && !addrlen.is_null() {
        let fake_addr = addr as *mut sockaddr_in;
        ptr::write_bytes(fake_addr, 0, 1);
        (*fake_addr).sin_family = (AF_INET as u8).into();
        (*fake_addr).sin_port = TARGET_PORT.to_be();
        (*fake_addr).sin_addr.s_addr = 0; // 0.0.0.0
        *addrlen = mem::size_of::<sockaddr_in>() as socklen_t;

        libc::write(
            2,
            b"[porrocket] getsockname() returning fake TCP info\n".as_ptr() as *const _,
            50,
        );
        return 0;
    }

    orig_getsockname(sockfd, addr, addrlen)
}

/// getpeername() hook: return fake TCP peer info for converted sockets.
unsafe fn hook_getpeername(sockfd: c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> c_int {
    initialize();

    if is_converted_socket(sockfd) && !addr.is_null() && !addrlen.is_null() {
        let fake_addr = addr as *mut sockaddr_in;
        ptr::write_bytes(fake_addr, 0, 1);
        (*fake_addr).sin_family = (AF_INET as u8).into();
        (*fake_addr).sin_port = 0;
        (*fake_addr).sin_addr.s_addr = libc::htonl(libc::INADDR_LOOPBACK);
        *addrlen = mem::size_of::<sockaddr_in>() as socklen_t;

        libc::write(
            2,
            b"[porrocket] getpeername() returning fake TCP peer\n".as_ptr() as *const _,
            50,
        );
        return 0;
    }

    orig_getpeername(sockfd, addr, addrlen)
}

/// close() hook: stop tracking a converted socket when it is closed.
#[cfg(target_os = "linux")]
unsafe fn hook_close(fd: c_int) -> c_int {
    untrack_converted_socket(fd);
    orig_close(fd)
}

// ---------------------------------------------------------------------------
// Linux: export the hooks under the real libc symbol names. LD_PRELOAD makes
// these shadow libc, and the getsockopt hook masks the Unix socket's domain.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn bind(sockfd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int {
    hook_bind(sockfd, addr, addrlen)
}

#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn getsockname(
    sockfd: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> c_int {
    hook_getsockname(sockfd, addr, addrlen)
}

#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn getpeername(
    sockfd: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> c_int {
    hook_getpeername(sockfd, addr, addrlen)
}

#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    hook_close(fd)
}

/// getsockopt() hook (Linux only): report AF_INET as the socket domain so that
/// applications validating SO_DOMAIN don't notice the Unix socket swap.
#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn getsockopt(
    sockfd: c_int,
    level: c_int,
    optname: c_int,
    optval: *mut libc::c_void,
    optlen: *mut socklen_t,
) -> c_int {
    initialize();

    let sym = libc::dlsym(libc::RTLD_NEXT, b"getsockopt\0".as_ptr() as *const _);
    if sym.is_null() {
        return -1;
    }
    let original: unsafe extern "C" fn(
        c_int,
        c_int,
        c_int,
        *mut libc::c_void,
        *mut socklen_t,
    ) -> c_int = mem::transmute(sym);

    if is_converted_socket(sockfd)
        && level == libc::SOL_SOCKET
        && optname == SO_DOMAIN
        && !optval.is_null()
        && !optlen.is_null()
    {
        let domain_ptr = optval as *mut c_int;
        *domain_ptr = AF_INET;
        *optlen = mem::size_of::<c_int>() as socklen_t;

        libc::write(
            2,
            b"[porrocket] getsockopt(SO_DOMAIN) returning AF_INET\n".as_ptr() as *const _,
            52,
        );
        return 0;
    }

    original(sockfd, level, optname, optval, optlen)
}

// ---------------------------------------------------------------------------
// macOS: two-level namespaces make plain symbol shadowing ineffective. Instead
// the hooks are wired up through the dyld __interpose table — an array of
// {replacement, original} pointer pairs that dyld rewrites at load time.
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
unsafe extern "C" fn porrocket_bind(
    sockfd: c_int,
    addr: *const sockaddr,
    addrlen: socklen_t,
) -> c_int {
    hook_bind(sockfd, addr, addrlen)
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn porrocket_getsockname(
    sockfd: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> c_int {
    hook_getsockname(sockfd, addr, addrlen)
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn porrocket_getpeername(
    sockfd: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> c_int {
    hook_getpeername(sockfd, addr, addrlen)
}

// NOTE: close() is intentionally NOT interposed on macOS. It is called
// constantly and very early in process startup; interposing it risks
// re-entrancy during dyld/libsystem initialization. Converted sockets are
// long-lived listeners, so skipping close-tracking is harmless here.

#[cfg(target_os = "macos")]
#[repr(C)]
struct Interpose {
    replacement: *const (),
    original: *const (),
}

#[cfg(target_os = "macos")]
unsafe impl Sync for Interpose {}

#[cfg(target_os = "macos")]
#[used]
#[link_section = "__DATA,__interpose"]
static INTERPOSE_BIND: Interpose = Interpose {
    replacement: porrocket_bind as *const (),
    original: libc::bind as *const (),
};

#[cfg(target_os = "macos")]
#[used]
#[link_section = "__DATA,__interpose"]
static INTERPOSE_GETSOCKNAME: Interpose = Interpose {
    replacement: porrocket_getsockname as *const (),
    original: libc::getsockname as *const (),
};

#[cfg(target_os = "macos")]
#[used]
#[link_section = "__DATA,__interpose"]
static INTERPOSE_GETPEERNAME: Interpose = Interpose {
    replacement: porrocket_getpeername as *const (),
    original: libc::getpeername as *const (),
};

// ---------------------------------------------------------------------------
// Constructor: runs when the library is loaded, before the target's main().
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[link_section = ".init_array"]
#[used]
pub static INITIALIZE_CTOR: extern "C" fn() = init_hook;

#[cfg(target_os = "macos")]
#[link_section = "__DATA,__mod_init_func"]
#[used]
pub static INITIALIZE_CTOR: extern "C" fn() = init_hook;

#[no_mangle]
pub extern "C" fn init_hook() {
    unsafe {
        initialize();
    }
}
