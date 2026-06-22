// KeyFlag — switches keyboard layout between US-International and ABNT2.
// Ported from Music\backup\keyboard_flag\rs\src\main.rs

use super::{ids::KEYFLAG_BASE, FlagModule};
use flag_win::{make_text_icon, reg_read_dword, reg_write_dword, rgb, w};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CMD_US:    u32 = KEYFLAG_BASE;
const CMD_ABNT2: u32 = KEYFLAG_BASE + 1;
const TIMER_ENFORCE: usize = KEYFLAG_BASE as usize;
const ENFORCE_MS: u32 = 350;
const WINEVENT_HOOK_MSG: u32 = WM_APP + 10;

const REG_KEY: &str   = r"Software\KeyFlag";
const REG_MODE: &str  = "Mode";
const KLID_US:    &str = "00020409";
const KLID_ABNT2: &str = "00010416";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KfMode { UsIntl, Abnt2 }

impl KfMode {
    fn klid(self) -> &'static str { match self { KfMode::UsIntl => KLID_US, KfMode::Abnt2 => KLID_ABNT2 } }
    fn label(self) -> &'static str { match self { KfMode::UsIntl => "US", KfMode::Abnt2 => "AB" } }
}

pub struct KeyFlag {
    mode:         KfMode,
    icon_us:      HICON,
    icon_abnt2:   HICON,
    hook:         HWINEVENTHOOK,
}

unsafe impl Send for KeyFlag {}

impl KeyFlag {
    pub fn new() -> Self {
        let mode = unsafe {
            match reg_read_dword(REG_KEY, REG_MODE) {
                Some(0) => KfMode::UsIntl,
                Some(1) => KfMode::Abnt2,
                _       => KfMode::UsIntl,
            }
        };
        Self {
            mode,
            icon_us:    HICON::default(),
            icon_abnt2: HICON::default(),
            hook:       HWINEVENTHOOK::default(),
        }
    }
    pub fn current_icon(&self) -> HICON {
        match self.mode { KfMode::UsIntl => self.icon_us, KfMode::Abnt2 => self.icon_abnt2 }
    }
}

impl FlagModule for KeyFlag {
    fn name(&self) -> &'static str { "KeyFlag" }

    fn on_init(&mut self, hwnd: HWND) {
        unsafe {
            self.icon_us    = make_text_icon(rgb(37, 99, 235), "US");
            self.icon_abnt2 = make_text_icon(rgb(124, 58, 237), "AB");
            apply_mode(self.mode);
            self.hook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND,
                None, Some(winevent_proc), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
            let _ = SetTimer(hwnd, TIMER_ENFORCE, ENFORCE_MS, None);
        }
    }

    fn on_destroy(&mut self, hwnd: HWND) {
        unsafe {
            let _ = KillTimer(hwnd, TIMER_ENFORCE);
            if !self.hook.is_invalid() { let _ = UnhookWinEvent(self.hook); }
        }
    }

    fn on_timer(&mut self, hwnd: HWND, id: usize) {
        if id == TIMER_ENFORCE { unsafe { enforce(self.mode); } }
        let _ = hwnd;
    }

    fn on_message(&mut self, hwnd: HWND, msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
        if msg == WINEVENT_HOOK_MSG {
            unsafe { enforce(self.mode); }
            return Some(LRESULT(0));
        }
        let _ = hwnd;
        None
    }

    fn append_menu(&self, hmenu: HMENU) -> bool {
        unsafe {
            let chk_us    = if self.mode == KfMode::UsIntl { MF_CHECKED } else { MF_UNCHECKED };
            let chk_abnt2 = if self.mode == KfMode::Abnt2   { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(hmenu, MF_STRING | chk_us,    CMD_US as usize,    PCWSTR(w("KeyFlag — US International").as_ptr()));
            let _ = AppendMenuW(hmenu, MF_STRING | chk_abnt2, CMD_ABNT2 as usize, PCWSTR(w("KeyFlag — ABNT2 (PT-BR)").as_ptr()));
        }
        true
    }

    fn on_command(&mut self, _hwnd: HWND, cmd: u32) -> bool {
        match cmd {
            CMD_US    => { self.set_mode(KfMode::UsIntl); true }
            CMD_ABNT2 => { self.set_mode(KfMode::Abnt2);   true }
            _ => false,
        }
    }
}

impl KeyFlag {
    fn set_mode(&mut self, mode: KfMode) {
        self.mode = mode;
        unsafe {
            reg_write_dword(REG_KEY, REG_MODE, match mode { KfMode::UsIntl => 0, KfMode::Abnt2 => 1 });
            apply_mode(mode);
        }
    }
}

unsafe fn apply_mode(mode: KfMode) {
    let klid = w(mode.klid());
    let hkl = LoadKeyboardLayoutW(PCWSTR(klid.as_ptr()), KLF_ACTIVATE).unwrap_or_default();
    unload_others(hkl);
    set_preload(mode);
    EnumWindows(Some(broadcast_lang), LPARAM(hkl.0 as isize));
}

unsafe fn unload_others(keep: HKL) {
    let mut hkls = [HKL::default(); 32];
    let n = GetKeyboardLayoutList(Some(&mut hkls));
    for &hkl in &hkls[..n as usize] {
        if hkl != keep { let _ = UnloadKeyboardLayout(hkl); }
    }
}

unsafe fn set_preload(mode: KfMode) {
    let sk = r"Keyboard Layout\Preload";
    flag_win::reg_write_str(sk, "1", mode.klid());
}

extern "system" fn broadcast_lang(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let hkl = lparam.0 as *mut std::ffi::c_void;
        let _ = PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST, WPARAM(0), LPARAM(hkl as isize));
    }
    TRUE
}

unsafe fn enforce(mode: KfMode) {
    let fw = GetForegroundWindow();
    if fw.0.is_null() { return; }
    let hkl = GetKeyboardLayout(GetWindowThreadProcessId(fw, None));
    let klid = w(mode.klid());
    let want = LoadKeyboardLayoutW(PCWSTR(klid.as_ptr()), KLF_ACTIVATE).unwrap_or_default();
    if hkl != want {
        let _ = PostMessageW(fw, WM_INPUTLANGCHANGEREQUEST, WPARAM(0), LPARAM(want.0 as isize));
    }
}

extern "system" fn winevent_proc(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    // No hwnd available here; signal the message window via a static pointer.
    unsafe {
        if let Some(hwnd) = GLOBAL_MSG_HWND {
            let _ = PostMessageW(hwnd, WINEVENT_HOOK_MSG, WPARAM(0), LPARAM(0));
        }
    }
}

static mut GLOBAL_MSG_HWND: Option<HWND> = None;
pub fn register_hwnd(hwnd: HWND) { unsafe { GLOBAL_MSG_HWND = Some(hwnd); } }
