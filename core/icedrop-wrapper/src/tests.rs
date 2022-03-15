use std::ffi::CString;

use super::{icedrop_client_new, icedrop_client_run_in_current_thread, icedrop_client_send_file};

#[derive(Clone, Copy)]
struct AnySendable<T>(T);

unsafe impl<T> Send for AnySendable<T> {}

#[test]
fn simple_test() {
    let client = AnySendable(icedrop_client_new());

    std::thread::spawn(move || {
        let remote_addr = CString::new("127.0.0.1:8080").unwrap();
        let local_file_path =
            CString::new("/Users/cyandev/Downloads/Detroit Become Human.mp4").unwrap();
        icedrop_client_send_file(
            client.0,
            remote_addr.as_ptr(),
            local_file_path.as_ptr(),
            std::ptr::null_mut(),
            None,
            None,
        );
    });

    icedrop_client_run_in_current_thread(client.0);
}
