//! Minimal Win32 FFI (no external crates).
#![allow(non_snake_case, clippy::upper_case_acronyms)]

use std::sync::atomic::{AtomicBool, Ordering};

pub type HANDLE = *mut core::ffi::c_void;
pub type BOOL = i32;
pub type DWORD = u32;

pub const STD_INPUT_HANDLE: DWORD = -10i32 as DWORD;
pub const STD_OUTPUT_HANDLE: DWORD = -11i32 as DWORD;
pub const STD_ERROR_HANDLE: DWORD = -12i32 as DWORD;
pub const FILE_TYPE_DISK: DWORD = 1;
pub const FILE_TYPE_CHAR: DWORD = 2;
pub const FILE_TYPE_PIPE: DWORD = 3;
pub const ENABLE_VIRTUAL_TERMINAL_PROCESSING: DWORD = 0x0004;
pub const ENABLE_PROCESSED_OUTPUT: DWORD = 0x0001;
pub const PROCESS_TERMINATE: DWORD = 0x0001;
pub const PROCESS_SUSPEND_RESUME: DWORD = 0x0800;
pub const SYNCHRONIZE: DWORD = 0x0010_0000;
pub const WAIT_OBJECT_0: DWORD = 0;
pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct COORD {
    pub X: i16,
    pub Y: i16,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct SMALL_RECT {
    pub Left: i16,
    pub Top: i16,
    pub Right: i16,
    pub Bottom: i16,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CONSOLE_SCREEN_BUFFER_INFO {
    pub dwSize: COORD,
    pub dwCursorPosition: COORD,
    pub wAttributes: u16,
    pub srWindow: SMALL_RECT,
    pub dwMaximumWindowSize: COORD,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct FILETIME {
    pub lo: u32,
    pub hi: u32,
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetStdHandle(n: DWORD) -> HANDLE;
    pub fn GetConsoleMode(h: HANDLE, mode: *mut DWORD) -> BOOL;
    pub fn SetConsoleMode(h: HANDLE, mode: DWORD) -> BOOL;
    pub fn GetFileType(h: HANDLE) -> DWORD;
    pub fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(DWORD) -> BOOL>, add: BOOL) -> BOOL;
    pub fn GetConsoleScreenBufferInfo(h: HANDLE, info: *mut CONSOLE_SCREEN_BUFFER_INFO) -> BOOL;
    pub fn GetConsoleOutputCP() -> u32;
    pub fn SetConsoleOutputCP(cp: u32) -> BOOL;
    pub fn GetConsoleCP() -> u32;
    pub fn SetConsoleCP(cp: u32) -> BOOL;
    pub fn OpenProcess(access: DWORD, inherit: BOOL, pid: DWORD) -> HANDLE;
    pub fn TerminateProcess(h: HANDLE, code: u32) -> BOOL;
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn WaitForSingleObject(h: HANDLE, ms: DWORD) -> DWORD;
    pub fn GetProcessTimes(h: HANDLE, c: *mut FILETIME, e: *mut FILETIME, k: *mut FILETIME, u: *mut FILETIME) -> BOOL;
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn FlushConsoleInputBuffer(h: HANDLE) -> BOOL;
    pub fn GetConsoleWindow() -> HANDLE;
    pub fn SetConsoleTitleW(title: *const u16) -> BOOL;
    pub fn GetLastError() -> DWORD;
}

#[link(name = "ntdll")]
extern "system" {
    fn NtSuspendProcess(h: HANDLE) -> i32;
    fn NtResumeProcess(h: HANDLE) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn GetUserNameW(buf: *mut u16, len: *mut DWORD) -> BOOL;
}

pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn ctrl_handler(kind: DWORD) -> BOOL {
    match kind {
        0 | 1 => {
            INTERRUPTED.store(true, Ordering::SeqCst);
            1
        }
        _ => 0,
    }
}

pub fn install_ctrl_handler() {
    unsafe {
        SetConsoleCtrlHandler(Some(ctrl_handler), 1);
    }
}

/// Clear an inherited "ignore Ctrl+C" attribute (set when a parent started us with
/// CREATE_NEW_PROCESS_GROUP), so Ctrl+C reaches us and the children we start.
pub fn clear_ctrl_c_ignore() {
    unsafe {
        SetConsoleCtrlHandler(None, 0);
    }
}

pub fn take_interrupt() -> bool {
    INTERRUPTED.swap(false, Ordering::SeqCst)
}
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Whether this process is attached to a console at all (GetConsoleCP fails without one).
/// It isn't when a parent started us with DETACHED_PROCESS, e.g. an editor's or agent's
/// background task. Then every console program we start would get a new visible console
/// window, so run_external starts them with CREATE_NO_WINDOW instead (Cygwin does the
/// same job with a hidden console).
pub fn has_console() -> bool {
    static HAS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HAS.get_or_init(|| unsafe { GetConsoleCP() != 0 })
}

pub fn std_handle(n: DWORD) -> HANDLE {
    unsafe { GetStdHandle(n) }
}

pub fn is_console(n: DWORD) -> bool {
    let h = std_handle(n);
    if h.is_null() || h as isize == -1 {
        return false;
    }
    let mut m = 0;
    unsafe { GetConsoleMode(h, &mut m) != 0 }
}

pub fn file_type(n: DWORD) -> DWORD {
    let h = std_handle(n);
    if h.is_null() || h as isize == -1 {
        return 0;
    }
    unsafe { GetFileType(h) }
}

/// Console window size (columns, lines).
pub fn console_size() -> Option<(usize, usize)> {
    for n in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE, STD_INPUT_HANDLE] {
        let h = std_handle(n);
        let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
        if unsafe { GetConsoleScreenBufferInfo(h, &mut info) } != 0 {
            let cols = (info.srWindow.Right - info.srWindow.Left + 1).max(1) as usize;
            let lines = (info.srWindow.Bottom - info.srWindow.Top + 1).max(1) as usize;
            return Some((cols, lines));
        }
    }
    None
}

pub fn enable_vt_output() {
    for n in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        let h = std_handle(n);
        let mut m = 0;
        unsafe {
            if GetConsoleMode(h, &mut m) != 0 {
                SetConsoleMode(h, m | ENABLE_VIRTUAL_TERMINAL_PROCESSING | ENABLE_PROCESSED_OUTPUT);
            }
        }
    }
}

