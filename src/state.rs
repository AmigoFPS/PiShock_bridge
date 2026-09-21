use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Contact {
    pub value: f64,
    pub routes: BTreeSet<String>,
    last_live: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pulse {
    pub op: u8,
    pub intensity: u8,
    pub duration: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Batch {
    pub devices: Vec<String>,
    pub pulse: Pulse,
}

pub struct State {
    pub contacts: BTreeMap<String, Contact>,
    pub devices: Vec<String>,
    pub armed: bool,
    pub connected: bool,
    busy: BTreeSet<String>,
    cooling: BTreeMap<String, Instant>,
    pub op: u8,
    pub intensity: u8,
    pub duration: u8,
    pub cooldown: u8,
    pub random: bool,
    pub status: String,
    pub osc_packets: u64,
    pub results: BTreeMap<String, String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            contacts: BTreeMap::new(),
            devices: Vec::new(),
            armed: false,
            connected: false,
            busy: BTreeSet::new(),
            cooling: BTreeMap::new(),
            op: 2,
            intensity: 5,
            duration: 1,
            cooldown: 1,
            random: false,
            status: "Waiting for VRChat".into(),
            osc_packets: 0,
            results: BTreeMap::new(),
        }
    }
}
impl State {
    pub fn snapshot(&mut self, values: Vec<(String, f64)>, started: Instant) {
        for (path, value) in values {
            if self
                .contacts
                .get(&path)
                .and_then(|c| c.last_live)
                .is_some_and(|t| t > started)
            {
                continue;
            }
            self.update(&path, value, false);
        }
    }
    pub fn reset(&mut self) {
        self.armed = false;
        self.contacts.clear();
    }
    pub fn set_devices(&mut self, devices: Vec<String>) {
        self.armed = false;
        for contact in self.contacts.values_mut() {
            contact.routes.retain(|name| devices.contains(name));
        }
        self.results.retain(|name, _| devices.contains(name));
        self.cooling.retain(|name, _| devices.contains(name));
        self.devices = devices;
    }
    pub fn targets(&self, path: &str) -> Vec<String> {
        let Some(contact) = self.contacts.get(path) else {
            return Vec::new();
        };
        self.devices
            .iter()
            .filter(|name| contact.routes.contains(*name))
            .cloned()
            .collect()
    }
    pub fn routed(&self) -> bool {
        self.contacts
            .keys()
            .any(|path| !self.targets(path).is_empty())
    }
    pub fn idle(&self) -> bool {
        self.busy.is_empty()
    }
    pub fn sending(&self, device: &str) -> bool {
        self.busy.contains(device)
    }
    pub fn cooldown_left(&self, device: &str) -> Option<Duration> {
        self.cooling
            .get(device)
            .and_then(|until| until.checked_duration_since(Instant::now()))
            .filter(|left| !left.is_zero())
    }
    pub fn ready(&self, device: &str) -> bool {
        !self.sending(device) && self.cooldown_left(device).is_none()
    }
    pub fn pulse(&self) -> Pulse {
        Pulse {
            op: self.op,
            intensity: if self.random {
                fastrand::u8(1..=self.intensity.max(1))
            } else {
                self.intensity
            },
            duration: if self.random {
                fastrand::u8(1..=self.duration.max(1))
            } else {
                self.duration
            },
        }
    }
    fn reserve(&mut self, candidates: &[String]) -> Option<Batch> {
        let devices: Vec<String> = candidates
            .iter()
            .filter(|d| self.ready(d))
            .cloned()
            .collect();
        if devices.is_empty() {
            return None;
        }
        let pulse = self.pulse();
        let until = Instant::now()
            + Duration::from_secs(u64::from(pulse.duration) + u64::from(self.cooldown));
        for device in &devices {
            self.busy.insert(device.clone());
            self.cooling.insert(device.clone(), until);
        }
        Some(Batch { devices, pulse })
    }
    pub fn test(&mut self, device: &str) -> Option<Batch> {
        self.reserve(&[device.to_owned()])
    }
    pub fn finish(&mut self, device: &str, result: Result<(), String>) {
        self.busy.remove(device);
        match result {
            Ok(()) => {
                let sent = self
                    .results
                    .get(device)
                    .and_then(|s| s.strip_prefix("sending "))
                    .unwrap_or("")
                    .to_owned();
                self.results
                    .insert(device.to_owned(), format!("accepted {sent}"));
            }
            Err(error) => {
                self.armed = false;
                self.results.insert(device.to_owned(), error);
                self.status = format!("{device} failed; output disarmed");
            }
        }
    }
    pub fn update(&mut self, path: &str, value: f64, live: bool) -> Option<Batch> {
        if !path.starts_with("/avatar/parameters/SHK") || !value.is_finite() {
            return None;
        }
        let contact = self.contacts.entry(path.to_owned()).or_default();
        let rising = contact.value <= 0.5 && value > 0.5;
        contact.value = value;
        if live {
            contact.last_live = Some(Instant::now());
        }
        if !(live && rising) {
            return None;
        }
        let name = path.trim_start_matches("/avatar/parameters/");
        let targets = self.targets(path);
        let reason = if targets.is_empty() {
            Some("contact not routed")
        } else if !self.armed {
            Some("output disarmed")
        } else if !self.connected {
            Some("VRChat not connected")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.status = format!("{name}: {reason}");
            return None;
        }
        let batch = self.reserve(&targets);
        if batch.is_none() {
            let waiting: Vec<String> = targets
                .iter()
                .map(|d| {
                    if self.sending(d) {
                        format!("{d} sending")
                    } else {
                        format!(
                            "{d} cooling down {}s",
                            self.cooldown_left(d).map_or(0, |t| t.as_secs() + 1)
                        )
                    }
                })
                .collect();
            self.status = format!("{name}: {}", waiting.join(", "));
        }
        batch
    }
}
