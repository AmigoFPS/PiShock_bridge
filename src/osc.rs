use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use rosc::{OscPacket, OscType};
use serde_json::{Value, json};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
    sync::{RwLock, mpsc},
};

const SERVICE: &str = "_oscjson._tcp.local";
const MDNS_GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 251), 5353);
const MDNS_TTL: u32 = 120;
const ADVERTISE_EVERY: Duration = Duration::from_secs(5);

pub enum Event {
    Traffic,
    Snapshot(Vec<(String, f64)>, Instant),
    Value(String, f64),
    Avatar,
    Connection(bool),
    Error(String),
}

pub struct Network {
    daemon: ServiceDaemon,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    pub port: u16,
}
impl Drop for Network {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        let _ = self.daemon.shutdown();
    }
}

pub async fn start(tx: mpsc::Sender<Event>) -> Result<Network> {
    let mut pair = None;
    for port in 33776..34288 {
        if let Ok(tcp) = TcpListener::bind(("0.0.0.0", port)).await
            && let Ok(udp) = UdpSocket::bind(("0.0.0.0", port)).await
        {
            pair = Some((port, tcp, udp));
            break;
        }
    }
    let (port, tcp, udp) = pair.ok_or_else(|| anyhow::anyhow!("No free OSCQuery port"))?;
    let tree = Arc::new(RwLock::new(root(&[])));
    let daemon = ServiceDaemon::new()?;
    let browse = daemon.browse(&format!("{SERVICE}."))?;
    let packet = advertisement(
        &format!("Pishock-bridge-{port}"),
        &format!("pishock-bridge-{port}.local"),
        port,
    );
    let advertise_tx = tx.clone();
    let advertise = tokio::spawn(async move {
        let mut reported = false;
        loop {
            if let Err(error) = broadcast(&packet)
                && !reported
            {
                reported = true;
                let _ = advertise_tx
                    .send(Event::Error(format!("mDNS advertisement failed: {error}")))
                    .await;
            }
            tokio::time::sleep(ADVERTISE_EVERY).await;
        }
    });
    let server_tree = tree.clone();
    let server = tokio::spawn(async move {
        while let Ok((mut stream, _)) = tcp.accept().await {
            let tree = server_tree.clone();
            let _ = tokio::time::timeout(Duration::from_secs(1), async {
                let mut data = Vec::new();
                let mut buf = [0; 1024];
                while data.len() < 8192 {
                    let n = stream.read(&mut buf).await?;
                    if n == 0 { break; }
                    data.extend_from_slice(&buf[..n]);
                    if data.windows(4).any(|w| w == b"\r\n\r\n") { break; }
                }
                let request = String::from_utf8_lossy(&data);
                let target = request.split_whitespace().nth(1).unwrap_or("/");
                let body = if target == "/?HOST_INFO" {
                    json!({"NAME":"Pishock_bridge", "OSC_IP":"127.0.0.1", "OSC_PORT":port,
                        "OSC_TRANSPORT":"UDP", "EXTENSIONS":{"ACCESS":true,"VALUE":true}})
                } else { tree.read().await.clone() }.to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await
            }).await;
        }
    });
    let udp_tx = tx.clone();
    let local_ips: Vec<_> = if_addrs::get_if_addrs()?
        .into_iter()
        .map(|i| i.ip())
        .collect();
    let receiver = tokio::spawn(async move {
        let mut buf = vec![0; 65535];
        while let Ok((n, peer)) = udp.recv_from(&mut buf).await {
            if !peer.ip().is_loopback() && !local_ips.contains(&peer.ip()) {
                continue;
            }
            if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..n]) {
                if udp_tx.send(Event::Traffic).await.is_err() {
                    return;
                }
                for event in decode(packet) {
                    if udp_tx.send(event).await.is_err() {
                        return;
                    }
                }
            }
        }
    });
    let discovery = tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
        {
            Ok(c) => c,
            Err(_) => {
                let _ = tx.send(Event::Error("HTTP client failed".into())).await;
                return;
            }
        };
        let mut services = BTreeMap::new();
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;
            while let Ok(event) = browse.try_recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        services.insert(info.get_fullname().to_owned(), info.get_port());
                    }
                    ServiceEvent::ServiceRemoved(_, name) => {
                        services.remove(&name);
                    }
                    _ => {}
                }
            }
            let mut found = false;
            for &candidate in services.values() {
                if candidate == port {
                    continue;
                }
                let base = format!("http://127.0.0.1:{candidate}");
                let started = Instant::now();
                let result = async {
                    let host: Value = client
                        .get(format!("{base}/?HOST_INFO"))
                        .send()
                        .await?
                        .error_for_status()?
                        .json()
                        .await?;
                    if !host["NAME"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("VRChat-Client-"))
                    {
                        anyhow::bail!("Not VRChat");
                    }
                    let node: Value = client
                        .get(format!("{base}/"))
                        .send()
                        .await?
                        .error_for_status()?
                        .json()
                        .await?;
                    Ok::<_, anyhow::Error>(collect(&node))
                }
                .await;
                if let Ok(values) = result {
                    *tree.write().await = root(&values);
                    let _ = tx.send(Event::Connection(true)).await;
                    let _ = tx.send(Event::Snapshot(values, started)).await;
                    found = true;
                    break;
                }
            }
            if !found {
                let _ = tx.send(Event::Connection(false)).await;
            }
        }
    });
    Ok(Network {
        daemon,
        tasks: vec![server, receiver, discovery, advertise],
        port,
    })
}

