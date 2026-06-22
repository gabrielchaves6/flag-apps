// AnyFlag — releases stuck modifier keys via Ctrl+Alt+Pause or Ctrl+Alt+Cancel.

use super::{ids::ANYFLAG_BASE, FlagModule};
use flag_win::{make_text_icon, reg_read_dword, reg_write_dword, rgb, w};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CMD_TOGGLE:    u32 = ANYFLAG_BASE;
const HOTKEY_PAUSE:  i32 = ANYFLAG_BASE as i32 + 10;
const HOTKEY_CANCEL: i32 = ANYFLAG_BASE as i32 + 11;
const TIMER_FLASH:   usize = ANYFLAG_BASE as usize;
const FLASH_MS:      u32 = 600;

const REG_KEY:     &str = r"Software\AnyFlag";
const REG_ENABLED: &str = "Enabled";

const BRAND: (u8, u8, u8) = (37, 99, 235);
const GREEN: (u8, u8, u8) = (34, 197, 94);

pub struct AnyFlag {
    enabled:     bool,
    icon_normal: HICON,
    icon_flash:  HICON,
    pub flashing: bool,
}

unsafe impl Send for AnyFlag {}

impl AnyFlag {
    pub fn new() -> Self {
        let enabled = unsafe { reg_read_dword(REG_KEY, REG_ENABLED).unwrap_or(0) != 0 };
        Self { enabled, icon_normal: HICON::default(), icon_flash: HICON::default(), flashing: false }
    }
}

impl FlagModule for AnyFlag {
    fn name(&self) -> &'static str { "AnyFlag" }

    fn on_init(&mut self, hwnd: HWND) {
        unsafe {
            self.icon_normal = make_text_icon(rgb(BRAND.0, BRAND.1, BRAND.2), "AF");
            self.icon_flash  = make_text_icon(rgb(GREEN.0,  GREEN.1,  GREEN.2),  "AF");
            if self.enabled { register_hotkeys(hwnd); }
        }
    }

    fn on_destroy(&mut self, hwnd: HWND) {
        unsafe { if self.enabled { unregister_hotkeys(hwnd); } }
    }

    fn on_hotkey(&mut self, hwnd: HWND, id: i32) {
        if !self.enabled { return; }
        if id == HOTKEY_PAUSE || id == HOTKEY_CANCEL {
            unsafe { do_release(hwnd, self); }
        }
    }

    fn on_timer(&mut self, hwnd: HWND, id: usize) {
        if id == TIMER_FLASH && self.flashing {
            unsafe {
                self.flashing = false;
                let _ = KillTimer(hwnd, TIMER_FLASH);
                let _ = PostMessageW(hwnd, WM_APP + 2, WPARAM(0), LPARAM(0));
            }
        }
    }

    fn append_menu(&self, hmenu: HMENU) -> bool {
        unsafe {
            let chk = if self.enabled { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(hmenu, MF_STRING | chk, CMD_TOGGLE as usize,
                PCWSTR(w("AnyFlag — Release stuck keys (Ctrl+Alt+Pause)").as_ptr()));
        }
        true
    }

    fn on_command(&mut self, hwnd: HWND, cmd: u32) -> bool {
        if cmd != CMD_TOGGLE { return false; }
        self.enabled = !self.enabled;
        unsafe {
            reg_write_dword(REG_KEY, REG_ENABLED, self.enabled as u32);
            if self.enabled { register_hotkeys(hwnd); } else { unregister_hotkeys(hwnd); }
        }
        true
    }
}

unsafe fn register_hotkeys(hwnd: HWND) {
    let _ = RegisterHotKey(hwnd, HOTKEY_PAUSE,
        HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0), VK_PAUSE.0 as u32);
    let _ = RegisterHotKey(hwnd, HOTKEY_CANCEL,
        HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0), VK_CANCEL.0 as u32);
}

unsafe fn unregister_hotkeys(hwnd: HWND) {
    let _ = UnregisterHotKey(hwnd, HOTKEY_PAUSE);
    let _ = UnregisterHotKey(hwnd, HOTKEY_CANCEL);
}

unsafe fn do_release(hwnd: HWND, module: &mut AnyFlag) {
    release_modifiers();
    module.flashing = true;
    let _ = SetTimer(hwnd, TIMER_FLASH, FLASH_MS, None);
    let _ = PostMessageW(hwnd, WM_APP + 2, WPARAM(0), LPARAM(0));
}

unsafe fn release_modifiers() {
    let keys = [
        VK_LCONTROL, VK_RCONTROL, VK_CONTROL,
        VK_LMENU, VK_RMENU, VK_MENU,
        VK_LSHIFT, VK_RSHIFT, VK_SHIFT,
        VK_LWIN, VK_RWIN,
    ];
    let inputs: Vec<INPUT> = keys.iter().map(|&vk| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: KEYEVENTF_KEYUP, time: 0, dwExtraInfo: 0 },
        },
    }).collect();
    SendInput(&inputs, core::mem::size_of::<INPUT>() as i32);
}
