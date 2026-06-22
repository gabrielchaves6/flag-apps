// NewTrayFlag — toggles "show all tray icons" via IsPromoted registry sweep.
// Ported from Music\backup\newtrayflag\rs\src\main.rs

use super::{ids::NEWTRAYFLAG_BASE, FlagModule};
use flag_win::{make_dots_icon, rgb, w};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CMD_TOGGLE:    u32 = NEWTRAYFLAG_BASE;
const TIMER_SWEEP:   usize = NEWTRAYFLAG_BASE as usize;
const SWEEP_MS:      u32 = 1200;
const SWEEP_CAP:     usize = 8;

const REG_NOTIFY: &str = r"Control Panel\NotifyIconSettings";

pub struct NewTrayFlag {
    on:          bool,
    icon_on:     HICON,
    icon_off:    HICON,
    sweep_idx:   usize,
    keys:        Vec<String>,
}

unsafe impl Send for NewTrayFlag {}

impl NewTrayFlag {
    pub fn new() -> Self {
        Self {
            on:        false,
            icon_on:   HICON::default(),
            icon_off:  HICON::default(),
            sweep_idx: 0,
            keys:      vec![],
        }
    }
    pub fn current_icon(&self) -> HICON {
        if self.on { self.icon_on } else { self.icon_off }
    }
}

impl FlagModule for NewTrayFlag {
    fn name(&self) -> &'static str { "NewTrayFlag" }

    fn on_init(&mut self, hwnd: HWND) {
        unsafe {
            self.icon_on  = make_dots_icon(rgb(37, 99, 235));
            self.icon_off = make_dots_icon(rgb(80, 80, 92));
            self.keys = enum_notify_keys();
            self.on   = detect_state(&self.keys);
            let _ = SetTimer(hwnd, TIMER_SWEEP, SWEEP_MS, None);
        }
    }

    fn on_destroy(&mut self, hwnd: HWND) {
        unsafe { let _ = KillTimer(hwnd, TIMER_SWEEP); }
    }

    fn on_timer(&mut self, hwnd: HWND, id: usize) {
        if id != TIMER_SWEEP { return; }
        unsafe {
            let target = self.on;
            let end = (self.sweep_idx + SWEEP_CAP).min(self.keys.len());
            for key in &self.keys[self.sweep_idx..end] {
                set_promoted(key, target);
            }
            self.sweep_idx = end;
            if self.sweep_idx >= self.keys.len() {
                self.sweep_idx = 0;
                self.keys = enum_notify_keys();
                nudge_taskbar();
            }
        }
        let _ = hwnd;
    }

    fn append_menu(&self, hmenu: HMENU) -> bool {
        unsafe {
            let chk = if self.on { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(hmenu, MF_STRING | chk, CMD_TOGGLE as usize,
                PCWSTR(w("NewTrayFlag — Show all tray icons").as_ptr()));
        }
        true
    }

    fn on_command(&mut self, hwnd: HWND, cmd: u32) -> bool {
        if cmd == CMD_TOGGLE {
            self.on = !self.on;
            self.sweep_idx = 0;
            unsafe { set_all_promoted(&self.keys, self.on); nudge_taskbar(); }
            let _ = hwnd;
            return true;
        }
        false
    }
}

unsafe fn enum_notify_keys() -> Vec<String> {
    let sk = w(REG_NOTIFY);
    let mut hkey = HKEY::default();
    if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), 0, KEY_READ, &mut hkey) != ERROR_SUCCESS {
        return vec![];
    }
    let mut result = vec![];
    let mut idx = 0u32;
    loop {
        let mut name = vec![0u16; 512];
        let mut len = name.len() as u32;
        let rc = RegEnumKeyExW(hkey, idx, windows::core::PWSTR(name.as_mut_ptr()), &mut len,
            None, windows::core::PWSTR::null(), None, None);
        if rc != ERROR_SUCCESS { break; }
        let s = String::from_utf16_lossy(&name[..len as usize]);
        result.push(format!("{}\\{}", REG_NOTIFY, s));
        idx += 1;
    }
    let _ = RegCloseKey(hkey);
    result
}

unsafe fn detect_state(keys: &[String]) -> bool {
    keys.iter().any(|k| flag_win::reg_read_dword(k, "IsPromoted") == Some(1))
}

unsafe fn set_promoted(subkey: &str, on: bool) {
    flag_win::reg_write_dword(subkey, "IsPromoted", on as u32);
}

unsafe fn set_all_promoted(keys: &[String], on: bool) {
    for k in keys { set_promoted(k, on); }
}

unsafe fn nudge_taskbar() {
    let msg = w("TraySettings");
    SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0),
        LPARAM(msg.as_ptr() as isize), SMTO_ABORTIFHUNG, 1000, None);
}
