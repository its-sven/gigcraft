//! GigCraft's look: a calm, modern interface with a light touch of stage gear.
//!
//! - Charcoal (dark) or brushed-aluminium grey (light) surfaces, following the
//!   system setting.
//! - One warm amber "LED" accent for selection and active controls; chords stay
//!   a cool blue so they never compete with the lyrics or the accent.
//! - Values the musician reads at a glance (key, tempo, position, song number)
//!   sit in small display readouts: a dark window, monospace digits and a tiny
//!   engraved label above, like the screen on a pedal or a mixer channel.
//! - Status lights (connection, ready, devices) are small LEDs.
//!
//! Contrast of text on surfaces is at least WCAG AA (4.5:1) in both themes.

use egui::{Color32, CornerRadius, Stroke, Theme, Vec2, Visuals};

/// The colours of one theme.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub stroke: Color32,
    pub text: Color32,
    pub dim: Color32,
    /// The amber LED accent.
    pub accent: Color32,
    /// Text on the accent.
    pub on_accent: Color32,
    pub chord: Color32,
    /// Gig notes and warnings.
    pub note: Color32,
    pub ok: Color32,
    pub bad: Color32,
    /// Display readouts.
    pub screen: Color32,
    pub screen_text: Color32,
    pub screen_label: Color32,
}

pub(crate) const fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: Color32::from_rgb(0x13, 0x14, 0x17),
            panel: Color32::from_rgb(0x1A, 0x1C, 0x20),
            raised: Color32::from_rgb(0x26, 0x28, 0x2E),
            stroke: Color32::from_rgb(0x34, 0x37, 0x3E),
            text: Color32::from_rgb(0xEC, 0xEC, 0xEE),
            dim: Color32::from_rgb(0x9C, 0x9F, 0xA7),
            accent: Color32::from_rgb(0xFF, 0xB0, 0x3A),
            on_accent: Color32::from_rgb(0x1A, 0x12, 0x02),
            chord: Color32::from_rgb(0x6C, 0xC8, 0xFF),
            note: Color32::from_rgb(0xFF, 0xA0, 0x5C),
            ok: Color32::from_rgb(0x3D, 0xD6, 0x7A),
            bad: Color32::from_rgb(0xFF, 0x5F, 0x57),
            screen: Color32::from_rgb(0x0B, 0x0C, 0x0E),
            screen_text: Color32::from_rgb(0xFF, 0xC2, 0x5E),
            screen_label: Color32::from_rgb(0x8A, 0x8D, 0x94),
        }
    } else {
        Palette {
            bg: Color32::from_rgb(0xEE, 0xED, 0xEA),
            panel: Color32::from_rgb(0xF6, 0xF5, 0xF2),
            raised: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            stroke: Color32::from_rgb(0xD3, 0xD1, 0xCB),
            text: Color32::from_rgb(0x17, 0x18, 0x1B),
            dim: Color32::from_rgb(0x5E, 0x61, 0x68),
            accent: Color32::from_rgb(0xE8, 0x8A, 0x00),
            on_accent: Color32::from_rgb(0x1A, 0x12, 0x02),
            chord: Color32::from_rgb(0x00, 0x5F, 0xB8),
            note: Color32::from_rgb(0xB4, 0x4D, 0x00),
            ok: Color32::from_rgb(0x1E, 0x8E, 0x4A),
            bad: Color32::from_rgb(0xC8, 0x2A, 0x22),
            screen: Color32::from_rgb(0x1C, 0x1D, 0x21),
            screen_text: Color32::from_rgb(0xFF, 0xC2, 0x5E),
            screen_label: Color32::from_rgb(0x9A, 0x9C, 0xA2),
        }
    }
}

/// The palette for the theme `ui` is drawn in.
pub(crate) fn pal(ui: &egui::Ui) -> Palette {
    palette(ui.visuals().dark_mode)
}

