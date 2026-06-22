// EnergyFlag — switches between Remote and OnSite power profiles.
// Also manages: veil (per-monitor capture-excluded overlay), dim (LightSwitch→PowerToys),
// and NoSleepDC (caffeinate on battery).
// Ported from Music\backup\energy_flag\rs\src\main.rs

use super::{ids::ENERGYFLAG_BASE, FlagModule};
use flag_win::{make_text_icon, reg_read_dword, reg_write_dword, rgb, w};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, EXECUTION_STATE};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, ResetEvent};
use windows::Win32::System::Threading::{CreateProcessW, PROCESS_CREATION_FLAGS, STARTUPINFOW, PROCESS_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::*;

const CMD_REMOTE:     u32 = ENERGYFLAG_BASE;
const CMD_ONSITE:     u32 = ENERGYFLAG_BASE + 1;
const CMD_VEIL:       u32 = ENERGYFLAG_BASE + 2;
const CMD_DIM:        u32 = ENERGYFLAG_BASE + 3;
const CMD_NOSLEEP_DC: u32 = ENERGYFLAG_BASE + 4;

const REG_KEY:     &str = r"Software\EnergyFlag";
const REG_MODE:    &str = "Mode";
const REG_VEIL:    &str = "Veil";
const REG_DIM:     &str = "Dim";
const REG_NOSLEEP: &str = "NoSleepDC";

const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;
const WS_EX_TRANSPARENT_VAL:  u32 = 0x00000020;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EfMode { Remote, OnSite }

struct PowerTimeouts { monitor_ac: u32, monitor_dc: u32, standby_ac: u32, standby_dc: u32, hibernate_ac: u32, hibernate_dc: u32 }

const REMOTE_TIMEOUTS: PowerTimeouts = PowerTimeouts { monitor_ac: 30, monitor_dc: 15, standby_ac: 0, standby_dc: 0, hibernate_ac: 0, hibernate_dc: 0 };
const ONSITE_TIMEOUTS: PowerTimeouts  = PowerTimeouts { monitor_ac: 10, monitor_dc: 5,  standby_ac: 30, standby_dc: 20, hibernate_ac: 0, hibernate_dc: 180 };

pub struct EnergyFlag {
    mode:      EfMode,
    veil:      bool,
    dim:       bool,
    nosleep:   bool,
    icon_remote: HICON,
    icon_onsite: HICON,
    veil_hwnds:  Vec<HWND>,
    dim_event:   Option<HANDLE>,
}

unsafe impl Send for EnergyFlag {}

impl EnergyFlag {
    pub fn new() -> Self {
        unsafe {
            let mode    = if reg_read_dword(REG_KEY, REG_MODE).unwrap_or(0)    == 1 { EfMode::OnSite } else { EfMode::Remote };
            let veil    = reg_read_dword(REG_KEY, REG_VEIL).unwrap_or(0)    != 0;
            let dim     = reg_read_dword(REG_KEY, REG_DIM).unwrap_or(0)     != 0;
            let nosleep = reg_read_dword(REG_KEY, REG_NOSLEEP).unwrap_or(0) != 0;
            Self { mode, veil, dim, nosleep, icon_remote: HICON::default(), icon_onsite: HICON::default(), veil_hwnds: vec![], dim_event: None }
        }
    }
    pub fn current_icon(&self) -> HICON {
        match self.mode { EfMode::Remote => self.icon_remote, EfMode::OnSite => self.icon_onsite }
    }
}

impl FlagModule for EnergyFlag {
    fn name(&self) -> &'static str { "EnergyFlag" }

    fn on_init(&mut self, hwnd: HWND) {
        unsafe {
            self.icon_remote = make_text_icon(rgb(37, 99, 235), "R");
            self.icon_onsite = make_text_icon(rgb(16, 185, 129), "O");
            apply_mode(self.mode);
            if self.veil  { show_veil(hwnd, &mut self.veil_hwnds);  }
            if self.dim   { signal_dim(self.dim, &mut self.dim_event); }
            if self.nosleep { apply_nosleep(self.nosleep); }
        }
    }

    fn on_destroy(&mut self, _hwnd: HWND) {
        unsafe {
            hide_veil(&mut self.veil_hwnds);
            signal_dim(false, &mut self.dim_event);
            apply_nosleep(false);
        }
    }

    fn on_display_change(&mut self, hwnd: HWND) {
        unsafe {
            if self.veil {
                hide_veil(&mut self.veil_hwnds);
                show_veil(hwnd, &mut self.veil_hwnds);
            }
        }
    }

    fn append_menu(&self, hmenu: HMENU) -> bool {
        unsafe {
            let cr = if self.mode == EfMode::Remote  { MF_CHECKED } else { MF_UNCHECKED };
            let co = if self.mode == EfMode::OnSite  { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(hmenu, MF_STRING | cr, CMD_REMOTE as usize, PCWSTR(w("EnergyFlag — Remote").as_ptr()));
            let _ = AppendMenuW(hmenu, MF_STRING | co, CMD_ONSITE as usize, PCWSTR(w("EnergyFlag — OnSite").as_ptr()));
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let cv = if self.veil    { MF_CHECKED } else { MF_UNCHECKED };
            let cd = if self.dim     { MF_CHECKED } else { MF_UNCHECKED };
            let cn = if self.nosleep { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(hmenu, MF_STRING | cv, CMD_VEIL as usize,       PCWSTR(w("EnergyFlag — Veil (hide screen)").as_ptr()));
            let _ = AppendMenuW(hmenu, MF_STRING | cd, CMD_DIM as usize,        PCWSTR(w("EnergyFlag — Dim (PowerToys)").as_ptr()));
            let _ = AppendMenuW(hmenu, MF_STRING | cn, CMD_NOSLEEP_DC as usize, PCWSTR(w("EnergyFlag — Keep awake on battery").as_ptr()));
        }
        true
    }

    fn on_command(&mut self, hwnd: HWND, cmd: u32) -> bool {
        match cmd {
            CMD_REMOTE => { self.mode = EfMode::Remote; unsafe { apply_mode(self.mode); reg_write_dword(REG_KEY, REG_MODE, 0); } true }
            CMD_ONSITE => { self.mode = EfMode::OnSite; unsafe { apply_mode(self.mode); reg_write_dword(REG_KEY, REG_MODE, 1); } true }
            CMD_VEIL => {
                self.veil = !self.veil;
                unsafe {
                    reg_write_dword(REG_KEY, REG_VEIL, self.veil as u32);
                    if self.veil { show_veil(hwnd, &mut self.veil_hwnds); }
                    else         { hide_veil(&mut self.veil_hwnds); }
                }
                true
            }
            CMD_DIM => {
                self.dim = !self.dim;
                unsafe { reg_write_dword(REG_KEY, REG_DIM, self.dim as u32); signal_dim(self.dim, &mut self.dim_event); }
                true
            }
            CMD_NOSLEEP_DC => {
                self.nosleep = !self.nosleep;
                unsafe { reg_write_dword(REG_KEY, REG_NOSLEEP, self.nosleep as u32); apply_nosleep(self.nosleep); }
                true
            }
            _ => false,
        }
    }
}

unsafe fn apply_mode(mode: EfMode) {
    let t = match mode { EfMode::Remote => &REMOTE_TIMEOUTS, EfMode::OnSite => &ONSITE_TIMEOUTS };
    powercfg("/change", &format!("monitor-timeout-ac {}", t.monitor_ac));
    powercfg("/change", &format!("monitor-timeout-dc {}", t.monitor_dc));
    powercfg("/change", &format!("standby-timeout-ac {}", t.standby_ac));
    powercfg("/change", &format!("standby-timeout-dc {}", t.standby_dc));
    powercfg("/change", &format!("hibernate-timeout-ac {}", t.hibernate_ac));
    powercfg("/change", &format!("hibernate-timeout-dc {}", t.hibernate_dc));
}

fn powercfg(cmd: &str, args: &str) {
    unsafe {
        let mut cmdline = flag_win::w(&format!("powercfg {} {}", cmd, args));
        let mut si = STARTUPINFOW { cb: core::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
        let mut pi = PROCESS_INFORMATION::default();
        let _ = CreateProcessW(None, PWSTR(cmdline.as_mut_ptr()),
            None, None, false, PROCESS_CREATION_FLAGS(0x08000000),
            None, None, &mut si, &mut pi);
    }
}

unsafe fn apply_nosleep(on: bool) {
    let _ = SetThreadExecutionState(
        if on { EXECUTION_STATE(ES_CONTINUOUS.0 | ES_SYSTEM_REQUIRED.0 | ES_DISPLAY_REQUIRED.0) }
        else  { ES_CONTINUOUS }
    );
}

unsafe fn signal_dim(on: bool, ev: &mut Option<HANDLE>) {
    let event_name = w("LightSwitch");
    if on {
        let h = CreateEventW(None, true, false, PCWSTR(event_name.as_ptr())).ok();
        if let Some(h) = h {
            let _ = SetEvent(h);
            *ev = Some(h);
        }
    } else if let Some(h) = ev.take() {
        let _ = ResetEvent(h);
        let _ = CloseHandle(h);
    }
}

extern "system" fn veil_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe fn show_veil(msg_hwnd: HWND, veils: &mut Vec<HWND>) {
    let hinstance = GetModuleHandleW(None).map(|h| HINSTANCE::from(h)).unwrap_or_default();
    let class = w("FlagVeil");
    let wc = WNDCLASSEXW {
        cbSize: core::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(veil_wndproc),
        hInstance: hinstance,
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    RegisterClassExW(&wc);

    let mut monitors: Vec<MONITORINFO> = vec![];
    extern "system" fn mon_enum(hm: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        unsafe {
            let v = &mut *(lp.0 as *mut Vec<MONITORINFO>);
            let mut mi = MONITORINFO { cbSize: core::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(hm, &mut mi);
            v.push(mi);
        }
        TRUE
    }
    let _ = EnumDisplayMonitors(None, None, Some(mon_enum), LPARAM(&mut monitors as *mut _ as isize));

    for mi in &monitors {
        let rc = mi.rcMonitor;
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_LAYERED.0 | WS_EX_TOPMOST.0 | WS_EX_TRANSPARENT_VAL),
            PCWSTR(class.as_ptr()), PCWSTR(w("").as_ptr()), WS_POPUP,
            rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top,
            msg_hwnd, None, hinstance, None) {
            Ok(h) => h,
            Err(_) => continue,
        };
        use windows::Win32::UI::WindowsAndMessaging::SetWindowDisplayAffinity;
        let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(WDA_EXCLUDEFROMCAPTURE));
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        veils.push(hwnd);
    }
}

unsafe fn hide_veil(veils: &mut Vec<HWND>) {
    for &h in veils.iter() { let _ = DestroyWindow(h); }
    veils.clear();
}
