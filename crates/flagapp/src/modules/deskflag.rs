// DeskFlag — virtual desktop HUD overlay.
// Shows current desktop index/total as a GDI painted layered window when desktop switches.
// Uses IVirtualDesktopManagerInternal (undocumented COM) for total count.
#![allow(static_mut_refs)]

use super::{ids::DESKFLAG_BASE, FlagModule};
use flag_win::{get_work_area, make_text_icon, rgb, w, wn};
use windows::core::{GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CMD_TOGGLE:    u32 = DESKFLAG_BASE;
const TIMER_HUD:     usize = DESKFLAG_BASE as usize;
const HUD_MS:        u32 = 2500;
const WM_DESK_CHANGE: u32 = WM_APP + 20;

pub struct DeskFlag {
    enabled:     bool,
    icon_on:     HICON,
    icon_off:    HICON,
    hud_hwnd:    Option<HWND>,
    current_idx: u32,
    total:       u32,
    hook:        HWINEVENTHOOK,
}

unsafe impl Send for DeskFlag {}

impl DeskFlag {
    pub fn new() -> Self {
        Self {
            enabled:     true,
            icon_on:     HICON::default(),
            icon_off:    HICON::default(),
            hud_hwnd:    None,
            current_idx: 0,
            total:       1,
            hook:        HWINEVENTHOOK::default(),
        }
    }
    pub fn current_icon(&self) -> HICON {
        if self.enabled { self.icon_on } else { self.icon_off }
    }
}

impl FlagModule for DeskFlag {
    fn name(&self) -> &'static str { "DeskFlag" }

    fn on_init(&mut self, hwnd: HWND) {
        unsafe {
            self.icon_on  = make_text_icon(rgb(37, 99, 235), "DF");
            self.icon_off = make_text_icon(rgb(80, 80, 92),  "DF");

            // Query total desktop count via undocumented COM
            self.total = query_desktop_count().max(1);

            // Create the HUD overlay window
            let hinstance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .map(|h| HINSTANCE::from(h)).unwrap_or_default();
            let class = w("FlagDeskHUD");
            let wc = WNDCLASSEXW {
                cbSize: core::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(hud_wndproc),
                hInstance: hinstance,
                lpszClassName: PCWSTR(class.as_ptr()),
                hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0 as *mut _),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            match CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_LAYERED.0 | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TRANSPARENT.0),
                PCWSTR(class.as_ptr()), PCWSTR(w("DeskFlag HUD").as_ptr()),
                WS_POPUP, 0, 0, 320, 72, None, None, hinstance, None)
            {
                Ok(h) => {
                    self.hud_hwnd = Some(h);
                }
                Err(_) => {}
            }

            // Hook EVENT_SYSTEM_DESKTOPSWITCH (0x0020)
            self.hook = SetWinEventHook(0x0020, 0x0020, None, Some(desk_winevent),
                0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);

            GLOBAL_DESKFLAG_HWND = Some(hwnd);
        }
    }

    fn on_destroy(&mut self, hwnd: HWND) {
        unsafe {
            if !self.hook.is_invalid() { let _ = UnhookWinEvent(self.hook); }
            if let Some(h) = self.hud_hwnd.take() { let _ = DestroyWindow(h); }
            let _ = hwnd;
        }
    }

    fn on_timer(&mut self, hwnd: HWND, id: usize) {
        if id == TIMER_HUD {
            unsafe {
                let _ = KillTimer(hwnd, TIMER_HUD);
                if let Some(h) = self.hud_hwnd {
                    let _ = ShowWindow(h, SW_HIDE);
                }
            }
        }
    }

    fn on_message(&mut self, hwnd: HWND, msg: u32, _wp: WPARAM, _lp: LPARAM) -> Option<windows::Win32::Foundation::LRESULT> {
        if msg == WM_DESK_CHANGE {
            unsafe {
                self.total = query_desktop_count().max(1);
                self.current_idx = (self.current_idx + 1) % self.total;
                if self.enabled { self.show_hud(hwnd); }
            }
            return Some(LRESULT(0));
        }
        None
    }

    fn append_menu(&self, hmenu: HMENU) -> bool {
        unsafe {
            let chk = if self.enabled { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(hmenu, MF_STRING | chk, CMD_TOGGLE as usize,
                PCWSTR(w("DeskFlag — Virtual desktop HUD").as_ptr()));
        }
        true
    }

    fn on_command(&mut self, _hwnd: HWND, cmd: u32) -> bool {
        if cmd == CMD_TOGGLE {
            self.enabled = !self.enabled;
            if !self.enabled {
                if let Some(h) = self.hud_hwnd {
                    unsafe { let _ = ShowWindow(h, SW_HIDE); }
                }
            }
            return true;
        }
        false
    }
}

impl DeskFlag {
    unsafe fn show_hud(&mut self, msg_hwnd: HWND) {
        let Some(hud) = self.hud_hwnd else { return };
        let hud_w = 320i32; let hud_h = 72i32;
        let work = get_work_area();
        let x = if work.right > work.left { work.right - hud_w - 24 } else { 100 };
        let y = if work.bottom > work.top { work.bottom - hud_h - 16 } else { 100 };

        let _ = SetWindowPos(hud, HWND_TOPMOST, x, y, hud_w, hud_h, SWP_SHOWWINDOW | SWP_NOACTIVATE);

        // Paint into the layered window using UpdateLayeredWindow
        paint_hud_gdi(hud, hud_w, hud_h, self.current_idx + 1, self.total);

        let _ = SetTimer(msg_hwnd, TIMER_HUD, HUD_MS, None);
    }
}

