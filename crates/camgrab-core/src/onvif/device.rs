use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};
use reqwest::Client;
use sha1::{Digest, Sha1};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("XML parsing error: {0}")]
    XmlParse(String),

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Endpoint not found")]
    EndpointNotFound,

    #[error("Profile not found")]
    ProfileNotFound,
}

pub type Result<T> = std::result::Result<T, DeviceError>;

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub manufacturer: String,
    pub model: String,
    pub firmware_version: String,
    pub serial_number: String,
    pub hardware_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaProfile {
    pub token: String,
    pub name: String,
    pub video_source: Option<String>,
    pub video_encoder: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeviceService {
    client: Client,
}

impl DeviceService {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn get_device_information(
        &self,
        endpoint: &str,
        auth: Option<(&str, &str)>,
    ) -> Result<DeviceInfo> {
        let soap_body =
            r#"<tds:GetDeviceInformation xmlns:tds="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let envelope = build_soap_envelope(soap_body, auth)?;

        let response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(DeviceError::InvalidResponse(format!(
                "HTTP status: {}",
                response.status()
            )));
        }

        let body = response.text().await?;
        parse_device_information(&body)
    }

    pub async fn get_profiles(
        &self,
        endpoint: &str,
        auth: Option<(&str, &str)>,
    ) -> Result<Vec<MediaProfile>> {
        let soap_body = r#"<trt:GetProfiles xmlns:trt="http://www.onvif.org/ver10/media/wsdl"/>"#;
        let envelope = build_soap_envelope(soap_body, auth)?;

        let response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(DeviceError::InvalidResponse(format!(
                "HTTP status: {}",
                response.status()
            )));
        }

        let body = response.text().await?;
        parse_profiles(&body)
    }

    pub async fn get_stream_uri(
        &self,
        endpoint: &str,
        profile_token: &str,
        auth: Option<(&str, &str)>,
    ) -> Result<String> {
        let soap_body = format!(
            r#"<trt:GetStreamUri xmlns:trt="http://www.onvif.org/ver10/media/wsdl">
                <trt:StreamSetup>
                    <tt:Stream xmlns:tt="http://www.onvif.org/ver10/schema">RTP-Unicast</tt:Stream>
                    <tt:Transport xmlns:tt="http://www.onvif.org/ver10/schema">
                        <tt:Protocol>RTSP</tt:Protocol>
                    </tt:Transport>
                </trt:StreamSetup>
                <trt:ProfileToken>{profile_token}</trt:ProfileToken>
            </trt:GetStreamUri>"#
        );

        let envelope = build_soap_envelope(&soap_body, auth)?;

        let response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(DeviceError::InvalidResponse(format!(
                "HTTP status: {}",
                response.status()
            )));
        }

        let body = response.text().await?;
        parse_stream_uri(&body)
    }
}

impl Default for DeviceService {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn build_soap_envelope(body: &str, auth: Option<(&str, &str)>) -> Result<String> {
    let mut writer = Writer::new(Vec::new());

    // XML declaration
    writer
        .write_event(Event::Decl(quick_xml::events::BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            None,
        )))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Envelope
    let mut envelope = BytesStart::new("s:Envelope");
    envelope.push_attribute(("xmlns:s", "http://www.w3.org/2003/05/soap-envelope"));
    envelope.push_attribute(("xmlns:tds", "http://www.onvif.org/ver10/device/wsdl"));
    envelope.push_attribute(("xmlns:trt", "http://www.onvif.org/ver10/media/wsdl"));
    envelope.push_attribute(("xmlns:tt", "http://www.onvif.org/ver10/schema"));
    writer
        .write_event(Event::Start(envelope))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Header
    writer
        .write_event(Event::Start(BytesStart::new("s:Header")))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Add WS-Security if authentication is provided
    if let Some((username, password)) = auth {
        write_security_header(&mut writer, username, password)?;
    }

