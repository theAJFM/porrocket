use libc::{c_int, sockaddr, sockaddr_in, sockaddr_un, socklen_t, AF_INET, AF_UNIX, SOCK_STREAM};
use std::collections::HashSet;
use std::mem;
use std::ptr;
use std::sync::{Mutex, Once};

// SO_DOMAIN is not in libc crate, define it manually
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
    });
}

// Original bind function pointer type
type BindFn = unsafe extern "C" fn(c_int, *const sockaddr, socklen_t) -> c_int;

// Get the original bind function
unsafe fn get_original_bind() -> BindFn {
    let bind_symbol = b"bind\0".as_ptr() as *const i8;
    let original = libc::dlsym(libc::RTLD_NEXT, bind_symbol);
    if original.is_null() {
        panic!("Failed to load original bind");
    }
    mem::transmute(original)
}

/// Our bind() replacement
#[no_mangle]
pub unsafe extern "C" fn bind(sockfd: c_int, addr: *const sockaddr, addrlen: socklen_t) -> c_int {
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

            // Copy the path
            let socket_path_ptr = ptr::addr_of!(SOCKET_PATH) as *const u8;
            let path_len = (0..108)
                .find(|&i| *socket_path_ptr.add(i) == 0)
                .unwrap_or(107);
            ptr::copy_nonoverlapping(
                socket_path_ptr as *const i8,
                unix_addr.sun_path.as_mut_ptr(),
                path_len,
            );

            // Remove existing socket file if it exists
            let _ = libc::unlink(socket_path_ptr as *const i8);

            // Bind to Unix socket
            let unix_addr_len = mem::size_of::<sockaddr_un>() as socklen_t;
            let original_bind = get_original_bind();
            let result = original_bind(
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
    let original_bind = get_original_bind();
    original_bind(sockfd, addr, addrlen)
}

/// Intercept getsockname to return fake TCP address for converted sockets
#[no_mangle]
pub unsafe extern "C" fn getsockname(
    sockfd: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> c_int {
    initialize();

    // Get the original function
    let getsockname_symbol = b"getsockname\0".as_ptr() as *const i8;
    let original = libc::dlsym(libc::RTLD_NEXT, getsockname_symbol);
    if original.is_null() {
        return -1;
    }
    let original_getsockname: unsafe extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int =
        mem::transmute(original);

    // If this is a converted socket, return fake TCP info
    if is_converted_socket(sockfd) {
        if !addr.is_null() && !addrlen.is_null() {
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
    }

    // Not a converted socket, use original
    original_getsockname(sockfd, addr, addrlen)
}

/// Intercept getsockopt to return fake TCP socket options
#[no_mangle]
pub unsafe extern "C" fn getsockopt(
    sockfd: c_int,
    level: c_int,
    optname: c_int,
    optval: *mut libc::c_void,
    optlen: *mut socklen_t,
) -> c_int {
    initialize();

    // Get the original function
    let getsockopt_symbol = b"getsockopt\0".as_ptr() as *const i8;
    let original = libc::dlsym(libc::RTLD_NEXT, getsockopt_symbol);
    if original.is_null() {
        return -1;
    }
    let original_getsockopt: unsafe extern "C" fn(
        c_int,
        c_int,
        c_int,
        *mut libc::c_void,
        *mut socklen_t,
    ) -> c_int = mem::transmute(original);

    // If this is a converted socket and asking for SO_DOMAIN, return AF_INET
    if is_converted_socket(sockfd) && level == libc::SOL_SOCKET && optname == SO_DOMAIN {
        if !optval.is_null() && !optlen.is_null() {
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
    }

    // For other options, use original
    original_getsockopt(sockfd, level, optname, optval, optlen)
}

/// Intercept getpeername to return fake peer address for converted sockets
#[no_mangle]
pub unsafe extern "C" fn getpeername(
    sockfd: c_int,
    addr: *mut sockaddr,
    addrlen: *mut socklen_t,
) -> c_int {
    initialize();

    // Get the original function
    let getpeername_symbol = b"getpeername\0".as_ptr() as *const i8;
    let original = libc::dlsym(libc::RTLD_NEXT, getpeername_symbol);
    if original.is_null() {
        return -1;
    }
    let original_getpeername: unsafe extern "C" fn(c_int, *mut sockaddr, *mut socklen_t) -> c_int =
        mem::transmute(original);

    // If this is a converted socket, return fake TCP peer info
    if is_converted_socket(sockfd) {
        if !addr.is_null() && !addrlen.is_null() {
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
    }

    // Not a converted socket, use original
    original_getpeername(sockfd, addr, addrlen)
}

/// Intercept close to stop tracking socket
#[no_mangle]
pub unsafe extern "C" fn close(fd: c_int) -> c_int {
    // Untrack if it was a converted socket
    untrack_converted_socket(fd);

    // Call original close
    let close_symbol = b"close\0".as_ptr() as *const i8;
    let original = libc::dlsym(libc::RTLD_NEXT, close_symbol);
    if original.is_null() {
        return -1;
    }
    let original_close: unsafe extern "C" fn(c_int) -> c_int = mem::transmute(original);
    original_close(fd)
}

// Constructor to run when library is loaded (Linux only)
#[cfg(target_os = "linux")]
#[link_section = ".init_array"]
#[used]
pub static INITIALIZE_CTOR: extern "C" fn() = init_hook;

#[no_mangle]
pub extern "C" fn init_hook() {
    unsafe {
        initialize();
    }
}
