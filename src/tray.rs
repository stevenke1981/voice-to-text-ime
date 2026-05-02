use tray_icon::{
    menu::{Menu, MenuItem, MenuId},
    TrayIcon, TrayIconBuilder,
};

pub struct TrayHandle {
    pub icon: TrayIcon,
    pub quit_id: MenuId,
    pub settings_id: MenuId,
}

pub fn setup_tray() -> TrayHandle {
    let tray_menu = Menu::new();
    let settings_i = MenuItem::new("設定 (Settings)", true, None);
    let quit_i = MenuItem::new("離開 (Quit)", true, None);
    let settings_id = settings_i.id().clone();
    let quit_id = quit_i.id().clone();
    let _ = tray_menu.append_items(&[&settings_i, &quit_i]);

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Voice-to-Text IME\n按住 Alt 說話")
        .with_icon(create_mic_icon())
        .build()
        .unwrap();

    TrayHandle { icon: tray, quit_id, settings_id }
}

fn create_mic_icon() -> tray_icon::Icon {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let cx = SIZE as f32 / 2.0;
    let cy = SIZE as f32 / 2.0;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let i = ((y * SIZE + x) * 4) as usize;
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            // Outer circle (dark bg)
            if dist <= 15.0 {
                rgba[i]   = 30;
                rgba[i+1] = 30;
                rgba[i+2] = 40;
                rgba[i+3] = 230;
            }
            // Mic body: narrow vertical rectangle in center
            if dx.abs() < 4.0 && dy > -9.0 && dy < 3.0 {
                rgba[i]   = 80;
                rgba[i+1] = 180;
                rgba[i+2] = 255;
                rgba[i+3] = 255;
            }
            // Mic arc: ring below center
            if dist > 7.0 && dist < 9.5 && dy > 0.0 && dy < 8.0 {
                rgba[i]   = 80;
                rgba[i+1] = 180;
                rgba[i+2] = 255;
                rgba[i+3] = 255;
            }
            // Mic stand: thin vertical line below arc
            if dx.abs() < 1.5 && dy > 8.0 && dy < 12.0 {
                rgba[i]   = 80;
                rgba[i+1] = 180;
                rgba[i+2] = 255;
                rgba[i+3] = 255;
            }
            // Mic base: short horizontal line at bottom
            if dx.abs() < 4.0 && dy > 11.0 && dy < 13.0 {
                rgba[i]   = 80;
                rgba[i+1] = 180;
                rgba[i+2] = 255;
                rgba[i+3] = 255;
            }
        }
    }

    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).unwrap()
}
