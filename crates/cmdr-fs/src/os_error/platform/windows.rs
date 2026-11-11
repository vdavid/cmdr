//! Windows: Win32 codes (`GetLastError`, which `std` reports as `raw_os_error`), and
//! the Winsock ones a socket hands back. Numbers from `winerror.h` and `winsock2.h`.
//!
//! The one class only Windows produces is `InUseByAnotherApp`: a sharing or lock
//! violation, another app holding the file open.

use super::super::OsErrorClass;

const WIN32: [(i32, OsErrorClass); 40] = [
    (2, OsErrorClass::NotFound),               // ERROR_FILE_NOT_FOUND
    (3, OsErrorClass::NotFound),               // ERROR_PATH_NOT_FOUND
    (5, OsErrorClass::AccessDenied),           // ERROR_ACCESS_DENIED
    (8, OsErrorClass::OutOfMemory),            // ERROR_NOT_ENOUGH_MEMORY
    (14, OsErrorClass::OutOfMemory),           // ERROR_OUTOFMEMORY
    (17, OsErrorClass::CrossDevice),           // ERROR_NOT_SAME_DEVICE
    (19, OsErrorClass::ReadOnlyFilesystem),    // ERROR_WRITE_PROTECT
    (21, OsErrorClass::NoDevice),              // ERROR_NOT_READY
    (23, OsErrorClass::Io),                    // ERROR_CRC
    (32, OsErrorClass::InUseByAnotherApp),     // ERROR_SHARING_VIOLATION
    (33, OsErrorClass::InUseByAnotherApp),     // ERROR_LOCK_VIOLATION
    (39, OsErrorClass::NoSpace),               // ERROR_HANDLE_DISK_FULL
    (50, OsErrorClass::Unsupported),           // ERROR_NOT_SUPPORTED
    (53, OsErrorClass::HostUnreachable),       // ERROR_BAD_NETPATH
    (64, OsErrorClass::ConnectionReset),       // ERROR_NETNAME_DELETED
    (80, OsErrorClass::AlreadyExists),         // ERROR_FILE_EXISTS
    (87, OsErrorClass::InvalidArgument),       // ERROR_INVALID_PARAMETER
    (109, OsErrorClass::BrokenPipe),           // ERROR_BROKEN_PIPE
    (112, OsErrorClass::NoSpace),              // ERROR_DISK_FULL
    (120, OsErrorClass::NotImplemented),       // ERROR_CALL_NOT_IMPLEMENTED
    (121, OsErrorClass::TimedOut),             // ERROR_SEM_TIMEOUT
    (123, OsErrorClass::InvalidArgument),      // ERROR_INVALID_NAME
    (145, OsErrorClass::NotEmpty),             // ERROR_DIR_NOT_EMPTY
    (183, OsErrorClass::AlreadyExists),        // ERROR_ALREADY_EXISTS
    (206, OsErrorClass::NameTooLong),          // ERROR_FILENAME_EXCED_RANGE
    (267, OsErrorClass::NotADirectory),        // ERROR_DIRECTORY
    (995, OsErrorClass::Canceled),             // ERROR_OPERATION_ABORTED
    (1117, OsErrorClass::Io),                  // ERROR_IO_DEVICE
    (1225, OsErrorClass::ConnectionRefused),   // ERROR_CONNECTION_REFUSED
    (1231, OsErrorClass::NetworkUnreachable),  // ERROR_NETWORK_UNREACHABLE
    (1232, OsErrorClass::HostUnreachable),     // ERROR_HOST_UNREACHABLE
    (1236, OsErrorClass::ConnectionAborted),   // ERROR_CONNECTION_ABORTED
    (10050, OsErrorClass::NetworkDown),        // WSAENETDOWN
    (10051, OsErrorClass::NetworkUnreachable), // WSAENETUNREACH
    (10053, OsErrorClass::ConnectionAborted),  // WSAECONNABORTED
    (10054, OsErrorClass::ConnectionReset),    // WSAECONNRESET
    (10057, OsErrorClass::NotConnected),       // WSAENOTCONN
    (10060, OsErrorClass::TimedOut),           // WSAETIMEDOUT
    (10061, OsErrorClass::ConnectionRefused),  // WSAECONNREFUSED
    (10065, OsErrorClass::HostUnreachable),    // WSAEHOSTUNREACH
];

pub(crate) const TABLES: &[&[(i32, OsErrorClass)]] = &[&WIN32];

pub(crate) fn class_of(code: i32) -> OsErrorClass {
    super::class_in(TABLES, code)
}

pub(crate) fn code_of(class: OsErrorClass) -> Option<i32> {
    super::code_in(TABLES, class)
}