pub fn kill_pid(pid: u32, code: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = TerminateProcess(h, code) != 0;
        CloseHandle(h);
        ok
    }
}

pub fn suspend_pid(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = NtSuspendProcess(h) == 0;
        CloseHandle(h);
        ok
    }
}

pub fn resume_pid(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = NtResumeProcess(h) == 0;
        CloseHandle(h);
        ok
    }
}

pub fn pid_alive(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return false;
        }
        let r = WaitForSingleObject(h, 0);
        CloseHandle(h);
        r != WAIT_OBJECT_0
    }
}

fn ft_secs(f: &FILETIME) -> f64 {
    let v = ((f.hi as u64) << 32) | f.lo as u64;
    v as f64 / 10_000_000.0
}

/// (user, system) CPU seconds of another process (by handle).
pub fn handle_times(h: HANDLE) -> (f64, f64) {
    let mut c = FILETIME::default();
    let mut e = FILETIME::default();
    let mut k = FILETIME::default();
    let mut u = FILETIME::default();
    unsafe {
        if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0 {
            return (ft_secs(&u), ft_secs(&k));
        }
    }
    (0.0, 0.0)
}

/// Accumulated CPU time of child processes waited for by this process.
pub static CHILD_CPU_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static CHILD_SYS_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn add_child_times(u: f64, s: f64) {
    CHILD_CPU_US.fetch_add((u * 1e6) as u64, Ordering::SeqCst);
    CHILD_SYS_US.fetch_add((s * 1e6) as u64, Ordering::SeqCst);
}

/// (user, system) CPU seconds of this process plus waited-for children.
pub fn total_times() -> (f64, f64) {
    let (u, s) = process_times();
    (u + CHILD_CPU_US.load(Ordering::SeqCst) as f64 / 1e6, s + CHILD_SYS_US.load(Ordering::SeqCst) as f64 / 1e6)
}

/// (user, system) CPU seconds of this process.
pub fn process_times() -> (f64, f64) {
    let mut c = FILETIME::default();
    let mut e = FILETIME::default();
    let mut k = FILETIME::default();
    let mut u = FILETIME::default();
    unsafe {
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) != 0 {
            return (ft_secs(&u), ft_secs(&k));
        }
    }
    (0.0, 0.0)
}

pub fn user_name() -> String {
    if let Ok(u) = std::env::var("USERNAME") {
        return u;
    }
    let mut buf = [0u16; 257];
    let mut len = buf.len() as DWORD;
    unsafe {
        if GetUserNameW(buf.as_mut_ptr(), &mut len) != 0 && len > 0 {
            return String::from_utf16_lossy(&buf[..(len - 1) as usize]);
        }
    }
    "user".into()
}

pub fn host_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "localhost".into())
}

pub fn set_title(t: &str) {
    let w: Vec<u16> = t.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        SetConsoleTitleW(w.as_ptr());
    }
}

/// Wait until the console input handle has input or `ms` elapses. Returns true if input is ready.
pub fn wait_stdin(ms: u32) -> bool {
    let h = std_handle(STD_INPUT_HANDLE);
    unsafe { WaitForSingleObject(h, ms) == WAIT_OBJECT_0 }
}

pub struct ConsoleCp {
    out: u32,
    inp: u32,
}

pub fn set_utf8_console() -> Option<ConsoleCp> {
    unsafe {
        let out = GetConsoleOutputCP();
        let inp = GetConsoleCP();
        if out == 0 {
            return None;
        }
        SetConsoleOutputCP(65001);
        SetConsoleCP(65001);
        Some(ConsoleCp { out, inp })
    }
}

impl Drop for ConsoleCp {
    fn drop(&mut self) {
        unsafe {
            SetConsoleOutputCP(self.out);
            SetConsoleCP(self.inp);
        }
    }
}
