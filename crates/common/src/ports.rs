use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimePorts {
    pub mcp_port: u16,
    pub ui_port: u16,
    pub metrics_port: Option<u16>,
}

pub fn resolve_runtime_ports(
    cwd: &Path,
    preferred_mcp: u16,
    preferred_ui: u16,
    preferred_metrics: Option<u16>,
) -> Result<RuntimePorts> {
    let state_path = runtime_ports_path(cwd);
    if let Some(existing) = load_ports(&state_path)? {
        if ports_available(&existing) {
            return Ok(existing);
        }
    }

    let mut reserved = Vec::new();
    let mcp = find_free_port(preferred_mcp, 38080, 38180, &reserved)?;
    reserved.push(mcp);
    let ui = find_free_port(preferred_ui, 38181, 38280, &reserved)?;
    reserved.push(ui);
    let metrics = preferred_metrics
        .map(|preferred| find_free_port(preferred, 38281, 38320, &reserved))
        .transpose()?;

    let out = RuntimePorts {
        mcp_port: mcp,
        ui_port: ui,
        metrics_port: metrics,
    };
    save_ports(&state_path, &out)?;
    Ok(out)
}

fn runtime_ports_path(cwd: &Path) -> PathBuf {
    cwd.join(".codivex").join("runtime-ports.json")
}

fn ports_available(ports: &RuntimePorts) -> bool {
    port_available(ports.mcp_port)
        && port_available(ports.ui_port)
        && ports.metrics_port.is_none_or(port_available)
}

fn find_free_port(preferred: u16, start: u16, end: u16, reserved: &[u16]) -> Result<u16> {
    if !reserved.contains(&preferred) && port_available(preferred) {
        return Ok(preferred);
    }
    for port in start..=end {
        if reserved.contains(&port) {
            continue;
        }
        if port_available(port) {
            return Ok(port);
        }
    }
    anyhow::bail!("no available port in range {start}..={end}");
}

fn port_available(port: u16) -> bool {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    TcpListener::bind(addr).is_ok()
}

fn load_ports(path: &Path) -> Result<Option<RuntimePorts>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed reading runtime port file: {}", path.display()))?;
    let parsed = serde_json::from_str::<RuntimePorts>(&raw)
        .with_context(|| format!("failed parsing runtime port file: {}", path.display()))?;
    Ok(Some(parsed))
}

fn save_ports(path: &Path, ports: &RuntimePorts) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(ports)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{find_free_port, resolve_runtime_ports};

    fn temp_base() -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("codivex-port-test-{ts}"));
        fs::create_dir_all(&base).expect("mkdir");
        base
    }

    #[test]
    fn configured_busy_picks_alternate() {
        let busy = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .expect("bind ephemeral");
        let busy_port = busy.local_addr().expect("addr").port();
        let base = temp_base();
        let ports = resolve_runtime_ports(base.as_path(), busy_port, 39001, None).expect("ports");
        assert_ne!(ports.mcp_port, busy_port);
        assert_eq!(ports.ui_port, 39001);
    }

    #[test]
    fn configured_free_uses_preferred() {
        let base = temp_base();
        let ports =
            resolve_runtime_ports(base.as_path(), 39011, 39012, Some(39013)).expect("ports");
        assert_eq!(ports.mcp_port, 39011);
        assert_eq!(ports.ui_port, 39012);
        assert_eq!(ports.metrics_port, Some(39013));
    }

    #[test]
    fn all_busy_returns_error() {
        let busy = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .expect("bind ephemeral");
        let busy_port = busy.local_addr().expect("addr").port();
        let result = find_free_port(busy_port, busy_port, busy_port, &[busy_port]);
        assert!(result.is_err());
    }

    #[test]
    fn restart_stability_keeps_same_ports_when_available() {
        let base = temp_base();
        let first =
            resolve_runtime_ports(base.as_path(), 39021, 39022, Some(39023)).expect("ports");
        let second =
            resolve_runtime_ports(base.as_path(), 39030, 39031, Some(39032)).expect("ports");
        assert_eq!(first, second);
    }
}
