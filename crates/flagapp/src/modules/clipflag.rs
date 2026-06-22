// ClipFlag — Rust port of snagit-clip-to-file.
// Watches clipboard for PNG images; saves to %LocalAppData%\snagit-clip-to-file\png\
// and rewrites clipboard with FileDrop + path text so Teams/Explorer can receive it.
// Uses AddClipboardFormatListener (WM_CLIPBOARDUPDATE) — no polling.

use super::{ids::CLIPFLAG_BASE, FlagModule};
use flag_win::{make_text_icon, rgb, w};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

// Standard clipboard format IDs
const CF_BITMAP_ID:      u32 = 2;
const CF_UNICODETEXT_ID: u32 = 13;
const CF_HDROP_ID:       u32 = 15;

const CMD_TOGGLE: u32 = CLIPFLAG_BASE;
const KEEP_LAST:  usize = 200;

// Shell KNOWNFOLDERID for LocalAppData
const FOLDERID_LOCAL_APP_DATA: windows::core::GUID = windows::core::GUID::from_u128(0xf1b32785_6fba_4fcf_9d55_7b8e7f157091);

pub struct ClipFlag {
    enabled:  bool,
    icon_on:  HICON,
    icon_off: HICON,
    out_dir:  std::path::PathBuf,
}

unsafe impl Send for ClipFlag {}

impl ClipFlag {
    pub fn new() -> Self {
        let out_dir = local_appdata_path();
        Self {
            enabled:  true,
            icon_on:  HICON::default(),
            icon_off: HICON::default(),
            out_dir,
        }
    }
    pub fn current_icon(&self) -> HICON {
        if self.enabled { self.icon_on } else { self.icon_off }
    }
}

impl FlagModule for ClipFlag {
    fn name(&self) -> &'static str { "ClipFlag" }

    fn on_init(&mut self, hwnd: HWND) {
        unsafe {
            self.icon_on  = make_text_icon(rgb(37, 99, 235),   "CF");
            self.icon_off = make_text_icon(rgb(80, 80, 92),     "CF");
            let _ = std::fs::create_dir_all(&self.out_dir);
            if self.enabled {
                let _ = AddClipboardFormatListener(hwnd);
            }
        }
    }

    fn on_destroy(&mut self, hwnd: HWND) {
        unsafe { let _ = RemoveClipboardFormatListener(hwnd); }
    }

    fn on_clipboard_update(&mut self, hwnd: HWND) {
        if !self.enabled { return; }
        unsafe { self.handle_clipboard(hwnd); }
    }

    fn append_menu(&self, hmenu: HMENU) -> bool {
        unsafe {
            let chk = if self.enabled { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(hmenu, MF_STRING | chk, CMD_TOGGLE as usize,
                PCWSTR(w("ClipFlag — Save clipboard PNG to file").as_ptr()));
        }
        true
    }

    fn on_command(&mut self, hwnd: HWND, cmd: u32) -> bool {
        if cmd == CMD_TOGGLE {
            self.enabled = !self.enabled;
            unsafe {
                if self.enabled { let _ = AddClipboardFormatListener(hwnd); }
                else            { let _ = RemoveClipboardFormatListener(hwnd); }
            }
            return true;
        }
        false
    }
}

impl ClipFlag {
    unsafe fn handle_clipboard(&self, hwnd: HWND) {
        // Guard: skip if clipboard already contains a FileDrop (our own write)
        if IsClipboardFormatAvailable(CF_HDROP_ID).is_ok() { return; }
        if !IsClipboardFormatAvailable(CF_BITMAP_ID).is_ok() { return; }

        if !OpenClipboard(hwnd).is_ok() { return; }
        let hbitmap = {
            let h = GetClipboardData(CF_BITMAP_ID);
            CloseClipboard();
            match h { Ok(h) => HBITMAP(h.0 as *mut _), Err(_) => return }
        };

        let path = self.save_bitmap(hbitmap);
        if path.is_none() { return; }
        let path = path.unwrap();
        self.prune_old();
        self.rewrite_clipboard(hwnd, &path);
    }