    writer
        .write_event(Event::End(BytesStart::new("s:Header").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Body
    writer
        .write_event(Event::Start(BytesStart::new("s:Body")))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Write the actual SOAP body content
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(body)))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    writer
        .write_event(Event::End(BytesStart::new("s:Body").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    writer
        .write_event(Event::End(BytesStart::new("s:Envelope").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    String::from_utf8(writer.into_inner()).map_err(|e| DeviceError::XmlParse(e.to_string()))
}

fn write_security_header(
    writer: &mut Writer<Vec<u8>>,
    username: &str,
    password: &str,
) -> Result<()> {
    // Generate nonce and timestamp
    let nonce_bytes = Uuid::new_v4().as_bytes().to_vec();
    let nonce_b64 = BASE64.encode(&nonce_bytes);
    let created = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

    // Calculate password digest: Base64(SHA1(nonce + created + password))
    let mut hasher = Sha1::new();
    hasher.update(&nonce_bytes);
    hasher.update(created.as_bytes());
    hasher.update(password.as_bytes());
    let digest = hasher.finalize();
    let digest_b64 = BASE64.encode(digest);

    // Security header
    let mut security = BytesStart::new("wsse:Security");
    security.push_attribute((
        "xmlns:wsse",
        "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd",
    ));
    security.push_attribute((
        "xmlns:wsu",
        "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd",
    ));
    writer
        .write_event(Event::Start(security))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // UsernameToken
    writer
        .write_event(Event::Start(BytesStart::new("wsse:UsernameToken")))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Username
    writer
        .write_event(Event::Start(BytesStart::new("wsse:Username")))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(username)))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("wsse:Username").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Password
    let mut password_elem = BytesStart::new("wsse:Password");
    password_elem.push_attribute(("Type", "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest"));
    writer
        .write_event(Event::Start(password_elem))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(&digest_b64)))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("wsse:Password").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Nonce
    let mut nonce_elem = BytesStart::new("wsse:Nonce");
    nonce_elem.push_attribute(("EncodingType", "http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary"));
    writer
        .write_event(Event::Start(nonce_elem))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(&nonce_b64)))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("wsse:Nonce").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    // Created
    writer
        .write_event(Event::Start(BytesStart::new("wsu:Created")))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(&created)))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;
    writer
        .write_event(Event::End(BytesStart::new("wsu:Created").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    writer
        .write_event(Event::End(BytesStart::new("wsse:UsernameToken").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    writer
        .write_event(Event::End(BytesStart::new("wsse:Security").to_end()))
        .map_err(|e| DeviceError::XmlParse(e.to_string()))?;

    Ok(())
}

fn parse_device_information(xml: &str) -> Result<DeviceInfo> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut manufacturer = String::new();
    let mut model = String::new();
    let mut firmware_version = String::new();
    let mut serial_number = String::new();
    let mut hardware_id = String::new();

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
                    .map_err(|e| DeviceError::XmlParse(e.to_string()))?
                    .to_string();

                match current_element.as_str() {
                    "Manufacturer" => manufacturer = text,
                    "Model" => model = text,
                    "FirmwareVersion" => firmware_version = text,
                    "SerialNumber" => serial_number = text,
                    "HardwareId" => hardware_id = text,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DeviceError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(DeviceInfo {
        manufacturer,
        model,
        firmware_version,
        serial_number,
        hardware_id,
    })
}

fn parse_profiles(xml: &str) -> Result<Vec<MediaProfile>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut profiles = Vec::new();
    let mut current_profile: Option<MediaProfile> = None;
    let mut current_element = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local_name = name.split(':').last().unwrap_or(&name).to_string();

                if local_name == "Profiles" {
                    // Extract token attribute
                    if let Some(token_attr) = e.attributes().filter_map(|a| a.ok()).find(|attr| {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        key.ends_with("token") || key == "token"
                    }) {
                        let token = String::from_utf8_lossy(&token_attr.value).to_string();
                        current_profile = Some(MediaProfile {
                            token,
                            name: String::new(),
                            video_source: None,
                            video_encoder: None,
                        });
                    }
                }

                current_element = local_name;
            }
            Ok(Event::Text(e)) => {
                let text = e
                    .unescape()
                    .map_err(|e| DeviceError::XmlParse(e.to_string()))?
                    .to_string();

                if let Some(ref mut profile) = current_profile {
                    match current_element.as_str() {
                        "Name" => profile.name = text,
                        "SourceToken" => profile.video_source = Some(text),
                        "Encoding" => profile.video_encoder = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local_name = name.split(':').last().unwrap_or(&name).to_string();

                if local_name == "Profiles" {
                    if let Some(profile) = current_profile.take() {
                        profiles.push(profile);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DeviceError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(profiles)
}

fn parse_stream_uri(xml: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut uri = String::new();
    let mut buf = Vec::new();
    let mut current_element = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_element = name.split(':').last().unwrap_or(&name).to_string();
            }
            Ok(Event::Text(e)) => {
                if current_element == "Uri" {
                    uri = e
                        .unescape()
                        .map_err(|e| DeviceError::XmlParse(e.to_string()))?
                        .to_string();
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DeviceError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    if uri.is_empty() {
        return Err(DeviceError::InvalidResponse(
            "No URI found in response".to_string(),
        ));
    }

    Ok(uri)
}

// Convenience functions for backward compatibility
pub async fn get_device_information(
    endpoint: &str,
    auth: Option<(&str, &str)>,
) -> Result<DeviceInfo> {
    DeviceService::new()
        .get_device_information(endpoint, auth)
        .await
}

pub async fn get_profiles(endpoint: &str, auth: Option<(&str, &str)>) -> Result<Vec<MediaProfile>> {
    DeviceService::new().get_profiles(endpoint, auth).await
}

pub async fn get_stream_uri(
    endpoint: &str,
    profile_token: &str,
    auth: Option<(&str, &str)>,
) -> Result<String> {
    DeviceService::new()
        .get_stream_uri(endpoint, profile_token, auth)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_soap_envelope_no_auth() {
        let body =
            r#"<tds:GetDeviceInformation xmlns:tds="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let result = build_soap_envelope(body, None);
        assert!(result.is_ok());

        let envelope = result.unwrap();
        assert!(envelope.contains("s:Envelope"));
        assert!(envelope.contains("s:Header"));
        assert!(envelope.contains("s:Body"));
        assert!(envelope.contains("GetDeviceInformation"));
        assert!(!envelope.contains("wsse:Security"));
    }

    #[test]
    fn test_build_soap_envelope_with_auth() {
        let body =
            r#"<tds:GetDeviceInformation xmlns:tds="http://www.onvif.org/ver10/device/wsdl"/>"#;
        let result = build_soap_envelope(body, Some(("admin", "password123")));
        assert!(result.is_ok());

        let envelope = result.unwrap();
        assert!(envelope.contains("wsse:Security"));
        assert!(envelope.contains("wsse:UsernameToken"));
        assert!(envelope.contains("wsse:Username"));
        assert!(envelope.contains("wsse:Password"));
        assert!(envelope.contains("wsse:Nonce"));
        assert!(envelope.contains("wsu:Created"));
        assert!(envelope.contains("admin"));
    }

    #[test]
    fn test_parse_device_information() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
    <SOAP-ENV:Body>
        <tds:GetDeviceInformationResponse>
            <tds:Manufacturer>Acme Corp</tds:Manufacturer>
            <tds:Model>Camera 3000</tds:Model>
            <tds:FirmwareVersion>1.2.3</tds:FirmwareVersion>
            <tds:SerialNumber>SN123456</tds:SerialNumber>
            <tds:HardwareId>HW-001</tds:HardwareId>
        </tds:GetDeviceInformationResponse>
    </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;

        let result = parse_device_information(xml);
        assert!(result.is_ok());

        let info = result.unwrap();
        assert_eq!(info.manufacturer, "Acme Corp");
        assert_eq!(info.model, "Camera 3000");
        assert_eq!(info.firmware_version, "1.2.3");
        assert_eq!(info.serial_number, "SN123456");
        assert_eq!(info.hardware_id, "HW-001");
    }

    #[test]
    fn test_parse_profiles() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
    <SOAP-ENV:Body>
        <trt:GetProfilesResponse>
            <trt:Profiles token="profile_1">
                <tt:Name>MainStream</tt:Name>
                <tt:VideoSourceConfiguration>
                    <tt:SourceToken>video_source_1</tt:SourceToken>
                </tt:VideoSourceConfiguration>
                <tt:VideoEncoderConfiguration>
                    <tt:Encoding>H264</tt:Encoding>
                </tt:VideoEncoderConfiguration>
            </trt:Profiles>
            <trt:Profiles token="profile_2">
                <tt:Name>SubStream</tt:Name>
            </trt:Profiles>
        </trt:GetProfilesResponse>
    </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;

        let result = parse_profiles(xml);
        assert!(result.is_ok());

        let profiles = result.unwrap();
        assert_eq!(profiles.len(), 2);

        assert_eq!(profiles[0].token, "profile_1");
        assert_eq!(profiles[0].name, "MainStream");

        assert_eq!(profiles[1].token, "profile_2");
        assert_eq!(profiles[1].name, "SubStream");
    }

    #[test]
    fn test_parse_stream_uri() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
    <SOAP-ENV:Body>
        <trt:GetStreamUriResponse>
            <trt:MediaUri>
                <tt:Uri>rtsp://192.168.1.100:554/stream1</tt:Uri>
            </trt:MediaUri>
        </trt:GetStreamUriResponse>
    </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;

        let result = parse_stream_uri(xml);
        assert!(result.is_ok());

        let uri = result.unwrap();
        assert_eq!(uri, "rtsp://192.168.1.100:554/stream1");
    }

    #[test]
    fn test_parse_stream_uri_missing() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
    <SOAP-ENV:Body>
        <trt:GetStreamUriResponse>
        </trt:GetStreamUriResponse>
    </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;

        let result = parse_stream_uri(xml);
        assert!(result.is_err());
    }
}
