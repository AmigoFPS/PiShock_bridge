use eframe::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Id, Layout, Margin, Order, Painter,
    Rect, RichText, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui, epaint::Shadow, pos2,
    style::HandleShape, vec2,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub type Lch = [f32; 3];

pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub colors: Colors,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Colors {
    pub base: Lch,
    pub contrast: Lch,
    pub accent: Lch,
}

const fn theme(
    id: &'static str,
    name: &'static str,
    base: Lch,
    contrast: Lch,
    accent: Lch,
) -> Theme {
    Theme {
        id,
        name,
        colors: Colors {
            base,
            contrast,
            accent,
        },
    }
}

pub const THEMES: [Theme; 9] = [
    theme(
        "terminal",
        "Terminal",
        [0.17, 0.012, 265.0],
        [0.86, 0.015, 265.0],
        [0.80, 0.16, 158.0],
    ),
    theme(
        "ember",
        "Ember",
        [0.16, 0.022, 40.0],
        [0.87, 0.020, 50.0],
        [0.72, 0.18, 42.0],
    ),
    theme(
        "flare",
        "Flare",
        [0.15, 0.016, 95.0],
        [0.88, 0.020, 95.0],
        [0.86, 0.17, 96.0],
    ),
    theme(
        "cobalt",
        "Cobalt",
        [0.16, 0.022, 250.0],
        [0.87, 0.020, 250.0],
        [0.74, 0.15, 235.0],
    ),
    theme(
        "orchid",
        "Orchid",
        [0.16, 0.022, 330.0],
        [0.88, 0.015, 330.0],
        [0.75, 0.19, 340.0],
    ),
    theme(
        "wire",
        "Wire",
        [0.13, 0.0, 0.0],
        [0.74, 0.0, 0.0],
        [0.98, 0.0, 0.0],
    ),
    theme(
        "void",
        "Void",
        [0.06, 0.0, 0.0],
        [0.92, 0.010, 300.0],
        [0.72, 0.20, 300.0],
    ),
    theme(
        "paper",
        "Paper",
        [0.97, 0.006, 95.0],
        [0.26, 0.012, 95.0],
        [0.52, 0.14, 155.0],
    ),
    theme(
        "blueprint",
        "Blueprint",
        [0.95, 0.012, 240.0],
        [0.28, 0.030, 250.0],
        [0.50, 0.16, 245.0],
    ),
];

pub fn default_theme(light: bool) -> usize {
    theme_index(if light { "paper" } else { "terminal" }).unwrap_or(0)
}

pub fn theme_index(id: &str) -> Option<usize> {
    THEMES.iter().position(|t| t.id == id)
}

pub struct Palette {
    pub base: Color32,
    pub contrast: Color32,
    pub accent: Color32,
    pub dim: Color32,
    pub hair: Color32,
    pub faint: Color32,
    pub sheet: Color32,
    pub dark: bool,
}

impl Colors {
    pub fn palette(&self) -> Palette {
        let (base, contrast, accent) = (oklab(self.base), oklab(self.contrast), oklab(self.accent));
        let base_rgb = srgb(base);
        Palette {
            base: base_rgb,
            contrast: srgb(contrast),
            accent: srgb(accent),
            dim: srgb(mix(base, contrast, 0.58)),
            hair: srgb(mix(base, contrast, 0.42)),
            faint: srgb(mix(base, contrast, 0.08)),
            sheet: Color32::from_rgba_unmultiplied(base_rgb.r(), base_rgb.g(), base_rgb.b(), 219),
            dark: self.base[0] < 0.5,
        }
    }

    pub fn lerp(from: &Colors, to: &Colors, t: f32) -> Colors {
        let lch = |a: Lch, b: Lch| {
            let d = ((b[2] - a[2]) % 360.0 + 540.0) % 360.0 - 180.0;
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + d * t,
            ]
        };
        Colors {
            base: lch(from.base, to.base),
            contrast: lch(from.contrast, to.contrast),
            accent: lch(from.accent, to.accent),
        }
    }
}

