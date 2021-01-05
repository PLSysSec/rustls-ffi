#![crate_type = "staticlib"]
use libc::{c_char, size_t};
use std::slice;
use std::{
    cmp::min,
    io::{Cursor, Read, Write},
};
use std::{ffi::CStr, sync::Arc};
use std::{io::ErrorKind::ConnectionAborted, mem};

use rustls::{ClientConfig, ClientSession, Session};

mod error;
use error::{map_error, rustls_result};
use rustls_result::NullParameter;

// We use the opaque struct pattern to tell C about our types without
// telling them what's inside.
// https://doc.rust-lang.org/nomicon/ffi.html#representing-opaque-structs
#[allow(non_camel_case_types)]
pub struct rustls_client_config {
    _private: [u8; 0],
}
#[allow(non_camel_case_types)]
pub struct rustls_client_session {
    _private: [u8; 0],
}

// Keep in sync with Cargo.toml.
const RUSTLS_CRATE_VERSION: &str = "0.19.0";

/// Write the version of the crustls C bindings and rustls itself into the
/// provided buffer, up to a max of `len` bytes. Output is UTF-8 encoded
/// and NUL terminated. Returns the number of bytes written before the NUL.
#[no_mangle]
pub extern "C" fn rustls_version(buf: *mut c_char, len: size_t) -> size_t {
    let write_buf: &mut [u8] = unsafe {
        if buf.is_null() {
            return 0;
        }
        slice::from_raw_parts_mut(buf as *mut u8, len as usize)
    };
    let version: String = format!(
        "crustls/{}/rustls/{}",
        env!("CARGO_PKG_VERSION"),
        RUSTLS_CRATE_VERSION,
    );
    let version: &[u8] = version.as_bytes();
    let len: usize = min(write_buf.len() - 1, version.len());
    write_buf[..len].copy_from_slice(&version[..len]);
    write_buf[len] = 0;
    len
}

/// Create a client_config. Caller owns the memory and must free it with
/// rustls_client_config_free.
#[no_mangle]
pub extern "C" fn rustls_client_config_new() -> *const rustls_client_config {
    let mut config = rustls::ClientConfig::new();
    config
        .root_store
        .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);
    env_logger::init();
    Arc::into_raw(Arc::new(config)) as *const _
}

/// "Free" a client_config previously returned from rustls_client_config_new.
/// Since client_config is actually an atomically reference-counted pointer,
/// extant client_sessions may still hold an internal reference to the
/// Rust object. However, C code must consider this pointer unusable after
/// "free"ing it.
/// Calling with NULL is fine. Must not be called twice with the same value.
#[no_mangle]
pub extern "C" fn rustls_client_config_free(config: *const rustls_client_config) {
    unsafe {
        if let Some(c) = (config as *const ClientConfig).as_ref() {
            // To free the client_config, we reconstruct the Arc. It should have a refcount of 1,
            // representing the C code's copy. When it drops, that refcount will go down to 0
            // and the inner ClientConfig will be dropped.
            let arc: Arc<ClientConfig> = Arc::from_raw(c);
            let strong_count = Arc::strong_count(&arc);
            if strong_count < 1 {
                eprintln!(
                    "rustls_client_config_free: invariant failed: arc.strong_count was < 1: {}. \
                    You must not free the same client_config multiple times.",
                    strong_count
                );
            }
        } else {
            eprintln!("rustls_client_config_free: config was NULL");
        }
    };
}

/// In rustls_client_config_new, we create an Arc, then call `into_raw` and return the resulting raw
/// pointer to C. C can then call rustls_client_session_new multiple times using that same raw
/// pointer. On each call, we need to reconstruct the Arc. But once we reconstruct the Arc, its
/// reference count will be decremented on drop. We need to reference count to stay at 1, because
/// the C code is holding a copy. This function turns the raw pointer back into an Arc, clones it
/// to increment the reference count (which will make it 2 in this particular case), and
/// mem::forgets the clone. The mem::forget prevents the reference count from being decremented when
/// we exit this function, so it will stay at 2 as long as we are in Rust code. Once the caller
/// drops its Arc, the reference count will go back down to 1, indicating the C code's copy.
///
/// Unsafety:
///
/// v must be a non-null pointer that resulted from previously calling `Arc::into_raw`.
unsafe fn arc_with_incref_from_raw<T>(v: *const T) -> Arc<T> {
    let r = Arc::from_raw(v);
    let val = Arc::clone(&r);
    mem::forget(r);
    val
}

