#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod backdrop;
mod osc;
mod pishock;
mod state;
mod theme;

use eframe::egui::{self, Align, Frame, LayerId, Layout, Margin, RichText, Sense, vec2};
use state::{Batch, Pulse, State};
use std::{sync::Arc, time::Duration};
use theme::{Fade, Palette, Prefs, THEMES};
use tokio::sync::mpsc;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Pishock_bridge")
            .with_icon(icon())
            .with_inner_size([920.0, 780.0])
            .with_min_inner_size([720.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Pishock_bridge",
        options,
        Box::new(|cc| {
            let light = cc.egui_ctx.system_theme() == Some(egui::Theme::Light);
            let app = App::new(Prefs::load(light));
            theme::apply(&cc.egui_ctx, &app.fade.current());
            Ok(Box::new(app))
        }),
    )
}

type Outcome = (String, Result<(), String>);

fn icon() -> egui::IconData {
    const RGBA: &[u8] = include_bytes!("../assets/icon.rgba");
    egui::IconData {
        rgba: RGBA.to_vec(),
        width: 64,
        height: 64,
    }
}

struct App {
    network: Option<osc::Network>,
    runtime: tokio::runtime::Runtime,
    api: Option<Arc<pishock::PiShock>>,
    state: State,
    rx: mpsc::Receiver<osc::Event>,
    done_tx: mpsc::Sender<Outcome>,
    done_rx: mpsc::Receiver<Outcome>,
    config_status: String,
    prefs: Prefs,
    fade: Fade,
    field: backdrop::Field,
    panel_open: bool,
}

