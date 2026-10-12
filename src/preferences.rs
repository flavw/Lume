use eframe::egui;
/// Windows common color dialog; other platforms retain egui's color picker.
pub fn color_button(ui: &mut egui::Ui, label: &str, color: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        let preview = egui::Color32::from_rgb(color[0], color[1], color[2]);
        let text = if color.iter().map(|v| *v as u16).sum::<u16>() > 384 {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        };
        #[cfg(windows)]
        if ui
            .add(egui::Button::new(egui::RichText::new(label).color(text)).fill(preview))
            .clicked()
        {
            use windows_sys::Win32::UI::{
                Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW},
                Input::KeyboardAndMouse::GetActiveWindow,
            };
            let mut custom = [0u32; 16];
            let mut dialog: CHOOSECOLORW = unsafe { std::mem::zeroed() };
            dialog.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
            dialog.hwndOwner = unsafe { GetActiveWindow() };
            dialog.rgbResult = color[0] as u32 | (color[1] as u32) << 8 | (color[2] as u32) << 16;
            dialog.lpCustColors = custom.as_mut_ptr();
            dialog.Flags = CC_FULLOPEN | CC_RGBINIT;
            if unsafe { ChooseColorW(&mut dialog) } != 0 {
                *color = [
                    dialog.rgbResult as u8,
                    (dialog.rgbResult >> 8) as u8,
                    (dialog.rgbResult >> 16) as u8,
                ];
            }
        }
        #[cfg(not(windows))]
        {
            ui.label(egui::RichText::new(label).color(text));
            ui.color_edit_button_srgb(color);
        }
    });
}