/// Create a new rustls::ClientSession, and return it in the output parameter `out`.
/// If this returns an error code, the memory pointed to by `session_out` remains unchanged.
/// If this returns a non-error, the memory pointed to by `session_out` is modified to point
/// at a valid ClientSession. The caller now owns the ClientSession and must call
/// `rustls_client_session_free` when done with it.
#[no_mangle]
pub extern "C" fn rustls_client_session_new(
    config: *const rustls_client_config,
    hostname: *const c_char,
    session_out: *mut *mut rustls_client_session,
) -> rustls_result {
    let hostname: &CStr = unsafe {
        if hostname.is_null() {
            return NullParameter;
        }
        CStr::from_ptr(hostname)
    };
    let config: Arc<ClientConfig> = unsafe {
        match (config as *const ClientConfig).as_ref() {
            Some(c) => arc_with_incref_from_raw(c),
            None => return NullParameter,
        }
    };
    let hostname: &str = match hostname.to_str() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("converting hostname to Rust &str: {}", e);
            return rustls_result::Io;
        }
    };
    let name_ref = match webpki::DNSNameRef::try_from_ascii_str(hostname) {
        Ok(nr) => nr,
        Err(e) => {
            eprintln!(
                "turning hostname '{}' into webpki::DNSNameRef: {}",
                hostname, e
            );
            return rustls_result::Io;
        }
    };
    let client = ClientSession::new(&config, name_ref);

    // We've succeeded. Put the client on the heap, and transfer ownership
    // to the caller. After this point, we must return CRUSTLS_OK so the
    // caller knows it is responsible for this memory.
    let b = Box::new(client);
    unsafe {
        *session_out = Box::into_raw(b) as *mut _;
    }

    return rustls_result::Ok;
}

#[no_mangle]
pub extern "C" fn rustls_client_session_wants_read(session: *const rustls_client_session) -> bool {
    unsafe {
        match (session as *const ClientSession).as_ref() {
            Some(cs) => cs.wants_read(),
            None => false,
        }
    }
}

#[no_mangle]
pub extern "C" fn rustls_client_session_wants_write(session: *const rustls_client_session) -> bool {
    unsafe {
        match (session as *const ClientSession).as_ref() {
            Some(cs) => cs.wants_write(),
            None => false,
        }
    }
}

#[no_mangle]
pub extern "C" fn rustls_client_session_is_handshaking(
    session: *const rustls_client_session,
) -> bool {
    unsafe {
        match (session as *const ClientSession).as_ref() {
            Some(cs) => cs.is_handshaking(),
            None => false,
        }
    }
}

#[no_mangle]
pub extern "C" fn rustls_client_session_process_new_packets(
    session: *mut rustls_client_session,
) -> rustls_result {
    let session: &mut ClientSession = unsafe {
        match (session as *mut ClientSession).as_mut() {
            Some(cs) => cs,
            None => return NullParameter,
        }
    };
    match session.process_new_packets() {
        Ok(()) => rustls_result::Ok,
        Err(e) => return map_error(e),
    }
}

/// Free a client_session previously returned from rustls_client_session_new.
/// Calling with NULL is fine. Must not be called twice with the same value.
#[no_mangle]
pub extern "C" fn rustls_client_session_free(session: *mut rustls_client_session) {
    unsafe {
        if let Some(c) = (session as *mut ClientSession).as_mut() {
            // Convert the pointer to a Box and drop it.
            Box::from_raw(c);
        } else {
            eprintln!("warning: rustls_client_config_free: config was NULL");
        }
    }
}

/// Write up to `count` plaintext bytes from `buf` into the ClientSession.
/// This will increase the number of output bytes available to
/// `rustls_client_session_write_tls`.
/// On success, store the number of bytes actually written in *out_n
/// (this may be less than `count`).
/// https://docs.rs/rustls/0.19.0/rustls/struct.ClientSession.html#method.write
#[no_mangle]
pub extern "C" fn rustls_client_session_write(
    session: *const rustls_client_session,
    buf: *const u8,
    count: size_t,
    out_n: *mut size_t,
) -> rustls_result {
    let session: &mut ClientSession = unsafe {
        match (session as *mut ClientSession).as_mut() {
            Some(cs) => cs,
            None => return NullParameter,
        }
    };
    let write_buf: &[u8] = unsafe {
        if buf.is_null() {
            return NullParameter;
        }
        slice::from_raw_parts(buf, count as usize)
    };
    let out_n: &mut size_t = unsafe {
        match out_n.as_mut() {
            Some(out_n) => out_n,
            None => return NullParameter,
        }
    };
    let n_written: usize = match session.write(write_buf) {
        Ok(n) => n,
        Err(_) => return rustls_result::Io,
    };
    *out_n = n_written;
    rustls_result::Ok
}

