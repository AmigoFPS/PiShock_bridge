use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::time::Duration;

pub struct Device {
    pub name: String,
    code: String,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Device({})", self.name)
    }
}

pub struct PiShock {
    auth: Value,
    devices: Vec<Device>,
}

impl PiShock {
    pub fn load() -> Result<Option<Self>> {
        let config: Value = match std::fs::read_to_string("config.json") {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|_| anyhow::anyhow!("config.json contains invalid JSON"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
            Err(_) => bail!("Cannot read config.json"),
        };
        let Some((auth, devices)) = credentials(&config, |key| std::env::var(key).ok())? else {
            return Ok(None);
        };
        Ok(Some(Self { auth, devices }))
    }

    pub fn device_names(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.name.clone()).collect()
    }

    pub async fn operate(&self, device: &str, op: u8, intensity: u8, duration: u8) -> Result<()> {
        let Some(device) = self.devices.iter().find(|d| d.name == device) else {
            bail!("Unknown device {device}");
        };
        let client = reqwest::Client::builder()
            .http1_only()
            .timeout(Duration::from_secs(5))
            .build()?;
        self.send(
            &client,
            "https://ps.pishock.com/PiShock/Operate",
            &device.code,
            op,
            intensity,
            duration,
        )
        .await
    }

    async fn send(
        &self,
        client: &reqwest::Client,
        endpoint: &str,
        code: &str,
        op: u8,
        intensity: u8,
        duration: u8,
    ) -> Result<()> {
        let body = payload(&self.auth, code, op, intensity, duration)?;
        let response = client
            .post(endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    anyhow::anyhow!("PiShock timed out; delivery unknown, not retried")
                } else if e.is_connect() {
                    anyhow::anyhow!("PiShock connection failed: {}", cause(&e))
                } else {
                    anyhow::anyhow!("PiShock request failed: {}", cause(&e))
                }
            })?;
        let status = response.status();
        let response = response
            .text()
            .await
            .map_err(|_| anyhow::anyhow!("Cannot read PiShock response; delivery unknown"))?;
        check_response(status.as_u16(), &response, &self.secrets())
    }

    fn secrets(&self) -> Vec<String> {
        ["apikey", "username"]
            .iter()
            .filter_map(|f| self.auth[f].as_str().map(str::to_owned))
            .chain(self.devices.iter().map(|d| d.code.clone()))
            .collect()
    }
}

fn cause(error: &reqwest::Error) -> String {
    let chain: Vec<String> =
        std::iter::successors(std::error::Error::source(error), |e| e.source())
            .map(ToString::to_string)
            .collect();
    if chain.is_empty() {
        error.to_string()
    } else {
        chain.join(": ")
    }
}

fn check_response(status: u16, response: &str, secrets: &[String]) -> Result<()> {
    let decoded = serde_json::from_str::<String>(response).unwrap_or_else(|_| response.to_owned());
    // "Attempted" is the current wording, "Succeeded" the documented one.
    let accepted = ["Operation Attempted.", "Operation Succeeded."];
    if !(200..300).contains(&status) || !accepted.contains(&decoded.trim()) {
        let mut safe = decoded;
        for secret in secrets.iter().filter(|s| !s.is_empty()) {
            safe = safe.replace(secret, "[redacted]");
        }
        let safe: String = safe
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(300)
            .collect();
        bail!(
            "PiShock HTTP {status}: {}",
            if safe.trim().is_empty() {
                "empty response"
            } else {
                safe.trim()
            }
        );
    }
    Ok(())
}

pub fn share_codes(value: &str) -> Vec<String> {
    let value = value.trim();
    let value = match value.find("sharecode=") {
        Some(at) => value[at + "sharecode=".len()..]
            .split(['&', '#', '/', '?'])
            .next()
            .unwrap_or(""),
        None => value,
    };
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !(s.starts_with('<') && s.ends_with('>')))
        .map(str::to_owned)
        .collect()
}

type Credentials = Option<(Value, Vec<Device>)>;

fn credentials(config: &Value, env: impl Fn(&str) -> Option<String>) -> Result<Credentials> {
    if !config.is_object() || config.get("pishock").is_some_and(|v| !v.is_object()) {
        bail!("config.json must contain a pishock object");
    }
    let section = config.get("pishock");
    let text = |value: Option<&Value>, what: &str| -> Result<Option<String>> {
        if value.is_some_and(|v| !v.is_string()) {
            bail!("config.json: {what} must be a string");
        }
        Ok(value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && !(s.starts_with('<') && s.ends_with('>')))
            .map(str::to_owned))
    };
    let read = |field: &str, key: &str| -> Result<Option<String>> {
        let file = text(
            section.and_then(|p| p.get(field)),
            &format!("pishock.{field}"),
        )?;
        Ok(env(key)
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_owned())
            .or(file))
    };
    let username = read("username", "PISHOCK_USERNAME")?;
    let api_key = read("api_key", "PISHOCK_API_KEY")?;
    let name = read("name", "PISHOCK_NAME")?.unwrap_or_else(|| "Pishock_bridge".into());

    let mut devices = Vec::new();
    let mut add = |name: String, codes: Vec<String>| {
        let many = codes.len() > 1;
        for (i, code) in codes.into_iter().enumerate() {
            let name = if many {
                format!("{name} {}", i + 1)
            } else {
                name.clone()
            };
            devices.push(Device { name, code });
        }
    };
    if let Some(code) = read("share_code", "PISHOCK_SHARE_CODE")? {
        add("PiShock".into(), share_codes(&code));
    }
    match section.and_then(|p| p.get("devices")) {
        None | Some(Value::Null) => {}
        Some(Value::Array(entries)) => {
            for (i, entry) in entries.iter().enumerate() {
                if !entry.is_object() {
                    bail!("config.json: pishock.devices[{i}] must be an object");
                }
                let code = text(
                    entry.get("share_code"),
                    &format!("pishock.devices[{i}].share_code"),
                )?;
                let name = text(entry.get("name"), &format!("pishock.devices[{i}].name"))?
                    .unwrap_or_else(|| format!("Device {}", i + 1));
                if let Some(code) = code {
                    add(name, share_codes(&code));
                }
            }
        }
        Some(_) => bail!("config.json: pishock.devices must be an array"),
    }
    let mut seen = std::collections::BTreeSet::new();
    for device in &devices {
        if !seen.insert(&device.name) {
            bail!("config.json: duplicate device name {}", device.name);
        }
    }
    match (username, api_key) {
        (Some(username), Some(api_key)) if !devices.is_empty() => Ok(Some((
            json!({ "username": username, "apikey": api_key, "name": name }),
            devices,
        ))),
        _ => Ok(None),
    }
}

fn payload(auth: &Value, code: &str, op: u8, intensity: u8, duration: u8) -> Result<Value> {
    if op > 2 || !(1..=100).contains(&intensity) || !(1..=15).contains(&duration) {
        bail!("Invalid operation, intensity or duration");
    }
    let mut body = auth.clone();
    body["code"] = json!(code);
    body["op"] = json!(op);
    body["duration"] = json!(duration);
    body["intensity"] = json!(if op == 2 { 0 } else { intensity });
    body["random"] = json!(false);
    body["scale"] = json!(false);
    Ok(body)
}