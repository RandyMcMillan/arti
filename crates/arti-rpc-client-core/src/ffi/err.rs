//! Error handling logic for our ffi code.

use c_str_macro::c_str;
use paste::paste;
use std::cell::RefCell;
use std::ffi::{c_char, CStr};
use std::fmt::Display;
use std::panic::{catch_unwind, UnwindSafe};

use crate::conn::ErrorResponse;
use crate::util::Utf8CStr;

use super::ArtiStatus;

/// Helper:
/// Given a restricted enum defining FfiStatus, also define a series of constants for its variants,
/// and a string conversion function.

// NOTE: I tried to use derive_deftly here, but ran into trouble when defining the constants.
// I wanted to have them be "pub const ARTI_FOO = FfiStatus::$vname",
// but that doesn't work with cbindgen, which won't expose a constant unless it is a public type
// it can recognize.
// There is no way to use derive_deftly to look at the explicit discriminant of an enum.
macro_rules! define_ffi_status {
    {
        $(#[$tm:meta])*
        pub(crate) enum FfiStatus {
            $(
                $(#[$m:meta])*
                [$s:expr]
                $id:ident = $e:expr,
            )+
        }

    } => {paste!{
        $(#[$tm])*
        pub(crate) enum FfiStatus {
            $(
                $(#[$m])*
                $id = $e,
            )+
        }

        $(
            $(#[$m])*
            pub const [<ARTI_ $id:snake:upper >] : ArtiStatus = $e;
        )+

        /// Return a string representing the meaning of a given `arti_status_t`.
        ///
        /// The result will always be non-NULL, even if the status is unrecognized.
        #[no_mangle]
        pub extern "C" fn arti_status_to_str(status: ArtiStatus) -> *const c_char {
            match status {
                $(
                    [<ARTI_ $id:snake:upper>] => c_str!($s),
                )+
                _ => c_str!("(unrecognized status)"),
            }.as_ptr()
        }
    }}
}

define_ffi_status! {
/// View of FFI status as rust enumeration.
///
/// Not exposed in the FFI interfaces, except via cast to ArtiStatus.
///
/// We define this as an enumeration so that we can treat it exhaustively in Rust.
#[derive(Copy, Clone, Debug)]
#[repr(u32)]
pub(crate) enum FfiStatus {
    /// The function has returned successfully.
    ["Success"]
    Success = 0,

    /// One or more of the inputs to the function was invalid.
    ["Invalid input"]
    InvalidInput = 1,

    /// Tried to use some functionality (for example, an authentication method or connection scheme)
    /// that wasn't available on this platform or build.
    ["Not supported"]
    NotSupported = 2,

    /// Tried to connect to Arti, but an IO error occurred.
    ["An IO error ocurred while connecting to Arti"]
    ConnectIo = 3,

    /// We tried to authenticate with Arti, but it rejected our attempt.
    ["Authenticationrejected"]
    BadAuth = 4,

    /// Our peer has, in some way, violated the Arti-RPC protocol.
    ["Peer violated he RPC protocol"]
    PeerProtocolViolation = 5,

    /// The peer has closed our connection; possibly because it is shutting down.
    ["Peer has shut own"]
    Shutdown = 6,

    /// An internal error occurred in the arti rpc client.
    ["Internal error possible bug?"]
    Internal = 7,

    /// The peer reports that one of our requests has failed.
    ["Request has failed"]
    RequestFailed = 8,

    /// Tried to check the status of a request and found that it was no longer running.
    ///
    /// TODO RPC: We should make sure that this is the actual semantics we want for this
    /// error!  Revisit after we have implemented real cancellation.
    ["Request was cancelled"]
    RequestCancelled = 9,
}
}

/// An error as returned by the Arti FFI code.
#[derive(Debug, Clone)]
pub struct FfiError {
    /// The status of this error messages
    pub(super) status: ArtiStatus,
    /// A human-readable message describing this error
    message: Utf8CStr,
    /// If present, a Json-formatted message from our peer that we are representing with this error.
    error_response: Option<ErrorResponse>,
}

impl FfiError {
    /// Helper: If this error stems from a resoponse from our RPC peer,
    /// return that reponse.
    fn error_response_as_cstr(&self) -> Option<&CStr> {
        self.error_response
            .as_ref()
            .map(|response| response.as_ref())
    }
}

/// Convenience trait to help implement `Into<FfiError>`
///
/// Any error that implements this trait will be convertible into an [`FfiError`].
// additional requirements: display doesn't make NULs.
pub(crate) trait IntoFfiError: Display + Sized {
    /// Return the status
    fn status(&self) -> FfiStatus;
    /// Return a message for this error.
    ///
    /// By default, returns the Display of this error.
    fn message(&self) -> String {
        self.to_string()
    }
    /// Consume this error and return an [`ErrorResponse`]
    fn into_error_response(self) -> Option<ErrorResponse> {
        None
    }
}
impl<T: IntoFfiError> From<T> for FfiError {
    fn from(value: T) -> Self {
        let status = value.status() as u32;
        let message = value
            .message()
            .try_into()
            .expect("Error message had a NUL?");
        let error_response = value.into_error_response();
        Self {
            status,
            message,
            error_response,
        }
    }
}

/// Tried to call a ffi function with a not-permitted null pointer argument.
#[derive(Clone, Debug, thiserror::Error)]
#[error("One of the arguments was NULL")]
pub(super) struct NullPointer;

impl IntoFfiError for NullPointer {
    fn status(&self) -> FfiStatus {
        FfiStatus::InvalidInput
    }
}

impl IntoFfiError for crate::ConnectError {
    fn status(&self) -> FfiStatus {
        use crate::ConnectError as E;
        use FfiStatus as F;
        match self {
            E::SchemeNotSupported => F::NotSupported,
            E::CannotConnect(_) => F::ConnectIo,
            E::AuthenticationRejected(_) => F::BadAuth,
            E::BadMessage(_) => F::PeerProtocolViolation,
            E::ProtoError(e) => e.status(),
        }
    }

    fn into_error_response(self) -> Option<ErrorResponse> {
        use crate::ConnectError as E;
        match self {
            E::AuthenticationRejected(msg) => Some(msg),
            _ => None,
        }
    }
}

impl IntoFfiError for crate::ProtoError {
    fn status(&self) -> FfiStatus {
        use crate::ProtoError as E;
        use FfiStatus as F;
        match self {
            E::Shutdown(_) => F::Shutdown,
            E::InvalidRequest(_) => F::InvalidInput,
            E::RequestIdInUse => F::InvalidInput,
            E::RequestCancelled => F::RequestCancelled,
            E::DuplicateWait => F::Internal,
            E::CouldNotEncode(_) => F::Internal,
        }
    }
}

impl IntoFfiError for crate::BuilderError {
    fn status(&self) -> FfiStatus {
        use crate::BuilderError as E;
        use FfiStatus as F;
        match self {
            E::InvalidConnectString => F::InvalidInput,
        }
    }
}

impl IntoFfiError for ErrorResponse {
    fn status(&self) -> FfiStatus {
        FfiStatus::RequestFailed
    }
    fn into_error_response(self) -> Option<ErrorResponse> {
        Some(self)
    }
}

// TODO RPC: Decide whether to eliminate LAST_ERR?
//
// Reasonable people point out that it might be better just to give every failure-capable function
// an out-param that can hold an error.
//
// This sounds a bit onerous to me, but the saving grace is that we expect basically nobody
// to call the C APIs directly: nearly everybody will wrap them in some other language with
// a real exception or error handling convention.
thread_local! {
    /// Thread-local: last error to occur in this thread.
    static LAST_ERR: RefCell<FfiError> = RefCell::new(FfiError {
        message: "(no error has occurred)".to_owned().try_into().expect("Error message couldn't become a CString?"),
        status: FfiStatus::Success as u32,
        error_response: None
    });
}

/// Helper: replace the last error with `e`.
pub(super) fn set_last_error(e: FfiError) {
    LAST_ERR.with(|cell| *cell.borrow_mut() = e);
}

/// An error returned by the Arti RPC code, exposed as an object.
///
/// After a function has returned an [`ArtiStatus`] other than [`ARTI_STATUS_SUCCESS`],
/// you can use [`arti_err_clone`]`(NULL)` to get a copy of the most recent error.
///
/// Functions that return information about an error will either take a pointer
/// to one of these objects, or NULL to indicate the most error in a given thread.
pub type ArtiError = FfiError;

/// Return the status code associated with a given error.
///
/// If `err` is NULL, instead return the status code from the most recent error to occur in this
/// thread.
///
/// # Safety
///
/// The provided pointer, if non-NULL, must be a valid `ArtiError`.
#[no_mangle]
pub unsafe extern "C" fn arti_err_status(err: *const ArtiError) -> ArtiStatus {
    catch_panic(
        || {
            if err.is_null() {
                LAST_ERR.with(|e| e.borrow().status)
            } else {
                // Safety: we require that `err` is a valid pointer of the proper type.
                unsafe { (*err).status }
            }
        },
        || ARTI_INTERNAL,
    )
}

/// Return a human-readable error message associated with a given error.
///
/// If `err` is NULL, instead return the error message from the most recent error to occur in this
/// thread.
///
/// The format of these messages may change arbitrarily between versions of this library;
/// it is a mistake to depend on the actual contents of this message.
///
/// # Safety
///
/// The returned pointer is only as valid for as long as `err` is valid.
///
/// If `err` is NULL, then the returned pointer is only valid until another
/// error occurs in this thread.
#[no_mangle]
pub unsafe extern "C" fn arti_err_message(err: *const ArtiError) -> *const c_char {
    catch_panic(
        || {
            if err.is_null() {
                // Note: "as_ptr()" allows the `message` part of `e` to escape this borrow().
                // This is safe so long as nothing mutates LAST_ERR while it is borrowed,
                // which is what we have required in our documentation.
                LAST_ERR.with(|e| e.borrow().message.as_ptr())
            } else {
                // Safety: We require that `err` is a valid pointer of the proper type.
                unsafe { (*err).message.as_ptr() }
            }
        },
        || c_str!("internal error (panic)").as_ptr(),
    )
}

/// Return a Json-formatted error response associated with a given error.
///
/// If `err` is NULL, instead return the response from the most recent error to occur in this
/// thread.
///
/// These messages are full responses, including the `error` field,
/// and the `id` field (if present).
///
/// Return NULL if the specified error does not represent an RPC error response.
///
/// # Safety
///
/// The returned pointer is only as valid for as long as `err` is valid.
///
/// If `err` is NULL, then the returned pointer is only valid until another
/// error occurs in this thread.
#[no_mangle]
pub unsafe extern "C" fn arti_err_response(err: *const ArtiError) -> *const c_char {
    catch_panic(
        || {
            if err.is_null() {
                // Note: "as_ptr()" allows the `error_response` part of `e` to escape this borrow().
                // This is safe so long as nothing mutates LAST_ERR while it is borrowed,
                // which is what we have required in our documentation.
                LAST_ERR
                    .with(|e| {
                        e.borrow()
                            .error_response_as_cstr()
                            .map(|cstr| cstr.as_ptr())
                    })
                    .unwrap_or(std::ptr::null())
            } else {
                // Safety: We require that `err` is a valid pointer of the proper type.
                unsafe { (*err).error_response_as_cstr() }
                    .map(|cstr| cstr.as_ptr())
                    .unwrap_or(std::ptr::null())
            }
        },
        std::ptr::null,
    )
}

/// Make and return copy of a provided error.
///
/// If `err` is NULL, instead return a copy of the most recent error to occur in this thread.
///
/// May return NULL if an internal error occurs.
///
/// # Safety
///
/// The resulting error may only be freed via `arti_err_free().`
#[no_mangle]
pub unsafe extern "C" fn arti_err_clone(err: *const ArtiError) -> *mut ArtiError {
    catch_panic(
        || {
            let cloned = if err.is_null() {
                LAST_ERR.with(|e| e.borrow().clone())
            } else {
                // Safety: We require that `err` is a valid pointer of the proper type.
                unsafe { (*err).clone() }
            };

            // Note: arti_err_free will later call Box::from_raw on this pointer.
            Box::into_raw(Box::new(cloned))
        },
        std::ptr::null_mut,
    )
}

/// Release storage held by a provided error.
///
/// # Safety
///
/// The provided pointer must be returned by `arti_err_clone`.
/// After this call, it may not longer be used.
#[no_mangle]
pub unsafe extern "C" fn arti_err_free(err: *mut ArtiError) {
    catch_panic(
        || {
            if err.is_null() {
                return;
            }
            // Safety: We require that the pointer came from `arti_err_clone`,
            // which returns a pointer that came from Box::into_raw().
            let err = unsafe { Box::from_raw(err) };
            drop(err);
        },
        || {},
    );
}

/// Run `body` and catch panics.  If one occurs, return the result of `on_err` instead.
pub(super) fn catch_panic<F, G, T>(body: F, on_err: G) -> T
where
    F: FnOnce() -> T + UnwindSafe,
    G: FnOnce() -> T,
{
    match catch_unwind(body) {
        Ok(x) => x,
        Err(_panic_info) => on_err(),
    }
}

/// Call `body`, converting any errors or panics that occur into an FfiError,
/// and storing that error as LAST_ERR.
pub(super) fn handle_errors<F>(body: F) -> ArtiStatus
where
    F: FnOnce() -> Result<(), FfiError> + UnwindSafe,
{
    match catch_unwind(body) {
        Ok(Ok(())) => ARTI_SUCCESS,
        Ok(Err(e)) => {
            // "body" returned an error.
            let status = e.status;
            set_last_error(e);
            status
        }
        Err(_panic_data) => {
            // "body" panicked.  Unfortunately, there is not a great way to get this
            // panic info to be exposed.
            let e = FfiError {
                status: ARTI_INTERNAL,
                message: "Internal panic in library code"
                    .to_string()
                    .try_into()
                    .expect("couldn't make a valid C string"),
                error_response: None,
            };
            set_last_error(e);
            ARTI_INTERNAL
        }
    }
}