pub(crate) fn visuals(dark: bool) -> Visuals {
    let p = palette(dark);
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let r8 = CornerRadius::same(8);
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.stroke);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = if dark { p.screen } else { p.raised };
    v.text_edit_bg_color = Some(if dark { p.screen } else { p.raised });
    v.faint_bg_color = p.bg;
    v.code_bg_color = p.bg;
    v.hyperlink_color = p.chord;
    v.warn_fg_color = p.note;
    v.error_fg_color = p.bad;
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, p.on_accent);
    v.slider_trailing_fill = true;
    v.striped = false;
    v.window_highlight_topmost = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.stroke);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r8;

    w.inactive.bg_fill = p.raised;
    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::new(1.0, p.stroke);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r8;

    let hover = if dark {
        Color32::from_rgb(0x30, 0x33, 0x3A)
    } else {
        Color32::from_rgb(0xFB, 0xF4, 0xE8)
    };
    w.hovered.bg_fill = hover;
    w.hovered.weak_bg_fill = hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = r8;
    w.hovered.expansion = 1.0;

    // Pressed: amber rim, text stays bright (egui also uses this colour for
    // strong text, so it must read on every surface).
    w.active.bg_fill = hover;
    w.active.weak_bg_fill = hover;
    w.active.bg_stroke = Stroke::new(1.5, p.accent);
    w.active.fg_stroke = Stroke::new(2.0, p.text);
    w.active.corner_radius = r8;

    w.open.bg_fill = p.raised;
    w.open.weak_bg_fill = p.raised;
    w.open.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = r8;
    v
}

/// Install both themes; egui follows the system setting.
pub(crate) fn install(ctx: &egui::Context) {
    for (theme, dark) in [(Theme::Dark, true), (Theme::Light, false)] {
        ctx.set_visuals_of(theme, visuals(dark));
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.spacing.button_padding = Vec2::new(10.0, 5.0);
            s.spacing.interact_size.y = 26.0;
            s.spacing.window_margin = egui::Margin::same(14);
            s.spacing.menu_margin = egui::Margin::same(8);
        });
    }
}

/// A display readout: tiny engraved label, value in the amber display font.
pub(crate) fn readout(ui: &mut egui::Ui, label: &str, value: &str, size: f32) -> egui::Response {
    let p = pal(ui);
    let r = egui::Frame::new()
        .fill(p.screen)
        .stroke(Stroke::new(1.0, p.stroke))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(label.to_uppercase())
                        .size((size * 0.42).max(9.0))
                        .color(p.screen_label),
                );
                ui.label(
                    egui::RichText::new(value)
                        .monospace()
                        .size(size)
                        .color(p.screen_text),
                );
            });
        })
        .response;
    let text = format!("{label}: {value}");
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text.clone()));
    r
}

/// A status light with a soft halo; `name` is what screen readers say.
pub(crate) fn led(ui: &mut egui::Ui, color: Color32, name: &str) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    let c = rect.center();
    ui.painter()
        .circle_filled(c, 6.0, color.gamma_multiply(0.25));
    ui.painter().circle_filled(c, 3.5, color);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, name));
    r
}

/// A section heading in small engraved capitals, like a label on a panel.
pub(crate) fn section(ui: &mut egui::Ui, text: &str) {
    let p = pal(ui);
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.5)
            .strong()
            .color(p.dim),
    )
    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Heading, true, text));
}

/// A raised panel (cards for parts, devices, tracks).
pub(crate) fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = pal(ui);
    egui::Frame::new()
        .fill(
            p.raised
                .gamma_multiply(if ui.visuals().dark_mode { 0.55 } else { 1.0 }),
        )
        .stroke(Stroke::new(1.0, p.stroke))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(10))
        .show(ui, add)
        .inner
}

/// The primary action of a screen (Save, Perform, Start live session).
pub(crate) fn primary(text: &str) -> egui::Button<'static> {
    let p = palette(true);
    egui::Button::new(
        egui::RichText::new(text.to_string())
            .strong()
            .color(p.on_accent),
    )
    .fill(p.accent)
    .corner_radius(CornerRadius::same(8))
}
