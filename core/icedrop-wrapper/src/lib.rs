use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::sync::Mutex;
use env_logger::{self, Env};
use icedrop_core::{SenderController, SenderCommand};

struct SenderControllerHandle {
  inner: Mutex<SenderController>,
  endpoint: String
}

#[no_mangle]
pub extern "C" fn icedrop_logger_init() {
  env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
}

/// Creates and returns a new `SenderController` instance. Must be destroyed
/// via `icedrop_sender_controller_destroy` function after usage.
#[no_mangle]
pub extern "C" fn icedrop_sender_controller_new(endpoint: *const c_char) -> *mut c_void {
  let endpoint_str = unsafe { CStr::from_ptr(endpoint).to_string_lossy().into_owned() };

  let sender_controller = SenderController::new();
  let handle = SenderControllerHandle {
    inner: Mutex::new(sender_controller),
    endpoint: endpoint_str
  };
  let handle_ref = Box::leak(Box::new(handle));
  return handle_ref as *mut _ as *mut c_void;
}

/// Destroys the given `SenderController` instance.
#[no_mangle]
pub extern "C" fn icedrop_sender_controller_destroy(ptr: *mut c_void) {
  let _ptr = ptr as *mut SenderControllerHandle;
  let handle = unsafe { Box::from_raw(_ptr) };
  drop(handle);
}

/// Enqueues a send file command with given file path.
#[no_mangle]
pub extern "C" fn icedrop_sender_controller_send_file(ptr: *mut c_void, path: *const c_char) {
  let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };

  let _ptr = unsafe { &*(ptr as *mut SenderControllerHandle) };
  let controller_guard = _ptr.inner.lock().unwrap();
  controller_guard.dispatch_command(SenderCommand::SendFile(path_str));
}

/// Enters the main loop of a sender thread. Calling this function will block
/// the current thread until the sender is closed by commands.
#[no_mangle]
pub extern "C" fn icedrop_sender_thread_main(ptr: *mut c_void) {
  let handle_ref = unsafe { &*(ptr as *mut SenderControllerHandle) };
  let mut controller_guard = handle_ref.inner.lock().unwrap();
  let rx = controller_guard.take_command_rx();
  drop(controller_guard);

  let endpoint = handle_ref.endpoint.clone();

  icedrop_core::sender_main(endpoint, rx);
}