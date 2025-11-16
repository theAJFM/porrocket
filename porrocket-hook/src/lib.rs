use libc::{c_int, sockaddr, sockaddr_in, sockaddr_un, socklen_t, AF_INET, AF_UNIX};
use std::mem;
use std::ptr;
use std::sync::Once;

static INIT: Once = Once::new();
static mut TARGET_PORT: u16 = 0;
static mut SOCKET_PATH: [u8; 108] = [0; 108]; // Max path length for Unix socket

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
    if !addr.is_null() && (*addr).sa_family == AF_INET as u8 {
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

            // Convert to Unix socket bind
            let mut unix_addr: sockaddr_un = mem::zeroed();
            unix_addr.sun_family = AF_UNIX as u8;

            // Copy the path
            let socket_path_ptr = ptr::addr_of!(SOCKET_PATH) as *const u8;
            let path_len = (0..108)
                .find(|&i| unsafe { *socket_path_ptr.add(i) == 0 })
                .unwrap_or(107);
            ptr::copy_nonoverlapping(
                socket_path_ptr as *const i8,
                unix_addr.sun_path.as_mut_ptr(),
                path_len,
            );

            // Remove existing socket file if it exists
            let _ = libc::unlink(socket_path_ptr as *const i8);

            // Call original bind with Unix socket address
            let unix_addr_len = mem::size_of::<sockaddr_un>() as socklen_t;
            let original_bind = get_original_bind();
            return original_bind(
                sockfd,
                &unix_addr as *const sockaddr_un as *const sockaddr,
                unix_addr_len,
            );
        }
    }

    // Not our target port, use original bind
    let original_bind = get_original_bind();
    original_bind(sockfd, addr, addrlen)
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
