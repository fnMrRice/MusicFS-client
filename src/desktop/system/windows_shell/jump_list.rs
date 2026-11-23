use super::{
    activation::{Identity, FLAG},
    menu::ITEMS,
};
use crate::i18n::tr;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;
use uuid::Uuid;
use windows::{
    core::{Interface, Result, HSTRING, PROPVARIANT},
    Win32::{
        Foundation::HWND,
        Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_Title},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{
            Common::{IObjectArray, IObjectCollection},
            DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW,
            PropertiesSystem::{
                IPropertyStore, PSCoerceToCanonicalValue, SHGetPropertyStoreForWindow, PROPERTYKEY,
            },
            ShellLink,
        },
    },
};

pub(super) struct JumpList(Sender<Vec<String>>);

impl JumpList {
    pub(super) fn new(main: isize, identity: &'static Identity) -> std::io::Result<Self> {
        let exe = std::env::current_exe()?;
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("MusicFS taskbar".into())
            .spawn(move || unsafe {
                if let Err(error) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
                    eprintln!("MusicFS taskbar initialization failed: {error}");
                    return;
                }
                worker(main, identity, &exe, rx);
                CoUninitialize();
            })?;
        let result = Self(tx);
        result.refresh();
        Ok(result)
    }

    pub fn refresh(&self) {
        let _ = self
            .0
            .send(ITEMS.iter().map(|(_, _, key)| tr(key).to_owned()).collect());
    }
}

unsafe fn worker(main: isize, identity: &Identity, exe: &Path, rx: Receiver<Vec<String>>) {
    let Ok(mut labels) = rx.recv() else {
        return;
    };
    let mut retry = 1;
    loop {
        // Explorer 恢复或切换语言时只发布最新文本；Shell COM 调用不阻塞音频和主窗口。
        for next in rx.try_iter() {
            labels = next;
        }
        match publish(main, identity, exe, &labels) {
            Ok(()) => {
                retry = 1;
                match rx.recv() {
                    Ok(next) => labels = next,
                    Err(_) => return,
                }
            }
            Err(error) => {
                if retry == 1 {
                    eprintln!("MusicFS taskbar update failed: {error}");
                }
                match rx.recv_timeout(Duration::from_secs(retry)) {
                    Ok(next) => labels = next,
                    Err(RecvTimeoutError::Disconnected) => return,
                    Err(RecvTimeoutError::Timeout) => retry = (retry * 2).min(60),
                }
            }
        }
    }
}

unsafe fn property(store: &IPropertyStore, key: &PROPERTYKEY, text: &str) -> Result<()> {
    let mut value = PROPVARIANT::from(text);
    PSCoerceToCanonicalValue(key, &mut value)?;
    store.SetValue(key, &value)
}

unsafe fn link(exe: &Path, verb: &str, title: &str, token: Uuid) -> Result<IShellLinkW> {
    let item: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
    let path = HSTRING::from(exe.as_os_str());
    item.SetPath(&path)?;
    item.SetArguments(&HSTRING::from(format!("{FLAG} {token} {verb}")))?;
    item.SetIconLocation(&path, 0)?;
    let store: IPropertyStore = item.cast()?;
    property(&store, &PKEY_Title, title)?;
    store.Commit()?;
    Ok(item)
}

unsafe fn publish(main: isize, identity: &Identity, exe: &Path, labels: &[String]) -> Result<()> {
    let window: IPropertyStore = SHGetPropertyStoreForWindow(HWND(main as _))?;
    property(&window, &PKEY_AppUserModel_ID, &identity.app)?;
    // 窗口属性库直接写入，不能调用该属性库未实现的 Commit。
    commit_tasks(&identity.app, identity.token, exe, labels)
}

unsafe fn commit_tasks(app: &str, token: Uuid, exe: &Path, labels: &[String]) -> Result<()> {
    let list: ICustomDestinationList =
        CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
    list.SetAppID(&HSTRING::from(app))?;
    let mut slots = 0;
    let _removed: IObjectArray = list.BeginList(&mut slots)?;
    let result = (|| {
        let tasks: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
        for ((_, verb, _), title) in ITEMS.iter().zip(labels) {
            tasks.AddObject(&link(exe, verb, title, token)?)?;
        }
        list.AddUserTasks(&tasks.cast::<IObjectArray>()?)?;
        list.CommitList()
    })();
    if result.is_err() {
        let _ = list.AbortList();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links_carry_the_translated_title_and_exact_instance_command() {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let token = Uuid::new_v4();
            let exe = std::env::current_exe().unwrap();
            for (_, verb, key) in ITEMS {
                let title = tr(key);
                let item = link(&exe, verb, title, token).unwrap();
                let mut args = [0u16; 256];
                item.GetArguments(&mut args).unwrap();
                let count = args.iter().position(|&c| c == 0).unwrap();
                assert_eq!(
                    String::from_utf16(&args[..count]).unwrap(),
                    format!("{FLAG} {token} {verb}")
                );
                let props: IPropertyStore = item.cast().unwrap();
                assert_eq!(props.GetValue(&PKEY_Title).unwrap().to_string(), title);
            }
            CoUninitialize();
        }
    }

    #[test]
    #[ignore = "publishes and removes a private test Jump List in Windows Explorer"]
    fn explorer_accepts_tasks_for_a_private_application_id() {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let token = Uuid::new_v4();
            let app = format!("MusicFS.Test.{}", token.simple());
            let labels: Vec<_> = ITEMS.iter().map(|(_, _, key)| tr(key).to_owned()).collect();
            let exe = std::env::current_exe().unwrap();
            let list: ICustomDestinationList =
                CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER).unwrap();
            let result = commit_tasks(&app, token, &exe, &labels);
            let cleanup = list.DeleteList(&HSTRING::from(&app));
            drop(list);
            CoUninitialize();
            result.unwrap();
            cleanup.unwrap();
        }
    }
}
