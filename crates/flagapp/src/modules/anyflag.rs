// AnyFlag — releases stuck modifier keys via Ctrl+Alt+Pause or Ctrl+Alt+Cancel.
// Ported from Music\backup\any_flag\rs\src\main.rs

use super::{ids::ANYFLAG_BASE, FlagModule};
use flag_win::{make_text_icon, rgb, w};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const HOTKEY_PAUSE:  i32 = ANYFLAG_BASE as i32;
const HOTKEY_CANCEL: i32 = ANYFLAG_BASE as i32 + 1;
const TIMER_FLASH:   usize = ANYFLAG_BASE as usize;
const FLASH_MS:      u32 = 600;

const BRAND: (u8, u8, u8) = (37, 99, 235);
const GREEN: (u8, u8, u8) = (34, 197, 94);

pub struct AnyFlag {
    icon_normal: HICON,
    icon_flash:  HICON,
    pub flashing: bool,
}

unsafe impl Send for AnyFlag {}

impl AnyFlag {
    pub fn new() -> Self {
        Self {
            icon_normal: HICON::default(),
            icon_flash:  HICON::default(),
            flashing:    false,
        }
    }
    pub fn current_icon(&self) -> HICON {
        if self.flashing { self.icon_flash } else { self.icon_normal }
    }
}

impl FlagModule for AnyFlag {
    fn name(&self) -> &'static str { "AnyFlag" }

    fn on_init(&mut self, hwnd: HWND) {
        unsafe {
            self.icon_normal = make_text_icon(rgb(BRAND.0, BRAND.1, BRAND.2), "AF");
            self.icon_flash  = make_text_icon(rgb(GREEN.0, GREEN.1, GREEN.2), "AF");
            let _ = RegisterHotKey(hwnd, HOTKEY_PAUSE,
                HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0),
                VK_PAUSE.0 as u32);
            let _ = RegisterHotKey(hwnd, HOTKEY_CANCEL,
                HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_ALT.0 | MOD_NOREPEAT.0),
                VK_CANCEL.0 as u32);
        }
    }

    fn on_destroy(&mut self, hwnd: HWND) {
        unsafe {
            let _ = UnregisterHotKey(hwnd, HOTKEY_PAUSE);
            let _ = UnregisterHotKey(hwnd, HOTKEY_CANCEL);
        }
    }

    fn on_hotkey(&mut self, hwnd: HWND, id: i32) {
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
            let _ = AppendMenuW(hmenu, MF_GRAYED | MF_STRING, ANYFLAG_BASE as usize,
                PCWSTR(w("AnyFlag — release modifiers: Ctrl+Alt+Pause").as_ptr()));
        }
        true
    }

    fn on_command(&mut self, _hwnd: HWND, _cmd: u32) -> bool { false }
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
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }).collect();
    SendInput(&inputs, core::mem::size_of::<INPUT>() as i32);
}