fn advertisement(name: &str, host: &str, port: u16) -> Vec<u8> {
    fn dns_name(out: &mut Vec<u8>, name: &str) {
        for label in name.split('.').filter(|l| !l.is_empty()) {
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0);
    }
    fn record(out: &mut Vec<u8>, name: &str, rr_type: u16, class: u16, rdata: &[u8]) {
        dns_name(out, name);
        out.extend_from_slice(&rr_type.to_be_bytes());
        out.extend_from_slice(&class.to_be_bytes());
        out.extend_from_slice(&MDNS_TTL.to_be_bytes());
        out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        out.extend_from_slice(rdata);
    }
    const IN: u16 = 1;
    const IN_FLUSH: u16 = 0x8001;
    let instance = format!("{name}.{SERVICE}");
    let mut packet = Vec::with_capacity(256);
    for word in [0u16, 0x8400, 0, 1, 0, 3] {
        packet.extend_from_slice(&word.to_be_bytes());
    }
    let mut ptr = Vec::new();
    dns_name(&mut ptr, &instance);
    record(&mut packet, SERVICE, 12, IN, &ptr);
    let mut srv = vec![0, 0, 0, 0];
    srv.extend_from_slice(&port.to_be_bytes());
    dns_name(&mut srv, host);
    record(&mut packet, &instance, 33, IN_FLUSH, &srv);
    record(&mut packet, &instance, 16, IN_FLUSH, b"\x09txtvers=1");
    record(
        &mut packet,
        host,
        1,
        IN_FLUSH,
        &Ipv4Addr::LOCALHOST.octets(),
    );
    packet
}

fn broadcast(packet: &[u8]) -> Result<()> {
    let mut sent = 0;
    let mut failure = None;
    for interface in if_addrs::get_if_addrs()? {
        let IpAddr::V4(addr) = interface.ip() else {
            continue;
        };
        if addr.is_loopback() {
            continue;
        }
        let result = (|| {
            let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            socket.set_reuse_address(true)?;
            if socket
                .bind(&SocketAddr::new(addr.into(), 5353).into())
                .is_err()
            {
                socket.bind(&SocketAddr::new(addr.into(), 0).into())?;
            }
            socket.set_multicast_ttl_v4(255)?;
            socket.set_multicast_loop_v4(true)?;
            socket.set_multicast_if_v4(&addr)?;
            socket.send_to(packet, &SocketAddr::from(MDNS_GROUP).into())?;
            std::io::Result::Ok(())
        })();
        match result {
            Ok(()) => sent += 1,
            Err(error) => failure = Some(error),
        }
    }
    match (sent, failure) {
        (0, Some(error)) => Err(error.into()),
        (0, None) => anyhow::bail!("no IPv4 network adapter"),
        _ => Ok(()),
    }
}

fn root(values: &[(String, f64)]) -> Value {
    let mut parameters = serde_json::Map::new();
    for (path, value) in values {
        parameters.insert(
            path.trim_start_matches("/avatar/parameters/").into(),
            json!({"FULL_PATH":path,"ACCESS":2,"TYPE":"f","VALUE":[value]}),
        );
    }
    json!({"FULL_PATH":"/", "ACCESS":0, "CONTENTS":{"avatar":{
    "FULL_PATH":"/avatar", "ACCESS":0,"CONTENTS":{
        "change":{"FULL_PATH":"/avatar/change","ACCESS":2,"TYPE":"s"},
        "parameters":{"FULL_PATH":"/avatar/parameters","ACCESS":2,"CONTENTS":parameters}
    }}}})
}

pub fn collect(node: &Value) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    if let Some(path) = node["FULL_PATH"].as_str()
        && path.starts_with("/avatar/parameters/SHK")
    {
        let v = &node["VALUE"][0];
        let value = v
            .as_f64()
            .or_else(|| v.as_bool().map(|b| if b { 1.0 } else { 0.0 }))
            .unwrap_or(0.0);
        out.push((path.to_owned(), value));
    }
    if let Some(children) = node["CONTENTS"].as_object() {
        for child in children.values() {
            out.extend(collect(child));
        }
    }
    out
}

fn decode(packet: OscPacket) -> Vec<Event> {
    match packet {
        OscPacket::Bundle(b) => b.content.into_iter().flat_map(decode).collect(),
        OscPacket::Message(m) if m.addr == "/avatar/change" => vec![Event::Avatar],
        OscPacket::Message(m) => {
            let value = match m.args.first() {
                Some(OscType::Float(v)) => *v as f64,
                Some(OscType::Double(v)) => *v,
                Some(OscType::Int(v)) => *v as f64,
                Some(OscType::Bool(v)) => {
                    if *v {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => return vec![],
            };
            if m.addr.starts_with("/avatar/parameters/SHK") {
                vec![Event::Value(m.addr, value)]
            } else {
                vec![]
            }
        }
    }
}