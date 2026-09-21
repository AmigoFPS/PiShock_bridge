use crate::theme::Palette;
use eframe::egui::{Color32, Painter, Rect, Vec2, pos2};
use std::time::Instant;

const NEAR: f32 = 0.34;
const FAR: f32 = 6.4;
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 3),
    (3, 2),
    (2, 0),
    (4, 5),
    (5, 7),
    (7, 6),
    (6, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

struct Box3 {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    h: f32,
    d: f32,
    rx: f32,
    ry: f32,
    rz: f32,
    vx: f32,
    vy: f32,
    vz: f32,
    accent: bool,
}

pub struct Field {
    boxes: Vec<Box3>,
    size: Vec2,
    focal: f32,
    spread: Vec2,
    last: Option<Instant>,
}

fn rand(a: f32, b: f32) -> f32 {
    a + fastrand::f32() * (b - a)
}

impl Field {
    pub fn new() -> Self {
        Self {
            boxes: Vec::new(),
            size: Vec2::ZERO,
            focal: 0.0,
            spread: Vec2::ZERO,
            last: None,
        }
    }

    fn make(&self, anywhere: bool) -> Box3 {
        let s = rand(0.09, 0.30);
        Box3 {
            x: rand(-self.spread.x, self.spread.x),
            y: rand(-self.spread.y, self.spread.y),
            z: if anywhere {
                rand(NEAR, FAR)
            } else {
                rand(FAR * 0.82, FAR)
            },
            w: s * rand(0.55, 1.7),
            h: s * rand(0.55, 1.7),
            d: s * rand(0.55, 1.7),
            rx: rand(0.0, std::f32::consts::TAU),
            ry: rand(0.0, std::f32::consts::TAU),
            rz: rand(0.0, std::f32::consts::TAU),
            vx: rand(-0.22, 0.22),
            vy: rand(-0.22, 0.22),
            vz: rand(-0.14, 0.14),
            accent: fastrand::f32() < 0.18,
        }
    }

    pub fn advance(&mut self, size: Vec2, speed: f32, density: f32) {
        if size != self.size {
            self.size = size;
            self.focal = size.x.min(size.y) * 0.95;
            self.spread = Vec2::new(
                size.x * FAR / (2.0 * self.focal) * 1.2,
                size.y * FAR / (2.0 * self.focal) * 1.2,
            );
        }
        let area = size.x * size.y;
        let want = ((area / 26000.0).clamp(10.0, 90.0) * density).round() as usize;
        while self.boxes.len() < want {
            let b = self.make(true);
            self.boxes.push(b);
        }
        self.boxes.truncate(want);

        let now = Instant::now();
        let dt = self
            .last
            .map(|t| now.duration_since(t).as_secs_f32().min(0.05))
            .unwrap_or(0.0);
        self.last = Some(now);
        if speed <= 0.0 {
            return;
        }
        for i in 0..self.boxes.len() {
            let b = &mut self.boxes[i];
            b.z -= 0.5 * speed * dt;
            b.rx += b.vx * dt;
            b.ry += b.vy * dt;
            b.rz += b.vz * dt;
            if b.z <= NEAR {
                self.boxes[i] = self.make(false);
            }
        }
    }

    pub fn draw(&self, painter: &Painter, rect: Rect, p: &Palette) {
        let (cx, cy, f) = (rect.center().x, rect.center().y, self.focal);
        for b in &self.boxes {
            if b.z <= NEAR {
                continue;
            }
            let alpha = ((FAR - b.z) / 1.3).min(1.0) * ((b.z - NEAR) / 0.9).min(1.0);
            if alpha <= 0.01 {
                continue;
            }
            let alpha = alpha * 0.5;
            let (sx, cx1) = b.rx.sin_cos();
            let (sy, cy1) = b.ry.sin_cos();
            let (sz, cz1) = b.rz.sin_cos();
            let (hw, hh, hd) = (b.w / 2.0, b.h / 2.0, b.d / 2.0);
            let mut points = [pos2(0.0, 0.0); 8];
            let mut visible = true;
            for (v, point) in points.iter_mut().enumerate() {
                let x = if v & 1 != 0 { hw } else { -hw };
                let y = if v & 2 != 0 { hh } else { -hh };
                let z = if v & 4 != 0 { hd } else { -hd };
                let (y1, z1) = (y * cx1 - z * sx, y * sx + z * cx1);
                let (x2, z2) = (x * cy1 + z1 * sy, -x * sy + z1 * cy1);
                let (x3, y3) = (x2 * cz1 - y1 * sz, x2 * sz + y1 * cz1);
                let wz = b.z + z2;
                if wz <= 0.05 {
                    visible = false;
                    break;
                }
                *point = pos2(cx + (b.x + x3) / wz * f, cy + (b.y + y3) / wz * f);
            }
            if !visible {
                continue;
            }
            let color = if b.accent { p.accent } else { p.contrast };
            let color = Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                (alpha * 255.0) as u8,
            );
            let width = (1.1 / b.z).clamp(0.5, 1.6);
            let stroke = eframe::egui::Stroke::new(width, color);
            for (a, z) in EDGES {
                painter.line_segment([points[a], points[z]], stroke);
            }
        }
    }
}