use std::{thread, time, mem};
use windows::{
    core::Result,
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::{
            Gdi::{
                BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
                GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
                DIB_RGB_COLORS, RGBQUAD, SRCCOPY, HGDIOBJ,
            },
        },
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_MOVE,
                MOUSEINPUT, VIRTUAL_KEY,
            },
            WindowsAndMessaging::{GetForegroundWindow, GetWindowRect},
        },
    },
};
use rand::{thread_rng, Rng};
use chrono::prelude::*;

// No topo do main.rs
mod driver_interface;
use driver_interface::{hide_current_process, protect_memory_region, is_system_secure};

// Configurações avançadas
const ACTIVATE_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0x50); // P
const RECOIL_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0x4F);  // O
const WEAPON_SWITCH_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0x55); // U
const GAME_WINDOW_TITLE: &str = "Valorant";

struct Config {
    target_color: (u8, u8, u8),
    color_tolerance: u8,
    scan_area: (i32, i32),
    lock_power: f32,
    fov_radius: f32,
    human_smoothness: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_color: (40, 40, 200), // BGR ajustado para exemplo
            color_tolerance: 40,
            scan_area: (50, 50),
            lock_power: 0.85,
            fov_radius: 150.0,
            human_smoothness: 3.5,
        }
    }
}

struct Aimbot {
    active: bool,
    recoil_active: bool,
    current_weapon: String,
    config: Config,
    last_shot_time: Option<DateTime<Utc>>,
    vandal_pattern: Vec<(f32, f32)>,
    phantom_pattern: Vec<(f32, f32)>,
    game_window: HWND,
}

impl Aimbot {
    fn new() -> Result<Self> {
        let mut aimbot = Self {
            active: false,
            recoil_active: false,
            current_weapon: "vandal".to_string(),
            config: Config::default(),
            last_shot_time: None,
            vandal_pattern: vec![
                (0.0, 1.2), (0.1, 1.5), (0.3, 1.8), (0.5, 2.0), 
                (0.7, 2.2), (0.9, 2.5), (1.1, 2.8), (1.3, 3.0),
            ],
            phantom_pattern: vec![
                (0.0, 0.8), (0.05, 1.0), (0.1, 1.2), (0.15, 1.4),
                (0.2, 1.6), (0.25, 1.8), (0.3, 2.0), (0.35, 2.2),
            ],
            game_window: unsafe { GetForegroundWindow() },
        };
        aimbot.update_game_window()?;
        Ok(aimbot)
    }

    fn update_game_window(&mut self) -> Result<()> {
        unsafe {
            // Lógica para encontrar a janela do jogo automaticamente
            self.game_window = GetForegroundWindow();
        }
        Ok(())
    }

    fn run(&mut self) -> Result<()> {
        println!("Valorant Aimbot (Rust Edition)");
        println!("P - Ativar/Desativar Aimbot");
        println!("O - Ativar/Desativar Controle de Recuo");
        println!("U - Alternar entre Vandal/Phantom");

        loop {
            self.handle_input()?;
            
            if self.active {
                self.update_game_window()?;
                if let Err(e) = self.aimbot_process() {
                    eprintln!("Erro no aimbot: {:?}", e);
                }
            }

            if self.recoil_active {
                self.recoil_control()?;
            }

            thread::sleep(time::Duration::from_millis(2));
        }
    }

    fn handle_input(&mut self) -> Result<()> {
        unsafe {
            if GetAsyncKeyState(ACTIVATE_KEY.0 as i32) & 1 != 0 {
                self.active = !self.active;
                println!("Aimbot {}", if self.active { "ativado" } else { "desativado" });
                thread::sleep(time::Duration::from_millis(300));
            }

            if GetAsyncKeyState(RECOIL_KEY.0 as i32) & 1 != 0 {
                self.recoil_active = !self.recoil_active;
                println!("Controle de recuo {}", if self.recoil_active { "ativado" } else { "desativado" });
                thread::sleep(time::Duration::from_millis(300));
            }

            if GetAsyncKeyState(WEAPON_SWITCH_KEY.0 as i32) & 1 != 0 {
                self.current_weapon = if self.current_weapon == "vandal" {
                    "phantom".to_string()
                } else {
                    "vandal".to_string()
                };
                println!("Arma alterada para: {}", self.current_weapon);
                thread::sleep(time::Duration::from_millis(300));
            }
        }
        Ok(())
    }

    fn aimbot_process(&mut self) -> Result<()> {
        let (center_x, center_y) = self.get_game_center()?;
        let (scan_w, scan_h) = self.config.scan_area;
        let capture_rect = (
            center_x - scan_w / 2,
            center_y - scan_h / 2,
            scan_w,
            scan_h,
        );

        let pixels = self.capture_screen(capture_rect)?;
        let targets = self.detect_targets(&pixels, scan_w, scan_h);

        if !targets.is_empty() {
            let target = self.select_target(targets, center_x, center_y);
            self.move_to_target(target, center_x, center_y)?;
        }

        Ok(())
    }

