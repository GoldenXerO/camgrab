use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

const WS_DISCOVERY_MULTICAST_ADDR: &str = "239.255.255.250";
const WS_DISCOVERY_PORT: u16 = 3702;
const DEFAULT_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),

    #[error("XML parsing error: {0}")]
    XmlParse(String),

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Timeout waiting for responses")]
    Timeout,
}

pub type Result<T> = std::result::Result<T, DiscoveryError>;

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub timeout: Duration,
    pub interface: Option<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            interface: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiscoveredDevice {
    pub address: String,
    pub scopes: Vec<String>,
    pub types: Vec<String>,
    pub xaddrs: Vec<String>,
    pub name: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
}

impl DiscoveredDevice {
    fn new(address: String) -> Self {
        Self {
            address,
            scopes: Vec::new(),
            types: Vec::new(),
            xaddrs: Vec::new(),
            name: None,
            manufacturer: None,
            model: None,
        }
    }

    fn parse_scopes(&mut self) {
        for scope in &self.scopes {
            if let Some(name) = scope.strip_prefix("onvif://www.onvif.org/name/") {
                self.name = Some(name.to_string());
            } else if let Some(hardware) = scope.strip_prefix("onvif://www.onvif.org/hardware/") {
                self.manufacturer = Some(hardware.to_string());
            } else if let Some(profile) = scope.strip_prefix("onvif://www.onvif.org/Profile/") {
                if self.model.is_none() {
                    self.model = Some(profile.to_string());
                }
            }
        }
    }
}

pub fn discover(config: &DiscoveryConfig) -> Result<Vec<DiscoveredDevice>> {
    let probe_msg = build_probe_message()?;

    // Create UDP socket
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(config.timeout))?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?;

    // Send multicast probe
    let multicast_addr = SocketAddr::new(
        IpAddr::V4(WS_DISCOVERY_MULTICAST_ADDR.parse().unwrap()),
        WS_DISCOVERY_PORT,
    );

    socket.send_to(probe_msg.as_bytes(), multicast_addr)?;

    // Collect responses with deduplication
    let mut devices = HashSet::new();
    let mut buf = [0u8; 65535];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((size, addr)) => {
                if let Ok(device) = parse_probe_match(&buf[..size], addr.ip().to_string()) {
                    devices.insert(device);
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(e) => return Err(DiscoveryError::Network(e)),
        }
    }

    let mut devices: Vec<_> = devices.into_iter().collect();
    for device in &mut devices {
        device.parse_scopes();
    }

    Ok(devices)
}