/// Read up to `count` plaintext bytes from the ClientSession into `buf`.
/// On success, store the number of bytes read in *out_n (this may be less
/// than `count`). A success with *out_n set to 0 means "all bytes currently
/// available have been read, but more bytes may become available after
/// subsequent calls to rustls_client_session_read_tls and
/// rustls_client_session_process_new_packets."
/// https://docs.rs/rustls/0.19.0/rustls/struct.ClientSession.html#method.read
#[no_mangle]
pub extern "C" fn rustls_client_session_read(
    session: *const rustls_client_session,
    buf: *mut u8,
    count: size_t,
    out_n: *mut size_t,
) -> rustls_result {
    let session: &mut ClientSession = unsafe {
        match (session as *mut ClientSession).as_mut() {
            Some(cs) => cs,
            None => return NullParameter,
        }
    };
    let read_buf: &mut [u8] = unsafe {
        if buf.is_null() {
            return NullParameter;
        }
        slice::from_raw_parts_mut(buf, count as usize)
    };
    let out_n = unsafe {
        match out_n.as_mut() {
            Some(out_n) => out_n,
            None => return NullParameter,
        }
    };
    // Since it's *possible* for a Read impl to consume the possibly-uninitialized memory from buf,
    // zero it out just in case. TODO: use Initializer once it's stabilized.
    // https://doc.rust-lang.org/nightly/std/io/trait.Read.html#method.initializer
    for c in read_buf.iter_mut() {
        *c = 0;
    }
    let n_read: usize = match session.read(read_buf) {
        Ok(n) => n,
        // The CloseNotify TLS alert is benign, but rustls returns it as an Error. See comment on
        // https://docs.rs/rustls/0.19.0/rustls/struct.ClientSession.html#impl-Read.
        // Log it and return EOF.
        Err(e) if e.kind() == ConnectionAborted && e.to_string().contains("CloseNotify") => {
            *out_n = 0;
            return rustls_result::Ok;
        }
        Err(_) => return rustls_result::Io,
    };
    *out_n = n_read;
    rustls_result::Ok
}

/// Read up to `count` TLS bytes from `buf` (usually read from a socket) into
/// the ClientSession. This may make packets available to
/// `rustls_client_session_process_new_packets`, which in turn may make more
/// bytes available to `rustls_client_session_read`.
/// On success, store the number of bytes actually read in *out_n (this may
/// be less than `count`). This function returns success and stores 0 in
/// *out_n when the input count is 0.
/// https://docs.rs/rustls/0.19.0/rustls/trait.Session.html#tymethod.read_tls
#[no_mangle]
pub extern "C" fn rustls_client_session_read_tls(
    session: *const rustls_client_session,
    buf: *const u8,
    count: size_t,
    out_n: *mut size_t,
) -> rustls_result {
    let session: &mut ClientSession = unsafe {
        match (session as *mut ClientSession).as_mut() {
            Some(cs) => cs,
            None => return NullParameter,
        }
    };
    let input_buf: &[u8] = unsafe {
        if buf.is_null() {
            return NullParameter;
        }
        slice::from_raw_parts(buf, count as usize)
    };
    let out_n = unsafe {
        match out_n.as_mut() {
            Some(out_n) => out_n,
            None => return NullParameter,
        }
    };
    let mut cursor = Cursor::new(input_buf);
    let n_read: usize = match session.read_tls(&mut cursor) {
        Ok(n) => n,
        Err(_) => return rustls_result::Io,
    };
    *out_n = n_read;
    rustls_result::Ok
}

/// Write up to `count` TLS bytes from the ClientSession into `buf`. Those
/// bytes should then be written to a socket. On success, store the number of
/// bytes actually written in *out_n (this maybe less than `count`).
/// https://docs.rs/rustls/0.19.0/rustls/trait.Session.html#tymethod.write_tls
#[no_mangle]
pub extern "C" fn rustls_client_session_write_tls(
    session: *const rustls_client_session,
    buf: *mut u8,
    count: size_t,
    out_n: *mut size_t,
) -> rustls_result {
    let session: &mut ClientSession = unsafe {
        match (session as *mut ClientSession).as_mut() {
            Some(cs) => cs,
            None => return NullParameter,
        }
    };
    let mut output_buf: &mut [u8] = unsafe {
        if buf.is_null() {
            return NullParameter;
        }
        slice::from_raw_parts_mut(buf, count as usize)
    };
    let out_n = unsafe {
        match out_n.as_mut() {
            Some(out_n) => out_n,
            None => return NullParameter,
        }
    };
    let n_written: usize = match session.write_tls(&mut output_buf) {
        Ok(n) => n,
        Err(_) => return rustls_result::Io,
    };
    *out_n = n_written;
    rustls_result::Ok
}
