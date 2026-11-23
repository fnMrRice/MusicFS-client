use super::{
    buttons::{Buttons, IDS},
    Glyphs,
};
use windows::{
    core::Result as WinResult,
    Win32::{
        Foundation::{HWND, RPC_E_CHANGED_MODE},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{ITaskbarList3, TaskbarList, THBF_HIDDEN, THB_FLAGS, THUMBBUTTON},
    },
};

pub(super) struct Apartment(bool);
impl Apartment {
    pub fn enter() -> WinResult<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr == RPC_E_CHANGED_MODE {
            return Ok(Self(false));
        }
        hr.ok().map(|()| Self(true))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Resources {
    generation: u64,
    list: Option<ITaskbarList3>,
    installed: bool,
    glyphs: Option<Glyphs>,
    appearance: u64,
}
impl Resources {
    pub fn apply(
        &mut self,
        hwnd: HWND,
        buttons: Buttons,
        generation: u64,
        appearance: u64,
        dpi: u32,
    ) -> Result<(), String> {
        if self.generation != generation {
            self.list = None;
            self.installed = false;
            self.generation = generation;
        }
        if self.list.is_none() {
            self.list = Some(unsafe {
                let api: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| e.to_string())?;
                api.HrInit().map_err(|e| e.to_string())?;
                api
            });
        }
        let change = self
            .glyphs
            .as_ref()
            .is_none_or(|g| self.appearance != appearance && g.needs_refresh(dpi));
        if change {
            self.glyphs = Some(Glyphs::new(dpi)?);
        }
        self.appearance = appearance;
        let native = buttons.native(self.glyphs.as_ref().unwrap());
        let api = self.list.as_ref().unwrap();
        unsafe {
            if self.installed {
                api.ThumbBarUpdateButtons(hwnd, &native)
                    .map_err(|e| e.to_string())?;
            } else {
                // 重新分组可能对同一工具栏重复发送创建通知；已有按钮必须按原 ID 更新。
                if let Err(add) = api.ThumbBarAddButtons(hwnd, &native) {
                    api.ThumbBarUpdateButtons(hwnd, &native)
                        .map_err(|update| format!("add: {add}; update: {update}"))?;
                }
                self.installed = true;
            }
        }
        Ok(())
    }
    pub fn disconnect(&mut self) {
        self.list = None;
        self.installed = false;
    }
    pub fn hide(&self, hwnd: HWND) {
        if self.installed {
            if let Some(list) = &self.list {
                let hidden = IDS.map(|id| THUMBBUTTON {
                    iId: id,
                    dwMask: THB_FLAGS,
                    dwFlags: THBF_HIDDEN,
                    ..Default::default()
                });
                let _ = unsafe { list.ThumbBarUpdateButtons(hwnd, &hidden) };
            }
        }
    }
}