fn build_probe_message() -> Result<String> {
    let message_id = Uuid::new_v4();

    let mut writer = Writer::new(Vec::new());

    // Write XML declaration
    writer
        .write_event(Event::Decl(quick_xml::events::BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            None,
        )))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // Envelope
    let mut envelope = BytesStart::new("s:Envelope");
    envelope.push_attribute(("xmlns:s", "http://www.w3.org/2003/05/soap-envelope"));
    envelope.push_attribute((
        "xmlns:a",
        "http://schemas.xmlsoap.org/ws/2004/08/addressing",
    ));
    envelope.push_attribute(("xmlns:d", "http://schemas.xmlsoap.org/ws/2005/04/discovery"));
    envelope.push_attribute(("xmlns:dn", "http://www.onvif.org/ver10/network/wsdl"));
    writer
        .write_event(Event::Start(envelope))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // Header
    writer
        .write_event(Event::Start(BytesStart::new("s:Header")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // Action
    writer
        .write_event(Event::Start(BytesStart::new("a:Action")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(
            "http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe",
        )))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("a:Action").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // MessageID
    writer
        .write_event(Event::Start(BytesStart::new("a:MessageID")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(&format!(
            "uuid:{message_id}"
        ))))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("a:MessageID").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // To
    writer
        .write_event(Event::Start(BytesStart::new("a:To")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(
            "urn:schemas-xmlsoap-org:ws:2005:04:discovery",
        )))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("a:To").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // ReplyTo
    writer
        .write_event(Event::Start(BytesStart::new("a:ReplyTo")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("a:Address")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(
            "http://schemas.xmlsoap.org/ws/2004/08/addressing/role/anonymous",
        )))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("a:Address").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("a:ReplyTo").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    writer
        .write_event(Event::End(BytesStart::new("s:Header").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // Body
    writer
        .write_event(Event::Start(BytesStart::new("s:Body")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // Probe
    writer
        .write_event(Event::Start(BytesStart::new("d:Probe")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    // Types
    writer
        .write_event(Event::Start(BytesStart::new("d:Types")))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(
            "dn:NetworkVideoTransmitter",
        )))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("d:Types").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    writer
        .write_event(Event::End(BytesStart::new("d:Probe").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    writer
        .write_event(Event::End(BytesStart::new("s:Body").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    writer
        .write_event(Event::End(BytesStart::new("s:Envelope").to_end()))
        .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?;

    String::from_utf8(writer.into_inner()).map_err(|e| DiscoveryError::XmlParse(e.to_string()))
}

fn parse_probe_match(data: &[u8], source_addr: String) -> Result<DiscoveredDevice> {
    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(true);

    let mut device = DiscoveredDevice::new(source_addr);
    let mut buf = Vec::new();
    let mut current_element = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = name.split(':').last().unwrap_or(&name).to_string();
            }
            Ok(Event::Text(e)) => {
                let text = e
                    .unescape()
                    .map_err(|e| DiscoveryError::XmlParse(e.to_string()))?
                    .to_string();

                match current_element.as_str() {
                    "XAddrs" => {
                        device.xaddrs = text.split_whitespace().map(String::from).collect();
                    }
                    "Types" => {
                        device.types = text.split_whitespace().map(String::from).collect();
                    }
                    "Scopes" => {
                        device.scopes = text.split_whitespace().map(String::from).collect();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DiscoveryError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(device)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_config_default() {
        let config = DiscoveryConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECS));
        assert!(config.interface.is_none());
    }

    #[test]
    fn test_build_probe_message() {
        let result = build_probe_message();
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert!(msg.contains("http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe"));
        assert!(msg.contains("dn:NetworkVideoTransmitter"));
        assert!(msg.contains("uuid:"));
    }

    #[test]
    fn test_parse_probe_match() {
        let response = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope"
                   xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing"
                   xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery">
    <SOAP-ENV:Header>
        <wsa:MessageID>uuid:12345678-1234-1234-1234-123456789012</wsa:MessageID>
    </SOAP-ENV:Header>
    <SOAP-ENV:Body>
        <d:ProbeMatches>
            <d:ProbeMatch>
                <wsa:EndpointReference>
                    <wsa:Address>uuid:abcd-efgh-ijkl-mnop</wsa:Address>
                </wsa:EndpointReference>
                <d:Types>dn:NetworkVideoTransmitter</d:Types>
                <d:Scopes>onvif://www.onvif.org/name/TestCamera onvif://www.onvif.org/hardware/TestManufacturer</d:Scopes>
                <d:XAddrs>http://192.168.1.100/onvif/device_service</d:XAddrs>
            </d:ProbeMatch>
        </d:ProbeMatches>
    </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;

        let result = parse_probe_match(response.as_bytes(), "192.168.1.100".to_string());
        assert!(result.is_ok());

        let mut device = result.unwrap();
        assert_eq!(device.address, "192.168.1.100");
        assert_eq!(device.xaddrs.len(), 1);
        assert!(device.xaddrs[0].contains("192.168.1.100"));
        assert!(!device.scopes.is_empty());

        device.parse_scopes();
        assert_eq!(device.name, Some("TestCamera".to_string()));
        assert_eq!(device.manufacturer, Some("TestManufacturer".to_string()));
    }

    #[test]
    fn test_discovered_device_parse_scopes() {
        let mut device = DiscoveredDevice::new("192.168.1.100".to_string());
        device.scopes = vec![
            "onvif://www.onvif.org/name/MyCamera".to_string(),
            "onvif://www.onvif.org/hardware/Acme".to_string(),
            "onvif://www.onvif.org/Profile/Streaming".to_string(),
        ];

        device.parse_scopes();

        assert_eq!(device.name, Some("MyCamera".to_string()));
        assert_eq!(device.manufacturer, Some("Acme".to_string()));
        assert_eq!(device.model, Some("Streaming".to_string()));
    }

    #[test]
    fn test_discovered_device_equality() {
        let device1 = DiscoveredDevice::new("192.168.1.100".to_string());
        let device2 = DiscoveredDevice::new("192.168.1.100".to_string());
        let device3 = DiscoveredDevice::new("192.168.1.101".to_string());

        assert_eq!(device1, device2);
        assert_ne!(device1, device3);
    }
}