    fn get_game_center(&self) -> Result<(i32, i32)> {
        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(self.game_window, &mut rect);
            Ok((
                (rect.left + rect.right) / 2,
                (rect.top + rect.bottom) / 2,
            ))
        }
    }

    fn capture_screen(&self, rect: (i32, i32, i32, i32)) -> Result<Vec<u8>> {
        let (x, y, w, h) = rect;
        unsafe {
            let hdc = GetDC(None);
            let hdc_mem = CreateCompatibleDC(hdc);
            let hbm = CreateCompatibleBitmap(hdc, w, h);
            
            // Fixed: Proper type casting for SelectObject
            let _old_obj = SelectObject(hdc_mem, HGDIOBJ(hbm.0));

            BitBlt(hdc_mem, 0, 0, w, h, hdc, x, y, SRCCOPY);

            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as _,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0,
                    ..Default::default()
                },
                bmiColors: [RGBQUAD::default(); 1],
            };

            let mut buffer = vec![0u8; (w * h * 4) as usize];
            GetDIBits(hdc_mem, hbm, 0, h as _, Some(buffer.as_mut_ptr() as _), &mut bmi, DIB_RGB_COLORS);

            DeleteObject(hbm);
            DeleteDC(hdc_mem);
            ReleaseDC(None, hdc);

            Ok(buffer)
        }
    }

    fn detect_targets(&self, pixels: &[u8], width: i32, height: i32) -> Vec<(i32, i32)> {
        let mut targets = Vec::new();
        let (target_b, target_g, target_r) = self.config.target_color;
        let tolerance = self.config.color_tolerance as i16;

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let (b, g, r) = (pixels[idx], pixels[idx + 1], pixels[idx + 2]);

                if (b as i16 - target_b as i16).abs() <= tolerance &&
                   (g as i16 - target_g as i16).abs() <= tolerance &&
                   (r as i16 - target_r as i16).abs() <= tolerance {
                    targets.push((x, y));
                }
            }
        }
        targets
    }

    fn select_target(&self, targets: Vec<(i32, i32)>, center_x: i32, center_y: i32) -> (i32, i32) {
        targets.iter()
            .map(|(x, y)| (center_x - self.config.scan_area.0 / 2 + x, center_y - self.config.scan_area.1 / 2 + y))
            .min_by_key(|(x, y)| {
                let dx = x - center_x;
                let dy = y - center_y;
                (dx * dx + dy * dy) as i32
            })
            .unwrap_or((center_x, center_y))
    }

    fn move_to_target(&self, target: (i32, i32), center_x: i32, center_y: i32) -> Result<()> {
        let dx = (target.0 - center_x) as f32;
        let dy = (target.1 - center_y) as f32;
        let distance = (dx * dx + dy * dy).sqrt();

        if distance > self.config.fov_radius {
            return Ok(());
        }

        let smoothed_dx = dx * self.config.lock_power / self.config.human_smoothness;
        let smoothed_dy = dy * self.config.lock_power / self.config.human_smoothness;

        self.move_mouse(smoothed_dx, smoothed_dy)
    }

    fn recoil_control(&mut self) -> Result<()> {
        // Fixed: Use u16 for bitmask to avoid integer overflow
        let is_firing = unsafe { GetAsyncKeyState(0x01) & 0x8000u16 as i16 != 0 };

        if is_firing {
            let now = Utc::now();
            let last_shot = self.last_shot_time.unwrap_or(now);
            let duration = now.signed_duration_since(last_shot);
            let pattern = self.get_recoil_pattern(duration.num_milliseconds());

            let mut rng = thread_rng();
            let jitter_x = rng.gen_range(-0.1..0.1);
            let jitter_y = rng.gen_range(-0.05..0.05);

            self.move_mouse(pattern.0 + jitter_x, pattern.1 + jitter_y)?;
            self.last_shot_time = Some(now);
        } else {
            self.last_shot_time = None;
        }
        Ok(())
    }

    fn get_recoil_pattern(&self, duration_ms: i64) -> (f32, f32) {
        let step = (duration_ms / 75) as usize;
        match self.current_weapon.as_str() {
            "vandal" => self.vandal_pattern.get(step).copied().unwrap_or((0.0, 0.0)),
            "phantom" => self.phantom_pattern.get(step).copied().unwrap_or((0.0, 0.0)),
            _ => (0.0, 0.0),
        }
    }

    fn move_mouse(&self, dx: f32, dy: f32) -> Result<()> {
        let mut input = INPUT::default();
        
        // Set type to mouse input
        input.r#type = INPUT_MOUSE;
        
        // Set mouse input data
        input.Anonymous.mi = MOUSEINPUT {
            dx: dx.round() as i32,
            dy: dy.round() as i32,
            mouseData: 0,
            dwFlags: MOUSEEVENTF_MOVE,
            time: 0,
            dwExtraInfo: 0,
        };

        // Fixed: Correct SendInput parameters
        unsafe {
            SendInput(&[input], mem::size_of::<INPUT>() as i32);
        }
        
        Ok(())
    }

    // Dentro da função main ou em outro ponto adequado
    fn main() -> Result<()> {
            // Inicializar o driver e ocultar o processo
            if let Err(e) = hide_current_process() {
                // Tratamento de erro silencioso ou log discreto
                eprintln!("Erro de inicialização: {:?}", e);
            }
            
            let mut aimbot = Aimbot::new()?;
            aimbot.run()
        }
    }

fn main() -> Result<()> {
    let mut aimbot = Aimbot::new()?;
    aimbot.run()
}