unsafe fn paint_hud_gdi(hwnd: HWND, w_px: i32, h_px: i32, current: u32, total: u32) {
    let screen_dc = GetDC(None);
    let mem_dc    = CreateCompatibleDC(screen_dc);
    let hbm       = CreateCompatibleBitmap(screen_dc, w_px, h_px);
    let old_bm    = SelectObject(mem_dc, hbm);

    // Background — dark semi-transparent (paint fully opaque; alpha set via UpdateLayeredWindow)
    let bg = CreateRoundRectRgn(0, 0, w_px, h_px, 18, 18);
    let bg_brush = CreateSolidBrush(rgb(24, 27, 35));
    FillRgn(mem_dc, bg, bg_brush);
    let _ = DeleteObject(bg);
    let _ = DeleteObject(bg_brush);

    // Text
    let label = format!("Desktop {} / {}", current, total);
    let font = flag_win::make_font(28, 600, false);
    let old_font = SelectObject(mem_dc, font);
    SetBkMode(mem_dc, TRANSPARENT);
    let _ = SetTextColor(mem_dc, rgb(245, 246, 250));
    let mut t = wn(&label);
    let mut rc = RECT { left: 0, top: 0, right: w_px, bottom: h_px };
    DrawTextW(mem_dc, &mut t, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    SelectObject(mem_dc, old_font);
    let _ = DeleteObject(font);

    // Build alpha channel: fully opaque where we painted (use ULW_ALPHA=2)
    let mut blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 220, AlphaFormat: 0 };
    let pt_dst = POINT { x: 0, y: 0 }; // ignored (used in ULW call below)
    let sz = SIZE { cx: w_px, cy: h_px };
    let pt_src = POINT { x: 0, y: 0 };
    let _ = UpdateLayeredWindow(hwnd, screen_dc, None, Some(&sz), mem_dc, Some(&pt_src), COLORREF(0), Some(&mut blend), UPDATE_LAYERED_WINDOW_FLAGS(2));

    SelectObject(mem_dc, old_bm);
    let _ = DeleteObject(hbm);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(None, screen_dc);
}

// ── undocumented IVirtualDesktopManagerInternal via raw COM ──────────────────

const CLSID_VDM: GUID = GUID { data1: 0xc5e0cdca, data2: 0x7b6e, data3: 0x41b2, data4: [0x9f, 0xc4, 0xd9, 0x39, 0x75, 0xcc, 0x46, 0x7b] };
const IID_IVDMI: GUID = GUID { data1: 0xf31574d6, data2: 0xb682, data3: 0x4cdc, data4: [0xbd, 0x56, 0x18, 0x27, 0x86, 0x0a, 0xbe, 0xc6] };

#[repr(C)]
struct VdmiVtbl {
    // IUnknown methods (3)
    query_interface: *const (),
    add_ref:         *const (),
    release:         *const (),
    // IVirtualDesktopManagerInternal
    get_count:       unsafe extern "system" fn(this: *mut VdmiRaw, count: *mut u32) -> HRESULT,
}

#[repr(C)]
struct VdmiRaw { vtbl: *const VdmiVtbl }

unsafe fn query_desktop_count() -> u32 {
    use windows::Win32::System::Com::CoCreateInstance;
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

    // Use CoCreateInstance with IUnknown return, then reinterpret as our raw vtable.
    // This bypasses the lack of IVirtualDesktopManagerInternal in the public windows crate.
    type CoCreateInstanceRaw = unsafe extern "system" fn(
        rclsid: *const GUID, punk_outer: *mut core::ffi::c_void, ctx: u32,
        riid: *const GUID, ppv: *mut *mut core::ffi::c_void) -> HRESULT;

    let ole32 = windows::Win32::System::LibraryLoader::LoadLibraryW(
        windows::core::PCWSTR(flag_win::w("ole32.dll").as_ptr())).ok();
    let Some(ole32) = ole32 else { return 1 };
    let fn_name = windows::core::PCSTR(b"CoCreateInstance\0".as_ptr());
    let proc = windows::Win32::System::LibraryLoader::GetProcAddress(ole32, fn_name);
    let Some(proc) = proc else { return 1 };
    let co_create: CoCreateInstanceRaw = core::mem::transmute(proc);

    let mut punk: *mut core::ffi::c_void = core::ptr::null_mut();
    let hr = co_create(&CLSID_VDM, core::ptr::null_mut(), 4 /* CLSCTX_LOCAL_SERVER */,
        &IID_IVDMI, &mut punk);
    if hr.is_err() || punk.is_null() { return 1; }

    let raw = punk as *mut VdmiRaw;
    let mut count = 1u32;
    let _ = ((*(*raw).vtbl).get_count)(raw, &mut count);

    // IUnknown::Release via vtable slot 2
    type ReleaseFn = unsafe extern "system" fn(*mut core::ffi::c_void) -> u32;
    let vtbl_bytes = (*raw).vtbl as *const *const ();
    let release_ptr = *vtbl_bytes.add(2);
    let release: ReleaseFn = core::mem::transmute(release_ptr);
    release(punk);

    count
}

extern "system" fn hud_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

static mut GLOBAL_DESKFLAG_HWND: Option<HWND> = None;

extern "system" fn desk_winevent(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    unsafe {
        if let Some(hwnd) = GLOBAL_DESKFLAG_HWND {
            let _ = PostMessageW(hwnd, WM_DESK_CHANGE, WPARAM(0), LPARAM(0));
        }
    }
}
