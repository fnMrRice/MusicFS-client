use super::{menu::ITEMS, wide, CLASS};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::ptr::null;
use std::sync::OnceLock;
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
    System::Threading::{
        CreateMutexW, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::{Shell::SetCurrentProcessExplicitAppUserModelID, WindowsAndMessaging::*},
};

pub(super) const FLAG: &str = "--musicfs-task";

pub(super) struct Identity {
    pub app: String,
    pub token: Uuid,
    _slot: OwnedHandle,
}

fn path_key(path: &Path) -> String {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let text = absolute.to_string_lossy().replace('/', "\\").to_lowercase();
    if let Some(network) = text.strip_prefix(r"\\?\unc\") {
        format!(r"\\{network}")
    } else {
        text.trim_start_matches(r"\\?\").to_owned()
    }
}

fn slot_prefix() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let config = std::env::var_os("MUSICFS_DESKTOP_CONFIG_DIR")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            directories::ProjectDirs::from("io.github", "MusicFS", "MusicFS Desktop")
                .map(|dir| dir.config_dir().to_owned())
        })
        .ok_or("Could not identify the MusicFS configuration directory")?;
    // 明确定义散列算法，使安装路径/配置的标识跨版本、跨重启保持一致。
    let bytes = format!("{}\0{}", path_key(&exe), path_key(&config));
    let hash = bytes.bytes().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    Ok(format!("MusicFS.Desktop.{hash:016x}"))
}

pub(super) fn identity() -> Result<&'static Identity, String> {
    static ID: OnceLock<Result<Identity, String>> = OnceLock::new();
    ID.get_or_init(|| {
        let prefix = slot_prefix()?;
        for slot in 1..=1024 {
            let app = if slot == 1 {
                prefix.clone()
            } else {
                format!("{prefix}.{slot}")
            };
            let name = wide(&format!("Local\\{app}"));
            unsafe {
                let mutex = CreateMutexW(null(), 0, name.as_ptr());
                if mutex.is_null() {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let taken = GetLastError() == ERROR_ALREADY_EXISTS;
                let handle = OwnedHandle::from_raw_handle(mutex);
                if !taken {
                    let hr = SetCurrentProcessExplicitAppUserModelID(wide(&app).as_ptr());
                    if hr < 0 {
                        return Err(format!("Could not set the taskbar application ID: {hr:#x}"));
                    }
                    return Ok(Identity {
                        app,
                        token: Uuid::new_v4(),
                        _slot: handle,
                    });
                }
            }
        }
        Err("No free MusicFS taskbar instance slot".into())
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub(super) fn title(token: Uuid) -> String {
    format!("MusicFS.Task.{token}")
}

fn target(args: &[OsString]) -> Option<(Uuid, usize)> {
    let [flag, token, verb] = args else {
        return None;
    };
    if flag != FLAG {
        return None;
    }
    let token = Uuid::parse_str(token.to_str()?).ok()?;
    let verb = verb.to_str()?;
    let index = ITEMS.iter().position(|(_, name, _)| *name == verb)?;
    Some((token, index + 1))
}

fn same_executable(pid: u32) -> bool {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let _process = OwnedHandle::from_raw_handle(process);
        let mut path = vec![0u16; 32768];
        let mut size = path.len() as u32;
        if QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size) == 0 {
            return false;
        }
        let target = PathBuf::from(OsString::from_wide(&path[..size as usize]));
        std::env::current_exe().is_ok_and(|exe| path_key(&exe) == path_key(&target))
    }
}

pub(super) fn relay(token: Uuid, id: usize) -> bool {
    let name = wide(&title(token));
    unsafe {
        let hwnd = FindWindowW(wide(CLASS).as_ptr(), name.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if !same_executable(pid) {
            return false;
        }
        let msg = RegisterWindowMessageW(name.as_ptr());
        if msg == 0 {
            return false;
        }
        AllowSetForegroundWindow(pid);
        let mut accepted = 0;
        SendMessageTimeoutW(
            hwnd,
            msg,
            id,
            0,
            SMTO_ABORTIFHUNG | SMTO_BLOCK | SMTO_ERRORONEXIT,
            1800,
            &mut accepted,
        ) != 0
            && accepted == 1
    }
}

pub(crate) fn handle_activation() -> bool {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|flag| flag == FLAG) {
        if let Some((token, id)) = target(&args) {
            relay(token, id);
        }
        // 过期链接、无效命令以及已经退出的实例均不能启动第二个播放器。
        true
    } else {
        // 主窗口创建之前设置进程标识，确保 Windows 正确分组首次启动的窗口。
        let _ = identity();
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }
    #[test]
    fn jump_list_accepts_only_existing_verbs_and_valid_instance_ids() {
        let token = Uuid::new_v4();
        let value = token.to_string();
        assert_eq!(target(&args(&[FLAG, &value, "toggle"])), Some((token, 3)));
        assert!(target(&args(&[FLAG, "none", "quit"])).is_none());
        assert!(target(&args(&[FLAG, &value, "unrecognized"])).is_none());
        assert!(target(&args(&[FLAG, &value, "quit", "extra"])).is_none());
        assert!(!relay(token, 1));
    }
}
