use std::io;
use std::path::Path;
use std::process::Command;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::env;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use thiserror::Error;

#[cfg(target_os = "windows")]
use std::ffi::{OsStr, OsString};
#[cfg(target_os = "windows")]
use std::mem::size_of;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use std::ptr::null;
#[cfg(target_os = "windows")]
use tempfile::Builder;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE, WAIT_OBJECT_0};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

#[cfg(target_os = "windows")]
use crate::privileged::{PrivilegedResponse, read_privileged_response};

#[cfg(target_os = "macos")]
const ADMINISTRATOR_SCRIPT: &str = include_str!("../assets/macos/elevate.applescript");

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Debug, Error)]
pub enum ElevationError {
    #[error("L'authentification administrateur a été annulée.")]
    Cancelled,

    #[error("Impossible de préparer ou lancer l'opération administrateur : {0}")]
    Io(#[from] io::Error),

    #[error("L'opération administrateur a échoué : {0}")]
    Failed(String),

    #[error("La réponse de l'opération administrateur est invalide : {0}")]
    InvalidOutput(String),
}

pub fn open_path(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]);
        command.arg(path);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command.spawn().map(|_| ())
}

#[cfg(target_os = "macos")]
pub fn deploy_with_administrator_privileges(
    source: &Path,
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    run_with_administrator_privileges("deploy", Some(source), target_directory)
}

#[cfg(target_os = "macos")]
pub fn delete_with_administrator_privileges(
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    run_with_administrator_privileges("delete", None, target_directory)
}

#[cfg(target_os = "macos")]
pub fn restore_with_administrator_privileges(
    history_directory: &Path,
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    run_with_administrator_privileges("restore", Some(history_directory), target_directory)
}

#[cfg(target_os = "macos")]
fn run_with_administrator_privileges(
    operation: &str,
    source: Option<&Path>,
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    let executable = env::current_exe()?.canonicalize()?;
    let target_directory = target_directory.canonicalize()?;
    let source = source.map(Path::canonicalize).transpose()?;

    let output = Command::new("/usr/bin/osascript")
        .args(["-e", ADMINISTRATOR_SCRIPT])
        .env("BOUCHONNEUR_EXECUTABLE", executable.as_os_str())
        .env("BOUCHONNEUR_OPERATION", operation)
        .env(
            "BOUCHONNEUR_SOURCE",
            source.as_deref().unwrap_or_else(|| Path::new("")),
        )
        .env("BOUCHONNEUR_TARGET", target_directory.as_os_str())
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() {
        return stdout
            .parse()
            .map_err(|_| ElevationError::InvalidOutput(stdout));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.contains("(-128)") {
        Err(ElevationError::Cancelled)
    } else {
        Err(ElevationError::Failed(stderr))
    }
}

#[cfg(target_os = "windows")]
pub fn deploy_with_administrator_privileges(
    source: &Path,
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    run_with_administrator_privileges("deploy", Some(source), target_directory)
}

#[cfg(target_os = "windows")]
pub fn delete_with_administrator_privileges(
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    run_with_administrator_privileges("delete", None, target_directory)
}

#[cfg(target_os = "windows")]
pub fn restore_with_administrator_privileges(
    history_directory: &Path,
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    run_with_administrator_privileges("restore", Some(history_directory), target_directory)
}

#[cfg(target_os = "windows")]
fn run_with_administrator_privileges(
    operation: &str,
    source: Option<&Path>,
    target_directory: &Path,
) -> Result<usize, ElevationError> {
    let executable = env::current_exe()?;
    let target_directory = target_directory.canonicalize()?;
    let source = source.map(Path::canonicalize).transpose()?;
    let result_file = Builder::new()
        .prefix("bouchonneur-result-")
        .tempfile()?
        .into_temp_path();

    let mut arguments = Vec::new();
    arguments.push(OsString::from(format!("--privileged-{operation}")));
    if let Some(source) = &source {
        arguments.push(source.as_os_str().to_owned());
    }
    arguments.push(target_directory.as_os_str().to_owned());
    arguments.push(OsString::from("--result-file"));
    arguments.push(result_file.as_os_str().to_owned());

    let verb = wide_null(OsStr::new("runas"));
    let executable = wide_null(executable.as_os_str());
    let parameters = windows_command_line(&arguments);
    let mut shell_info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: executable.as_ptr(),
        lpParameters: parameters.as_ptr(),
        lpDirectory: null(),
        nShow: SW_HIDE,
        ..Default::default()
    };

    // SAFETY: all pointers reference null-terminated buffers that outlive the call and the
    // structure is initialized with the documented size. ShellExecuteExW writes only to it.
    if unsafe { ShellExecuteExW(&raw mut shell_info) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            return Err(ElevationError::Cancelled);
        }
        return Err(ElevationError::Io(error));
    }

    if shell_info.hProcess.is_null() {
        return Err(ElevationError::Failed(
            "Windows n'a pas fourni de processus administrateur.".to_owned(),
        ));
    }
    let process = ProcessHandle(shell_info.hProcess);

    // SAFETY: the handle belongs to the process returned by ShellExecuteExW and stays valid
    // until ProcessHandle closes it after the wait and exit-code query.
    let wait_result = unsafe { WaitForSingleObject(process.0, INFINITE) };
    if wait_result != WAIT_OBJECT_0 {
        return Err(ElevationError::Failed(format!(
            "Attente du processus administrateur impossible (code {wait_result})."
        )));
    }

    let mut exit_code = 0;
    // SAFETY: process.0 is a live process handle and exit_code points to writable memory.
    if unsafe { GetExitCodeProcess(process.0, &raw mut exit_code) } == 0 {
        return Err(ElevationError::Io(io::Error::last_os_error()));
    }

    let response = read_privileged_response(&result_file).map_err(ElevationError::InvalidOutput)?;
    match response {
        PrivilegedResponse::Success(affected_files) if exit_code == 0 => Ok(affected_files),
        PrivilegedResponse::Success(_) => Err(ElevationError::Failed(format!(
            "Le processus administrateur s'est terminé avec le code {exit_code}."
        ))),
        PrivilegedResponse::Error(message) => Err(ElevationError::Failed(message)),
    }
}

#[cfg(target_os = "windows")]
struct ProcessHandle(HANDLE);

#[cfg(target_os = "windows")]
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: this type is only constructed with the owned handle returned by
        // ShellExecuteExW and drops exactly once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(target_os = "windows")]
fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

#[cfg(target_os = "windows")]
fn windows_command_line(arguments: &[OsString]) -> Vec<u16> {
    let mut command_line = Vec::new();
    for (index, argument) in arguments.iter().enumerate() {
        if index > 0 {
            command_line.push(b' ' as u16);
        }
        append_quoted_windows_argument(&mut command_line, argument);
    }
    command_line.push(0);
    command_line
}

#[cfg(target_os = "windows")]
fn append_quoted_windows_argument(command_line: &mut Vec<u16>, argument: &OsStr) {
    command_line.push(b'"' as u16);
    let mut backslashes = 0;

    for unit in argument.encode_wide() {
        if unit == b'\\' as u16 {
            backslashes += 1;
            continue;
        }

        if unit == b'"' as u16 {
            command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
        } else {
            command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
        }
        backslashes = 0;
        command_line.push(unit);
    }

    command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    command_line.push(b'"' as u16);
}