    unsafe fn save_bitmap(&self, hbmp: HBITMAP) -> Option<std::path::PathBuf> {
        use windows::Win32::Graphics::Gdi::*;
        let screen_dc = GetDC(None);
        let mem_dc    = CreateCompatibleDC(screen_dc);
        let old       = SelectObject(mem_dc, hbmp);

        let mut bm = BITMAP::default();
        GetObjectW(hbmp, core::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
        let (w_px, h_px) = (bm.bmWidth, bm.bmHeight);
        if w_px <= 0 || h_px <= 0 { SelectObject(mem_dc, old); DeleteDC(mem_dc); ReleaseDC(None, screen_dc); return None; }

        let bmi = BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w_px, biHeight: -h_px, biPlanes: 1, biBitCount: 32,
            biCompression: BI_RGB.0, ..Default::default()
        };
        let row_bytes = (w_px * 4) as usize;
        let mut pixels = vec![0u8; row_bytes * h_px as usize];
        let mut bi = BITMAPINFO { bmiHeader: bmi, ..Default::default() };
        GetDIBits(mem_dc, hbmp, 0, h_px as u32, Some(pixels.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        // BGRA → RGBA swap
        for px in pixels.chunks_exact_mut(4) { px.swap(0, 2); px[3] = 255; }
        SelectObject(mem_dc, old); DeleteDC(mem_dc); ReleaseDC(None, screen_dc);

        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let secs = now.as_secs();
        let ms   = now.subsec_millis();
        // Format as pseudo-timestamp using seconds+ms (no chrono dep)
        let name = format!("snip_{:020}_{:03}.png", secs, ms);
        let path = self.out_dir.join(&name);

        // Write PNG manually (no dep — use raw PNG encoder)
        match write_rgba_png(&path, w_px as u32, h_px as u32, &pixels) {
            Ok(()) => Some(path),
            Err(_) => None,
        }
    }

    fn prune_old(&self) {
        let Ok(mut entries) = std::fs::read_dir(&self.out_dir) else { return };
        let mut files: Vec<_> = entries.flatten()
            .filter(|e| e.path().extension().map(|x| x == "png").unwrap_or(false))
            .collect();
        files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        if files.len() > KEEP_LAST {
            for old in &files[..files.len() - KEEP_LAST] {
                let _ = std::fs::remove_file(old.path());
            }
        }
    }

    unsafe fn rewrite_clipboard(&self, hwnd: HWND, path: &std::path::Path) {
        if !OpenClipboard(hwnd).is_ok() { return; }
        EmptyClipboard();

        // DROPFILES layout (manually defined to avoid SystemServices feature)
        #[repr(C)]
        struct DropFiles { p_files: u32, pt_x: i32, pt_y: i32, f_nc: i32, f_wide: i32 }

        // 1. CF_HDROP — for Explorer/Teams
        let path_str = path.to_string_lossy();
        let path_w: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
        let df_size = core::mem::size_of::<DropFiles>();
        let total = df_size + path_w.len() * 2 + 2;
        let hmem = GlobalAlloc(GMEM_MOVEABLE, total).unwrap_or_default();
        if !hmem.0.is_null() {
            let ptr = GlobalLock(hmem) as *mut u8;
            let df = ptr as *mut DropFiles;
            (*df).p_files = df_size as u32;
            (*df).pt_x = 0; (*df).pt_y = 0;
            (*df).f_nc = 0; (*df).f_wide = 1;
            let wptr = ptr.add(df_size) as *mut u16;
            core::ptr::copy_nonoverlapping(path_w.as_ptr(), wptr, path_w.len());
            *wptr.add(path_w.len()) = 0;
            GlobalUnlock(hmem);
            SetClipboardData(CF_HDROP_ID, HANDLE(hmem.0 as *mut _));
        }

        // 2. CF_UNICODETEXT — path for Claude CLI / other text consumers
        let text_w: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
        let text_mem = GlobalAlloc(GMEM_MOVEABLE, text_w.len() * 2).unwrap_or_default();
        if !text_mem.0.is_null() {
            let ptr = GlobalLock(text_mem) as *mut u16;
            core::ptr::copy_nonoverlapping(text_w.as_ptr(), ptr, text_w.len());
            GlobalUnlock(text_mem);
            SetClipboardData(CF_UNICODETEXT_ID, HANDLE(text_mem.0 as *mut _));
        }

        CloseClipboard();
    }
}

fn local_appdata_path() -> std::path::PathBuf {
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_LOCAL_APP_DATA, KF_FLAG_DEFAULT, None) {
            let s = p.to_string().unwrap_or_default();
            return std::path::PathBuf::from(s).join("snagit-clip-to-file").join("png");
        }
    }
    std::env::var("LOCALAPPDATA").map(|p| std::path::PathBuf::from(p)
        .join("snagit-clip-to-file").join("png")).unwrap_or_else(|_| std::path::PathBuf::from("png"))
}

// ── Minimal PNG encoder (no external deps) ──────────────────────────────────

fn write_rgba_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut out = Vec::with_capacity(1024 + rgba.len());
    // PNG signature
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    // IHDR
    write_chunk(&mut out, b"IHDR", &{
        let mut h = [0u8; 13];
        h[0..4].copy_from_slice(&width.to_be_bytes());
        h[4..8].copy_from_slice(&height.to_be_bytes());
        h[8]  = 8;  // bit depth
        h[9]  = 6;  // RGBA
        h[10] = 0; h[11] = 0; h[12] = 0;
        h
    });
    // IDAT — filter type 0 (None) per row
    let row_bytes = (width * 4) as usize;
    let mut raw = Vec::with_capacity((row_bytes + 1) * height as usize);
    for row in 0..height as usize {
        raw.push(0); // filter type None
        raw.extend_from_slice(&rgba[row * row_bytes..(row + 1) * row_bytes]);
    }
    let compressed = zlib_compress(&raw);
    write_chunk(&mut out, b"IDAT", &compressed);
    write_chunk(&mut out, b"IEND", &[]);
    std::fs::write(path, &out)
}

fn write_chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    let len = (data.len() as u32).to_be_bytes();
    out.extend_from_slice(&len);
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    let crc = crc32(&[&name[..], data].concat());
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffffffffu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 { crc = if crc & 1 != 0 { 0xedb88320 ^ (crc >> 1) } else { crc >> 1 }; }
    }
    !crc
}

fn zlib_compress(data: &[u8]) -> Vec<u8> {
    // zlib header (CMF=0x78 deflate window=32k, FLG=0x9C check)
    let mut out = vec![0x78u8, 0x9C];
    // DEFLATE non-compressed blocks (type 00) for simplicity
    let mut adler_s1 = 1u32;
    let mut adler_s2 = 0u32;
    for chunk in data.chunks(65535) {
        let last = chunk.as_ptr_range().end == data.as_ptr_range().end;
        out.push(if last { 1 } else { 0 }); // BFINAL | BTYPE=00
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
        for &b in chunk {
            adler_s1 = (adler_s1 + b as u32) % 65521;
            adler_s2 = (adler_s2 + adler_s1) % 65521;
        }
    }
    let adler = (adler_s2 << 16) | adler_s1;
    out.extend_from_slice(&adler.to_be_bytes());
    out
}
