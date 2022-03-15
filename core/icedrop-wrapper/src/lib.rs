#![feature(fn_traits)]
#![allow(dead_code)]

mod client;

#[cfg(test)]
mod tests;

use std::ffi::{c_void, CStr};
use std::mem::forget;
use std::os::raw::c_char;

use client::{IcedropClient, SendFileRequest, UserInfoPtr};

/// Creates and returns a new [`IcedropClient`] instance. Must be destroyed
/// via [`icedrop_client_destroy`] function after usage.
#[no_mangle]
pub extern "C" fn icedrop_client_new() -> *mut c_void {
    let client = Box::new(IcedropClient::new());
    let client_ptr = Box::leak(client) as *mut IcedropClient;
    unsafe { std::mem::transmute(client_ptr) }
}

/// Destroys the given [`IcedropClient`] instance. Must called when the client is not running, it's
/// always recommended to call this function in the same thread that runs the client.
#[no_mangle]
pub extern "C" fn icedrop_client_destroy(client: *mut c_void) {
    let client_ptr = client as *mut IcedropClient;
    let client = unsafe { Box::from_raw(client_ptr) };
    drop(client);
}

/// Runs the client's main loop in caller thread. This function will block until the client is asked
/// to stop via [`icedrop_client_stop`] function.
#[no_mangle]
pub extern "C" fn icedrop_client_run_in_current_thread(client: *mut c_void) {
    let client_ptr = client as *mut IcedropClient;
    let mut client = unsafe { Box::from_raw(client_ptr) };
    client.run_in_current_thread();
    forget(client);
}

/// Forces the client to stop running.
///
/// Note that the client can still run before this function returns.
#[no_mangle]
pub extern "C" fn icedrop_client_stop(client: *mut c_void) {}

/// Initiate an send file request.
#[no_mangle]
pub extern "C" fn icedrop_client_send_file(
    client: *mut c_void,
    remote_addr: *const c_char,
    local_file_path: *const c_char,
    user_info: *mut c_void,
    segment_sent_callback: Option<unsafe extern "C" fn(*mut c_void, u32, usize) -> c_void>,
    completed_callback: Option<unsafe extern "C" fn(*mut c_void, bool) -> c_void>,
) {
    let client_ptr = client as *mut IcedropClient;
    let client = unsafe { Box::from_raw(client_ptr) };

    unsafe {
        let remote_addr = CStr::from_ptr(remote_addr).to_str().unwrap();
        let local_file_path = CStr::from_ptr(local_file_path).to_str().unwrap();

        let maybe_send_file_req = SendFileRequest::new(remote_addr, local_file_path);
        if maybe_send_file_req.is_err() {
            if let Some(completed_callback) = completed_callback {
                completed_callback(user_info, false);
            }
            return;
        }

        let mut send_file_req = maybe_send_file_req.unwrap();

        send_file_req.user_info = UserInfoPtr(user_info);
        if let Some(segment_sent_callback) = segment_sent_callback {
            send_file_req.segment_sent_callback = Some(Box::new(move |arg_0, arg_1, arg_2| {
                segment_sent_callback(arg_0, arg_1, arg_2);
            }));
        }
        if let Some(completed_callback) = completed_callback {
            send_file_req.completed_callback = Some(Box::new(move |arg_0| {
                completed_callback(arg_0, true);
            }));
        }

        client.send_request(send_file_req);
    }

    forget(client);
}

/// Initiate an send file request with an opened file descriptor.
#[no_mangle]
pub extern "C" fn icedrop_client_send_file_with_fd(
    client: *mut c_void,
    remote_addr: *const c_char,
    file_fd: i32,
    user_info: *mut c_void,
    segment_sent_callback: Option<unsafe extern "C" fn(*mut c_void, u32, usize) -> c_void>,
    completed_callback: Option<unsafe extern "C" fn(*mut c_void, bool) -> c_void>,
) {
    let client_ptr = client as *mut IcedropClient;
    let client = unsafe { Box::from_raw(client_ptr) };

    unsafe {
        let remote_addr = CStr::from_ptr(remote_addr).to_str().unwrap();

        let mut send_file_req = SendFileRequest::with_fd(remote_addr, file_fd);

        send_file_req.user_info = UserInfoPtr(user_info);
        if let Some(segment_sent_callback) = segment_sent_callback {
            send_file_req.segment_sent_callback = Some(Box::new(move |arg_0, arg_1, arg_2| {
                segment_sent_callback(arg_0, arg_1, arg_2);
            }));
        }
        if let Some(completed_callback) = completed_callback {
            send_file_req.completed_callback = Some(Box::new(move |arg_0| {
                completed_callback(arg_0, true);
            }));
        }

        client.send_request(send_file_req);
    }

    forget(client);
}