impl App {
    fn new(prefs: Prefs) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Cannot start async runtime");
        let (tx, rx) = mpsc::channel(256);
        let (done_tx, done_rx) = mpsc::channel(64);
        let mut state = State::default();
        let network = match runtime.block_on(osc::start(tx)) {
            Ok(network) => Some(network),
            Err(error) => {
                state.status = format!("OSC could not start: {error}");
                None
            }
        };
        let fade = Fade::new(THEMES[prefs.theme].colors);
        let mut app = Self {
            network,
            runtime,
            api: None,
            state,
            rx,
            done_tx,
            done_rx,
            config_status: String::new(),
            prefs,
            fade,
            field: backdrop::Field::new(),
            panel_open: false,
        };
        app.reload_config();
        app
    }

    fn reload_config(&mut self) {
        match pishock::PiShock::load() {
            Ok(api) => {
                self.api = api.map(Arc::new);
                self.config_status = if self.api.is_some() {
                    "Credentials loaded"
                } else {
                    "Monitor only: fill in config.json"
                }
                .into();
            }
            Err(error) => {
                self.api = None;
                self.config_status = error.to_string();
            }
        }
        let devices = self
            .api
            .as_ref()
            .map(|a| a.device_names())
            .unwrap_or_default();
        self.state.set_devices(devices);
    }

    fn pump(&mut self) {
        for _ in 0..256 {
            let Ok(message) = self.rx.try_recv() else {
                break;
            };
            match message {
                osc::Event::Traffic => self.state.osc_packets += 1,
                osc::Event::Connection(connected) => {
                    if self.state.connected && !connected {
                        self.state.reset();
                        self.state.status = "VRChat disconnected; output disarmed".into();
                    }
                    if !self.state.connected && connected {
                        self.state.status = "VRChat connected; route contacts to devices".into();
                    }
                    self.state.connected = connected;
                }
                osc::Event::Avatar => {
                    self.state.reset();
                    self.state.status = "Avatar changed; output disarmed".into();
                }
                osc::Event::Snapshot(values, started) => self.state.snapshot(values, started),
                osc::Event::Value(path, value) => {
                    if let Some(batch) = self.state.update(&path, value, true) {
                        let source = path.trim_start_matches("/avatar/parameters/").to_owned();
                        self.dispatch(&source, batch);
                    }
                }
                osc::Event::Error(error) => {
                    self.state.status = error;
                    self.state.armed = false;
                }
            }
        }
        while let Ok((device, result)) = self.done_rx.try_recv() {
            self.state.finish(&device, result);
        }
    }

    fn dispatch(&mut self, source: &str, batch: Batch) {
        let Some(api) = self.api.clone() else {
            for device in &batch.devices {
                self.state.finish(device, Ok(()));
            }
            self.state.results.clear();
            self.state.armed = false;
            return;
        };
        let pulse = batch.pulse;
        self.state.status = format!(
            "{source}: sending {} to {}{}",
            describe(pulse),
            batch.devices.join(", "),
            if self.state.random { " (random)" } else { "" }
        );
        for device in batch.devices {
            self.state
                .results
                .insert(device.clone(), format!("sending {}", describe(pulse)));
            let api = api.clone();
            let done = self.done_tx.clone();
            self.runtime.spawn(async move {
                let result = api
                    .operate(&device, pulse.op, pulse.intensity, pulse.duration)
                    .await
                    .map_err(|e| e.to_string());
                let _ = done.send((device, result)).await;
            });
        }
    }

    fn select_theme(&mut self, index: usize) {
        self.prefs.theme = index;
        self.fade.start(THEMES[index].colors, true);
        self.prefs.save();
    }

    fn view(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.panel_open = false;
            self.state.armed = false;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::T)) {
            self.panel_open = !self.panel_open;
        }
        let animating = self.fade.animating();
        if animating {
            theme::apply(ctx, &self.fade.current());
        }
        let p = self.fade.current().palette();

        let screen = ctx.content_rect();
        self.field
            .advance(screen.size(), self.prefs.speed, self.prefs.density);
        let background = ctx.layer_painter(LayerId::background());
        background.rect_filled(screen, 0, p.base);
        self.field.draw(&background, screen, &p);

        let sheet = |margin: Margin| Frame::NONE.fill(p.sheet).inner_margin(margin);

        egui::TopBottomPanel::top("header")
            .frame(sheet(Margin::symmetric(20, 0)))
            .exact_height(56.0)
            .show_separator_line(false)
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(RichText::new("pi@shock:~$ ").color(p.dim).size(13.5));
                    ui.label(RichText::new("pishock_bridge").color(p.contrast).size(13.5));
                    let (cursor, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
                    if ctx.input(|i| i.time) % 1.1 < 0.55 {
                        ui.painter().rect_filled(cursor, 0, p.accent);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let label =
                            RichText::new("[T] THEME")
                                .size(13.0)
                                .color(if self.panel_open {
                                    p.accent
                                } else {
                                    p.contrast
                                });
                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                            self.panel_open = !self.panel_open;
                        }
                        ui.add_space(12.0);
                        let (vrchat, live) = if !self.state.connected {
                            ("WAITING", false)
                        } else if self.state.osc_packets == 0 {
                            ("FOUND, NO OSC YET", false)
                        } else {
                            ("CONNECTED", true)
                        };
                        ui.label(
                            RichText::new(vrchat)
                                .color(if live { p.accent } else { p.contrast })
                                .size(13.0),
                        );
                        ui.label(RichText::new("VRCHAT").color(p.dim).size(13.0));
                    });
                });
                theme::dashed(
                    ui.painter(),
                    rect.left_bottom(),
                    rect.right_bottom(),
                    p.hair,
                );
            });

        egui::TopBottomPanel::bottom("results")
            .frame(sheet(Margin::symmetric(20, 14)))
            .show_separator_line(false)
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                theme::dashed(
                    ui.painter(),
                    rect.left_top() - vec2(0.0, 8.0),
                    rect.right_top() - vec2(0.0, 8.0),
                    p.hair,
                );
                theme::heading(ui, "PiShock result", &p);
                if self.state.results.is_empty() {
                    ui.label(RichText::new("no operation sent yet").color(p.dim));
                }
                for (device, outcome) in &self.state.results {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(device.to_uppercase()).font(theme::mono(12.0)));
                        let color = if outcome.starts_with("accepted ") {
                            p.accent
                        } else if outcome.starts_with("sending ") {
                            p.dim
                        } else {
                            p.contrast
                        };
                        ui.label(RichText::new(outcome).color(color));
                    });
                }
                ui.add_space(2.0);
                ui.label(RichText::new(&self.state.status).color(p.dim).size(13.0));
            });

        egui::SidePanel::right("output")
            .exact_width(300.0)
            .resizable(false)
            .frame(sheet(Margin {
                left: 18,
                right: 20,
                top: 16,
                bottom: 12,
            }))
            .show_separator_line(false)
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                theme::dashed(
                    ui.painter(),
                    rect.left_top() - vec2(10.0, 16.0),
                    rect.left_bottom() + vec2(-10.0, 12.0),
                    p.hair,
                );
                egui::ScrollArea::vertical().show(ui, |ui| self.output_panel(ui, &p));
            });

        egui::CentralPanel::default()
            .frame(Frame::NONE.inner_margin(Margin {
                left: 20,
                right: 12,
                top: 16,
                bottom: 12,
            }))
            .show(ctx, |ui| {
                theme::heading(ui, "Avatar contacts", &p);
                ui.label(
                    RichText::new("Route each SHK contact to the devices it should fire.")
                        .color(p.dim)
                        .size(13.5),
                );
                ui.add_space(6.0);
                egui::ScrollArea::vertical().show(ui, |ui| self.contacts_panel(ui, &p));
            });

        if self.panel_open {
            let events = theme::panel(ctx, &mut self.prefs, &p);
            if let Some(index) = events.theme {
                self.select_theme(index);
            }
            if events.prefs_changed {
                self.prefs.save();
            }
            if events.close {
                self.panel_open = false;
            }
        }

        let moving = self.prefs.speed > 0.0 && self.prefs.density > 0.0;
        ctx.request_repaint_after(Duration::from_millis(if animating || moving {
            16
        } else {
            33
        }));
    }

    fn output_panel(&mut self, ui: &mut egui::Ui, p: &Palette) {
        theme::heading(ui, "Output", p);
        theme::title(ui, "Credentials", p);
        ui.label(&self.config_status);
        if ui
            .add_enabled(self.state.idle(), egui::Button::new("RELOAD CONFIG.JSON"))
            .clicked()
        {
            self.reload_config();
        }
        theme::dashed_separator(ui, p);

        theme::title(ui, "Devices", p);
        if self.state.devices.is_empty() {
            ui.label(RichText::new("none: add pishock.devices to config.json").color(p.dim));
        }
        for device in self.state.devices.clone() {
            ui.horizontal(|ui| {
                ui.label(&device);
                let tag = if self.state.sending(&device) {
                    Some("SENDING".to_owned())
                } else {
                    self.state
                        .cooldown_left(&device)
                        .map(|left| format!("COOLDOWN {}S", left.as_secs() + 1))
                };
                if let Some(tag) = tag {
                    ui.label(RichText::new(tag).font(theme::mono(12.0)).color(p.dim));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ready = self.api.is_some() && self.state.ready(&device);
                    let test = ui
                        .add_enabled(ready, egui::Button::new("TEST"))
                        .on_hover_text("Sends the action below to this device now, without VRChat, contacts or arming.");
                    if test.clicked()
                        && let Some(batch) = self.state.test(&device)
                    {
                        self.dispatch("Test", batch);
                    }
                });
            });
        }
        theme::dashed_separator(ui, p);

        theme::title(ui, "Action", p);
        let mut changed = false;
        ui.horizontal(|ui| {
            for (op, label) in [(2, "BEEP"), (1, "VIBRATE"), (0, "SHOCK")] {
                changed |= ui.selectable_value(&mut self.state.op, op, label).changed();
            }
        });
        ui.horizontal(|ui| {
            let random = egui::Button::new("RANDOM").selected(self.state.random);
            let random = ui.add(random).on_hover_text(
                "Every activation rolls intensity and seconds between 1 and the maximums set below; each result line shows the roll.",
            );
            if random.clicked() {
                self.state.random = !self.state.random;
                changed = true;
            }
            ui.add(
                egui::Label::new(
                    RichText::new(if self.state.random {
                        "rolls 1..max each time"
                    } else {
                        "fixed values"
                    })
                    .color(p.dim)
                    .size(13.0),
                )
                .truncate(),
            );
        });
        let row = |ui: &mut egui::Ui, label: &str, hint: &str, slider: egui::Slider<'_>| -> bool {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    vec2(96.0, 20.0),
                    Layout::left_to_right(Align::Center),
                    |ui| theme::title(ui, label, p),
                );
                ui.spacing_mut().slider_width = 104.0;
                ui.add(slider).on_hover_text(hint).changed()
            })
            .inner
        };
        let max = if self.state.random { "max " } else { "" };
        ui.add_enabled_ui(self.state.op != 2, |ui| {
            changed |= row(
                ui,
                &format!("{max}intensity"),
                "Intensity 1-100 as sent; a share code's own limit rejects anything above it.",
                egui::Slider::new(&mut self.state.intensity, 1..=100),
            );
        });
        changed |= row(
            ui,
            &format!("{max}seconds"),
            "Pulse length in seconds.",
            egui::Slider::new(&mut self.state.duration, 1..=15),
        );
        changed |= row(
            ui,
            "cooldown",
            "Seconds after a pulse ends before that device accepts another activation. Other devices are unaffected.",
            egui::Slider::new(&mut self.state.cooldown, 0..=60).suffix("s"),
        );
        if changed {
            self.state.armed = false;
        }
        theme::dashed_separator(ui, p);

        theme::title(ui, "Arm", p);
        let can_arm = self.api.is_some() && self.state.connected && self.state.routed();
        let width = ui.available_width();
        let button = if self.state.armed {
            egui::Button::new(RichText::new("ARMED  -  CLICK TO DISARM").color(p.base))
                .fill(p.accent)
        } else {
            egui::Button::new(RichText::new("ARM OUTPUT").color(p.accent))
                .fill(p.base)
                .stroke(theme::stroke(p.accent))
        }
        .min_size(vec2(width, 40.0));
        if ui
            .add_enabled(self.state.armed || can_arm, button)
            .clicked()
        {
            self.state.armed = !self.state.armed;
        }
        ui.label(if self.state.armed {
            "Armed: waiting for a new activation"
        } else {
            "Disarmed"
        });
        if !can_arm {
            ui.small("Load credentials, connect VRChat and route a contact to arm.");
        }
        ui.add_space(6.0);
        ui.small("One pulse per routed, ready device when a contact crosses 0.5. Routing or setting changes, Esc, an avatar change and any failure disarm; a sent pulse runs to completion.");
        if let Some(network) = &self.network {
            theme::dashed_separator(ui, p);
            ui.label(
                RichText::new(format!("OSCQUERY :{}", network.port))
                    .color(p.dim)
                    .size(13.0),
            );
        }
    }

    fn contacts_panel(&mut self, ui: &mut egui::Ui, p: &Palette) {
        if self.state.contacts.is_empty() {
            theme::sheet(ui, p, |ui| {
                ui.label(RichText::new("No SHK parameters yet").color(p.contrast));
                ui.label(
                    RichText::new(
                        "Enable OSC in VRChat and load an avatar with SHK contact parameters.",
                    )
                    .color(p.dim),
                );
            });
            return;
        }
        let devices = self.state.devices.clone();
        let count = self.state.contacts.len();
        let mut disarm = false;
        theme::sheet(ui, p, |ui| {
            for (i, (path, contact)) in self.state.contacts.iter_mut().enumerate() {
                ui.push_id(path, |ui| {
                    let name = path.trim_start_matches("/avatar/parameters/");
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).color(p.contrast));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:.3}", contact.value))
                                    .color(p.dim)
                                    .size(13.0),
                            );
                        });
                    });
                    ui.add(
                        egui::ProgressBar::new(contact.value.clamp(0.0, 1.0) as f32)
                            .corner_radius(0)
                            .fill(p.accent)
                            .desired_height(8.0),
                    );
                    ui.horizontal_wrapped(|ui| {
                        theme::title(ui, "route", p);
                        if devices.is_empty() {
                            ui.label(RichText::new("no devices configured").color(p.dim));
                        }
                        for device in &devices {
                            let routed = contact.routes.contains(device);
                            let chip = egui::Button::new(device.to_uppercase()).selected(routed);
                            if ui.add(chip).clicked() {
                                if routed {
                                    contact.routes.remove(device);
                                } else {
                                    contact.routes.insert(device.clone());
                                }
                                disarm = true;
                            }
                        }
                    });
                    if i + 1 < count {
                        theme::dashed_separator(ui, p);
                    }
                });
            }
        });
        if disarm {
            self.state.armed = false;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
        self.view(ctx);
    }
}

fn mode(op: u8) -> &'static str {
    match op {
        0 => "SHOCK",
        1 => "VIBRATE",
        _ => "BEEP",
    }
}

fn describe(pulse: Pulse) -> String {
    if pulse.op == 2 {
        format!("{} ({}s)", mode(pulse.op), pulse.duration)
    } else {
        format!(
            "{} ({}%, {}s)",
            mode(pulse.op),
            pulse.intensity,
            pulse.duration
        )
    }
}