fn oklab([l, c, h]: Lch) -> [f32; 3] {
    let h = h.to_radians();
    [l, c * h.cos(), c * h.sin()]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn srgb([l, a, b]: [f32; 3]) -> Color32 {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let encode = |x: f32| {
        let x = x.max(0.0);
        let x = if x <= 0.003_130_8 {
            12.92 * x
        } else {
            1.055 * x.powf(1.0 / 2.4) - 0.055
        };
        (x.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    Color32::from_rgb(
        encode(4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_93 * s_),
        encode(-1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_4 * s_),
        encode(-0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_),
    )
}

pub struct Fade {
    from: Colors,
    to: Colors,
    started: Option<Instant>,
}

impl Fade {
    const LENGTH: Duration = Duration::from_millis(450);

    pub fn new(colors: Colors) -> Self {
        Self {
            from: colors,
            to: colors,
            started: None,
        }
    }

    pub fn start(&mut self, to: Colors, animate: bool) {
        self.from = self.current();
        self.to = to;
        self.started = animate.then(Instant::now);
    }

    pub fn current(&self) -> Colors {
        let Some(started) = self.started else {
            return self.to;
        };
        let t = (started.elapsed().as_secs_f32() / Self::LENGTH.as_secs_f32()).min(1.0);
        Colors::lerp(&self.from, &self.to, t * t * (3.0 - 2.0 * t))
    }

    pub fn animating(&mut self) -> bool {
        match self.started {
            Some(started) if started.elapsed() < Self::LENGTH => true,
            Some(_) => {
                self.started = None;
                false
            }
            None => false,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Prefs {
    pub theme: usize,
    pub speed: f32,
    pub density: f32,
}

impl Prefs {
    const FILE: &str = "settings.json";

    pub fn load(light: bool) -> Self {
        let saved: Value = std::fs::read_to_string(Self::FILE)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        Self::from_json(&saved, light)
    }

    fn from_json(saved: &Value, light: bool) -> Self {
        let number = |key: &str, default: f32, max: f32| {
            saved[key]
                .as_f64()
                .map(|v| (v as f32).clamp(0.0, max))
                .unwrap_or(default)
        };
        Self {
            theme: saved["theme"]
                .as_str()
                .and_then(theme_index)
                .unwrap_or_else(|| default_theme(light)),
            speed: number("speed", 1.0, 3.0),
            density: number("density", 1.0, 2.0),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "theme": THEMES[self.theme].id,
            "speed": (self.speed * 10.0).round() / 10.0,
            "density": (self.density * 10.0).round() / 10.0,
        })
    }

    pub fn save(&self) {
        let _ = std::fs::write(Self::FILE, self.to_json().to_string());
    }
}

pub fn stroke(color: Color32) -> Stroke {
    Stroke::new(1.0_f32, color)
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn apply(ctx: &egui::Context, colors: &Colors) {
    let p = colors.palette();
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Small, mono(12.0)),
        (TextStyle::Body, mono(15.0)),
        (TextStyle::Button, mono(13.0)),
        (TextStyle::Heading, mono(20.0)),
        (TextStyle::Monospace, mono(15.0)),
    ]
    .into();
    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(9.0, 4.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.slider_width = 120.0;
    style.spacing.icon_width = 16.0;

    let v = &mut style.visuals;
    *v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.contrast);
    v.weak_text_color = Some(p.dim);
    v.panel_fill = p.base;
    v.window_fill = p.base;
    v.extreme_bg_color = p.faint;
    v.faint_bg_color = p.faint;
    v.code_bg_color = p.faint;
    v.window_stroke = stroke(p.hair);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.accent;
    v.error_fg_color = p.contrast;
    v.selection.bg_fill = p.accent;
    v.selection.stroke = stroke(p.base);
    v.slider_trailing_fill = true;
    v.handle_shape = HandleShape::Rect { aspect_ratio: 0.6 };
    v.striped = false;
    v.text_cursor.stroke.color = p.accent;

    let w = &mut v.widgets;
    for widget in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        widget.corner_radius = CornerRadius::ZERO;
        widget.expansion = 0.0;
    }
    w.noninteractive.bg_fill = p.base;
    w.noninteractive.weak_bg_fill = p.base;
    w.noninteractive.bg_stroke = stroke(p.hair);
    w.noninteractive.fg_stroke = stroke(p.contrast);
    w.inactive.bg_fill = p.faint;
    w.inactive.weak_bg_fill = p.base;
    w.inactive.bg_stroke = stroke(p.hair);
    w.inactive.fg_stroke = stroke(p.contrast);
    w.hovered.bg_fill = p.faint;
    w.hovered.weak_bg_fill = p.faint;
    w.hovered.bg_stroke = stroke(p.accent);
    w.hovered.fg_stroke = stroke(p.accent);
    w.active.bg_fill = p.accent;
    w.active.weak_bg_fill = p.accent;
    w.active.bg_stroke = stroke(p.accent);
    w.active.fg_stroke = stroke(p.base);
    w.open = w.hovered;
    ctx.set_style(style);
}

pub fn heading(ui: &mut Ui, text: &str, p: &Palette) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_uppercase(), mono(15.0), p.base);
    let block = galley.size() + vec2(18.0, 10.0);
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), block.y + 8.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0, p.sheet);
    painter.rect_stroke(rect, 0, stroke(p.contrast), StrokeKind::Inside);
    let span = Rect::from_min_size(rect.min + vec2(4.0, 4.0), block);
    painter.rect_filled(span, 0, p.contrast);
    painter.galley(span.min + vec2(9.0, 5.0), galley, p.base);
    let strip = Rect::from_min_max(
        pos2(span.max.x + 4.0, rect.min.y + 1.0),
        rect.max - vec2(1.0, 1.0),
    );
    painter.line_segment([strip.left_top(), strip.left_bottom()], stroke(p.contrast));
    hatch(painter, strip, p.contrast);
    ui.add_space(6.0);
}

pub fn title(ui: &mut Ui, text: &str, p: &Palette) {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(mono(12.0))
            .color(p.dim),
    );
}

pub fn hatch(painter: &Painter, rect: Rect, color: Color32) {
    let painter = painter.with_clip_rect(rect);
    let pitch = 10.0 * std::f32::consts::SQRT_2;
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [pos2(x, rect.bottom()), pos2(x + rect.height(), rect.top())],
            stroke(color),
        );
        x += pitch;
    }
}

pub fn dashed(painter: &Painter, from: egui::Pos2, to: egui::Pos2, color: Color32) {
    painter.add(Shape::dashed_line(&[from, to], stroke(color), 3.0, 3.0));
}

pub fn dashed_rect(painter: &Painter, rect: Rect, color: Color32) {
    let rect = rect.shrink(0.5);
    dashed(painter, rect.left_top(), rect.right_top(), color);
    dashed(painter, rect.right_top(), rect.right_bottom(), color);
    dashed(painter, rect.right_bottom(), rect.left_bottom(), color);
    dashed(painter, rect.left_bottom(), rect.left_top(), color);
}

pub fn dashed_separator(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 11.0), Sense::hover());
    let y = rect.center().y;
    dashed(
        ui.painter(),
        pos2(rect.left(), y),
        pos2(rect.right(), y),
        p.hair,
    );
}

pub fn sheet<R>(ui: &mut Ui, p: &Palette, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let response = egui::Frame::NONE
        .fill(p.sheet)
        .inner_margin(Margin::same(14))
        .show(ui, add_contents);
    dashed_rect(ui.painter(), response.response.rect, p.hair);
    response.inner
}

#[derive(Default)]
pub struct PanelEvents {
    pub theme: Option<usize>,
    pub prefs_changed: bool,
    pub close: bool,
}

pub fn panel(ctx: &egui::Context, prefs: &mut Prefs, p: &Palette) -> PanelEvents {
    let mut events = PanelEvents::default();
    let width = 340.0_f32.min(ctx.content_rect().width() - 40.0);
    let x = ctx.content_rect().right() - 20.0 - width;
    egui::Area::new(Id::new("theme-panel"))
        .order(Order::Foreground)
        .fixed_pos(pos2(x, 57.0))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (bar, _) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
            ui.painter().rect_filled(bar, 0, p.base);
            hatch(ui.painter(), bar.shrink(1.0), p.contrast);
            dashed_rect(ui.painter(), bar, p.hair);
            let close = Rect::from_min_size(
                pos2(bar.right() - 14.0 - 62.0, bar.center().y - 12.0),
                vec2(62.0, 24.0),
            );
            ui.painter().rect_filled(close, 0, p.base);
            let button = egui::Button::new(RichText::new("CLOSE").font(mono(12.0)).color(p.base))
                .fill(p.contrast)
                .stroke(stroke(p.contrast));
            if ui.put(close.shrink(3.0), button).clicked() {
                events.close = true;
            }
            let body = egui::Frame::NONE
                .fill(p.base)
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(width - 32.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    title(ui, "Palette", p);
                    ui.add_space(10.0);
                    for (i, theme) in THEMES.iter().enumerate() {
                        if theme_row(ui, theme, i == prefs.theme, p) {
                            events.theme = Some(i);
                        }
                    }
                    ui.add_space(12.0);
                    dashed_separator(ui, p);
                    ui.add_space(6.0);
                    ui.spacing_mut().item_spacing.y = 10.0;
                    for (label, value, max) in [
                        ("Speed", &mut prefs.speed, 3.0),
                        ("Density", &mut prefs.density, 2.0),
                    ] {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                vec2(74.0, 20.0),
                                Layout::left_to_right(Align::Center),
                                |ui| title(ui, label, p),
                            );
                            ui.spacing_mut().slider_width = width - 32.0 - 74.0 - 8.0 - 52.0;
                            let slider = egui::Slider::new(value, 0.0..=max)
                                .step_by(0.1)
                                .fixed_decimals(1);
                            events.prefs_changed |= ui.add(slider).changed();
                        });
                    }
                });
            let rect = body.response.rect;
            dashed(ui.painter(), rect.left_top(), rect.left_bottom(), p.hair);
            dashed(ui.painter(), rect.right_top(), rect.right_bottom(), p.hair);
            dashed(
                ui.painter(),
                rect.left_bottom(),
                rect.right_bottom(),
                p.hair,
            );
        });
    events
}

fn theme_row(ui: &mut Ui, theme: &Theme, active: bool, p: &Palette) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    let lit = active || response.hovered();
    let color = if lit { p.accent } else { p.contrast };
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(theme.name.to_owned(), mono(13.0), color);
    painter.galley(
        pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
        galley.clone(),
        color,
    );
    let swatches = 3.0 * 16.0 + 2.0 * 6.0;
    let rule_from = rect.left() + galley.size().x + 6.0;
    let rule_to = rect.right() - swatches - 6.0;
    let hr = if lit { p.accent } else { p.hair };
    dashed(
        painter,
        pos2(rule_from, rect.center().y),
        pos2(rule_to, rect.center().y),
        hr,
    );
    let colors = theme.colors.palette();
    for (i, fill) in [colors.base, colors.contrast, colors.accent]
        .into_iter()
        .enumerate()
    {
        let x = rect.right() - swatches + i as f32 * 22.0;
        let sw = Rect::from_min_size(pos2(x, rect.center().y - 8.0), vec2(16.0, 16.0));
        painter.rect_stroke(sw, 0, stroke(hr), StrokeKind::Inside);
        painter.rect_filled(sw.shrink(3.0), 0, fill);
    }
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    response.clicked()
}
