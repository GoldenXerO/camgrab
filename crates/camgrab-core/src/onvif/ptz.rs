use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Client;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PtzError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("XML parsing error: {0}")]
    XmlParse(String),

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),

    #[error("Device error: {0}")]
    Device(#[from] super::device::DeviceError),

    #[error("PTZ not supported")]
    NotSupported,

    #[error("Invalid position: {0}")]
    InvalidPosition(String),

    #[error("Preset not found: {0}")]
    PresetNotFound(String),
}

pub type Result<T> = std::result::Result<T, PtzError>;

#[derive(Debug, Clone, PartialEq)]
pub struct PtzPosition {
    pub pan: f64,
    pub tilt: f64,
    pub zoom: f64,
}

impl PtzPosition {
    pub fn new(pan: f64, tilt: f64, zoom: f64) -> Self {
        Self { pan, tilt, zoom }
    }

    pub fn validate(&self) -> Result<()> {
        if !(-1.0..=1.0).contains(&self.pan) {
            return Err(PtzError::InvalidPosition(format!(
                "Pan {} out of range [-1.0, 1.0]",
                self.pan
            )));
        }
        if !(-1.0..=1.0).contains(&self.tilt) {
            return Err(PtzError::InvalidPosition(format!(
                "Tilt {} out of range [-1.0, 1.0]",
                self.tilt
            )));
        }
        if !(0.0..=1.0).contains(&self.zoom) {
            return Err(PtzError::InvalidPosition(format!(
                "Zoom {} out of range [0.0, 1.0]",
                self.zoom
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PtzRange {
    pub min: f64,
    pub max: f64,
}

impl PtzRange {
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }
}

impl Default for PtzRange {
    fn default() -> Self {
        Self {
            min: -1.0,
            max: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PtzCapabilities {
    pub pan_range: PtzRange,
    pub tilt_range: PtzRange,
    pub zoom_range: PtzRange,
    pub presets: bool,
}

impl Default for PtzCapabilities {
    fn default() -> Self {
        Self {
            pan_range: PtzRange::default(),
            tilt_range: PtzRange::default(),
            zoom_range: PtzRange::new(0.0, 1.0),
            presets: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PtzPreset {
    pub token: String,
    pub name: String,
    pub position: Option<PtzPosition>,
}

impl PtzPreset {
    pub fn new(token: String, name: String) -> Self {
        Self {
            token,
            name,
            position: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PtzCommand {
    AbsoluteMove(PtzPosition),
    RelativeMove(PtzPosition),
    ContinuousMove(PtzPosition),
    Stop,
    GotoPreset(String),
    SetPreset(String),
    RemovePreset(String),
    GotoHome,
}

#[derive(Debug, Clone)]
pub struct PtzController {
    client: Client,
    endpoint: String,
    profile_token: String,
    auth: Option<(String, String)>,
}

impl PtzController {
    pub fn new(endpoint: &str, profile_token: &str, auth: Option<(&str, &str)>) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            endpoint: endpoint.to_string(),
            profile_token: profile_token.to_string(),
            auth: auth.map(|(u, p)| (u.to_string(), p.to_string())),
        }
    }

    pub async fn execute(&self, command: PtzCommand) -> Result<()> {
        let soap_body = self.build_command_body(&command)?;
        let auth_ref = self.auth.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
        let envelope = super::device::build_soap_envelope(&soap_body, auth_ref)?;

        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(PtzError::InvalidResponse(format!(
                "HTTP status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    pub async fn get_position(&self) -> Result<PtzPosition> {
        let soap_body = format!(
            r#"<tptz:GetStatus xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                <tptz:ProfileToken>{}</tptz:ProfileToken>
            </tptz:GetStatus>"#,
            self.profile_token
        );

        let auth_ref = self.auth.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
        let envelope = super::device::build_soap_envelope(&soap_body, auth_ref)?;

        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(PtzError::InvalidResponse(format!(
                "HTTP status: {}",
                response.status()
            )));
        }

        let body = response.text().await?;
        parse_position(&body)
    }

    pub async fn get_presets(&self) -> Result<Vec<PtzPreset>> {
        let soap_body = format!(
            r#"<tptz:GetPresets xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                <tptz:ProfileToken>{}</tptz:ProfileToken>
            </tptz:GetPresets>"#,
            self.profile_token
        );

        let auth_ref = self.auth.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
        let envelope = super::device::build_soap_envelope(&soap_body, auth_ref)?;

        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(PtzError::InvalidResponse(format!(
                "HTTP status: {}",
                response.status()
            )));
        }

        let body = response.text().await?;
        parse_presets(&body)
    }

    pub async fn get_capabilities(&self) -> Result<PtzCapabilities> {
        let soap_body = format!(
            r#"<tptz:GetConfigurationOptions xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                <tptz:ConfigurationToken>{}</tptz:ConfigurationToken>
            </tptz:GetConfigurationOptions>"#,
            self.profile_token
        );

        let auth_ref = self.auth.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
        let envelope = super::device::build_soap_envelope(&soap_body, auth_ref)?;

        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(PtzError::InvalidResponse(format!(
                "HTTP status: {}",
                response.status()
            )));
        }

        let body = response.text().await?;
        parse_capabilities(&body)
    }

    fn build_command_body(&self, command: &PtzCommand) -> Result<String> {
        match command {
            PtzCommand::AbsoluteMove(position) => {
                position.validate()?;
                Ok(format!(
                    r#"<tptz:AbsoluteMove xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                        <tptz:ProfileToken>{}</tptz:ProfileToken>
                        <tptz:Position>
                            <tt:PanTilt x="{}" y="{}" xmlns:tt="http://www.onvif.org/ver10/schema"/>
                            <tt:Zoom x="{}" xmlns:tt="http://www.onvif.org/ver10/schema"/>
                        </tptz:Position>
                    </tptz:AbsoluteMove>"#,
                    self.profile_token, position.pan, position.tilt, position.zoom
                ))
            }
            PtzCommand::RelativeMove(position) => {
                position.validate()?;
                Ok(format!(
                    r#"<tptz:RelativeMove xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                        <tptz:ProfileToken>{}</tptz:ProfileToken>
                        <tptz:Translation>
                            <tt:PanTilt x="{}" y="{}" xmlns:tt="http://www.onvif.org/ver10/schema"/>
                            <tt:Zoom x="{}" xmlns:tt="http://www.onvif.org/ver10/schema"/>
                        </tptz:Translation>
                    </tptz:RelativeMove>"#,
                    self.profile_token, position.pan, position.tilt, position.zoom
                ))
            }
            PtzCommand::ContinuousMove(velocity) => {
                velocity.validate()?;
                Ok(format!(
                    r#"<tptz:ContinuousMove xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                        <tptz:ProfileToken>{}</tptz:ProfileToken>
                        <tptz:Velocity>
                            <tt:PanTilt x="{}" y="{}" xmlns:tt="http://www.onvif.org/ver10/schema"/>
                            <tt:Zoom x="{}" xmlns:tt="http://www.onvif.org/ver10/schema"/>
                        </tptz:Velocity>
                    </tptz:ContinuousMove>"#,
                    self.profile_token, velocity.pan, velocity.tilt, velocity.zoom
                ))
            }
            PtzCommand::Stop => Ok(format!(
                r#"<tptz:Stop xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                        <tptz:ProfileToken>{}</tptz:ProfileToken>
                        <tptz:PanTilt>true</tptz:PanTilt>
                        <tptz:Zoom>true</tptz:Zoom>
                    </tptz:Stop>"#,
                self.profile_token
            )),
            PtzCommand::GotoPreset(preset_token) => Ok(format!(
                r#"<tptz:GotoPreset xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                        <tptz:ProfileToken>{}</tptz:ProfileToken>
                        <tptz:PresetToken>{}</tptz:PresetToken>
                    </tptz:GotoPreset>"#,
                self.profile_token, preset_token
            )),
            PtzCommand::SetPreset(preset_name) => Ok(format!(
                r#"<tptz:SetPreset xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                        <tptz:ProfileToken>{}</tptz:ProfileToken>
                        <tptz:PresetName>{}</tptz:PresetName>
                    </tptz:SetPreset>"#,
                self.profile_token, preset_name
            )),
            PtzCommand::RemovePreset(preset_token) => Ok(format!(
                r#"<tptz:RemovePreset xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                        <tptz:ProfileToken>{}</tptz:ProfileToken>
                        <tptz:PresetToken>{}</tptz:PresetToken>
                    </tptz:RemovePreset>"#,
                self.profile_token, preset_token
            )),
            PtzCommand::GotoHome => Ok(format!(
                r#"<tptz:GotoHomePosition xmlns:tptz="http://www.onvif.org/ver20/ptz/wsdl">
                        <tptz:ProfileToken>{}</tptz:ProfileToken>
                    </tptz:GotoHomePosition>"#,
                self.profile_token
            )),
        }
    }
}

fn parse_position(xml: &str) -> Result<PtzPosition> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut pan = 0.0;
    let mut tilt = 0.0;
    let mut zoom = 0.0;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local_name = name.split(':').last().unwrap_or(&name).to_string();

                if local_name == "PanTilt" {
                    for attr in e.attributes().filter_map(|a| a.ok()) {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        let value = String::from_utf8_lossy(&attr.value);

                        if key == "x" {
                            pan = value.parse().unwrap_or(0.0);
                        } else if key == "y" {
                            tilt = value.parse().unwrap_or(0.0);
                        }
                    }
                } else if local_name == "Zoom" {
                    for attr in e.attributes().filter_map(|a| a.ok()) {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        let value = String::from_utf8_lossy(&attr.value);

                        if key == "x" {
                            zoom = value.parse().unwrap_or(0.0);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(PtzError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(PtzPosition { pan, tilt, zoom })
}

fn parse_presets(xml: &str) -> Result<Vec<PtzPreset>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut presets = Vec::new();
    let mut current_preset: Option<PtzPreset> = None;
    let mut current_element = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local_name = name.split(':').last().unwrap_or(&name).to_string();

                if local_name == "Preset" {
                    // Extract token attribute
                    if let Some(token_attr) = e.attributes().filter_map(|a| a.ok()).find(|attr| {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        key == "token"
                    }) {
                        let token = String::from_utf8_lossy(&token_attr.value).to_string();
                        current_preset = Some(PtzPreset::new(token, String::new()));
                    }
                }

                current_element = local_name;
            }
            Ok(Event::Text(e)) => {
                let text = e
                    .unescape()
                    .map_err(|e| PtzError::XmlParse(e.to_string()))?
                    .to_string();

                if let Some(ref mut preset) = current_preset {
                    if current_element == "Name" {
                        preset.name = text;
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local_name = name.split(':').last().unwrap_or(&name).to_string();

                if local_name == "Preset" {
                    if let Some(preset) = current_preset.take() {
                        presets.push(preset);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(PtzError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(presets)
}

fn parse_capabilities(xml: &str) -> Result<PtzCapabilities> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut capabilities = PtzCapabilities::default();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e) | Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local_name = name.split(':').last().unwrap_or(&name).to_string();

                if local_name == "PanTiltLimits" || local_name == "ZoomLimits" {
                    for attr in e.attributes().filter_map(|a| a.ok()) {
                        let key = String::from_utf8_lossy(attr.key.as_ref());
                        let value = String::from_utf8_lossy(&attr.value);

                        if local_name == "PanTiltLimits" {
                            if key == "min" {
                                let min = value.parse().unwrap_or(-1.0);
                                capabilities.pan_range.min = min;
                                capabilities.tilt_range.min = min;
                            } else if key == "max" {
                                let max = value.parse().unwrap_or(1.0);
                                capabilities.pan_range.max = max;
                                capabilities.tilt_range.max = max;
                            }
                        } else if local_name == "ZoomLimits" {
                            if key == "min" {
                                capabilities.zoom_range.min = value.parse().unwrap_or(0.0);
                            } else if key == "max" {
                                capabilities.zoom_range.max = value.parse().unwrap_or(1.0);
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(PtzError::XmlParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    // Assume presets are supported if no error occurred
    capabilities.presets = true;

    Ok(capabilities)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ptz_position_validate() {
        let valid_pos = PtzPosition::new(0.5, -0.3, 0.8);
        assert!(valid_pos.validate().is_ok());

        let invalid_pan = PtzPosition::new(1.5, 0.0, 0.5);
        assert!(invalid_pan.validate().is_err());

        let invalid_tilt = PtzPosition::new(0.0, -1.5, 0.5);
        assert!(invalid_tilt.validate().is_err());

        let invalid_zoom = PtzPosition::new(0.0, 0.0, 1.5);
        assert!(invalid_zoom.validate().is_err());
    }

    #[test]
    fn test_ptz_controller_build_absolute_move() {
        let controller = PtzController::new("http://192.168.1.100/onvif/ptz", "profile_1", None);

        let position = PtzPosition::new(0.5, -0.3, 0.8);
        let command = PtzCommand::AbsoluteMove(position);
        let result = controller.build_command_body(&command);

        assert!(result.is_ok());
        let body = result.unwrap();
        assert!(body.contains("AbsoluteMove"));
        assert!(body.contains("profile_1"));
        assert!(body.contains("0.5"));
        assert!(body.contains("-0.3"));
        assert!(body.contains("0.8"));
    }

    #[test]
    fn test_ptz_controller_build_relative_move() {
        let controller = PtzController::new("http://192.168.1.100/onvif/ptz", "profile_1", None);

        let position = PtzPosition::new(0.1, 0.2, 0.0);
        let command = PtzCommand::RelativeMove(position);
        let result = controller.build_command_body(&command);

        assert!(result.is_ok());
        let body = result.unwrap();
        assert!(body.contains("RelativeMove"));
        assert!(body.contains("Translation"));
    }

    #[test]
    fn test_ptz_controller_build_continuous_move() {
        let controller = PtzController::new("http://192.168.1.100/onvif/ptz", "profile_1", None);

        let velocity = PtzPosition::new(0.5, 0.5, 0.0);
        let command = PtzCommand::ContinuousMove(velocity);
        let result = controller.build_command_body(&command);

        assert!(result.is_ok());
        let body = result.unwrap();
        assert!(body.contains("ContinuousMove"));
        assert!(body.contains("Velocity"));
    }

    #[test]
    fn test_ptz_controller_build_stop() {
        let controller = PtzController::new("http://192.168.1.100/onvif/ptz", "profile_1", None);

        let command = PtzCommand::Stop;
        let result = controller.build_command_body(&command);

        assert!(result.is_ok());
        let body = result.unwrap();
        assert!(body.contains("Stop"));
        assert!(body.contains("PanTilt"));
        assert!(body.contains("Zoom"));
    }

    #[test]
    fn test_ptz_controller_build_goto_preset() {
        let controller = PtzController::new("http://192.168.1.100/onvif/ptz", "profile_1", None);

        let command = PtzCommand::GotoPreset("preset_1".to_string());
        let result = controller.build_command_body(&command);

        assert!(result.is_ok());
        let body = result.unwrap();
        assert!(body.contains("GotoPreset"));
        assert!(body.contains("preset_1"));
    }

    #[test]
    fn test_ptz_controller_build_set_preset() {
        let controller = PtzController::new("http://192.168.1.100/onvif/ptz", "profile_1", None);

        let command = PtzCommand::SetPreset("MyPreset".to_string());
        let result = controller.build_command_body(&command);

        assert!(result.is_ok());
        let body = result.unwrap();
        assert!(body.contains("SetPreset"));
        assert!(body.contains("MyPreset"));
    }

    #[test]
    fn test_ptz_controller_build_goto_home() {
        let controller = PtzController::new("http://192.168.1.100/onvif/ptz", "profile_1", None);

        let command = PtzCommand::GotoHome;
        let result = controller.build_command_body(&command);

        assert!(result.is_ok());
        let body = result.unwrap();
        assert!(body.contains("GotoHomePosition"));
    }

    #[test]
    fn test_parse_position() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
    <SOAP-ENV:Body>
        <tptz:GetStatusResponse>
            <tptz:PTZStatus>
                <tt:Position>
                    <tt:PanTilt x="0.5" y="-0.3"/>
                    <tt:Zoom x="0.8"/>
                </tt:Position>
            </tptz:PTZStatus>
        </tptz:GetStatusResponse>
    </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;

        let result = parse_position(xml);
        assert!(result.is_ok());

        let position = result.unwrap();
        assert_eq!(position.pan, 0.5);
        assert_eq!(position.tilt, -0.3);
        assert_eq!(position.zoom, 0.8);
    }

    #[test]
    fn test_parse_presets() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
    <SOAP-ENV:Body>
        <tptz:GetPresetsResponse>
            <tptz:Preset token="preset_1">
                <tt:Name>Home</tt:Name>
            </tptz:Preset>
            <tptz:Preset token="preset_2">
                <tt:Name>Entrance</tt:Name>
            </tptz:Preset>
        </tptz:GetPresetsResponse>
    </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;

        let result = parse_presets(xml);
        assert!(result.is_ok());

        let presets = result.unwrap();
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].token, "preset_1");
        assert_eq!(presets[0].name, "Home");
        assert_eq!(presets[1].token, "preset_2");
        assert_eq!(presets[1].name, "Entrance");
    }

    #[test]
    fn test_ptz_range_default() {
        let range = PtzRange::default();
        assert_eq!(range.min, -1.0);
        assert_eq!(range.max, 1.0);
    }

    #[test]
    fn test_ptz_capabilities_default() {
        let caps = PtzCapabilities::default();
        assert_eq!(caps.pan_range.min, -1.0);
        assert_eq!(caps.pan_range.max, 1.0);
        assert_eq!(caps.zoom_range.min, 0.0);
        assert_eq!(caps.zoom_range.max, 1.0);
        assert!(!caps.presets);
    }

    #[test]
    fn test_parse_capabilities() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope">
    <SOAP-ENV:Body>
        <tptz:GetConfigurationOptionsResponse>
            <tptz:PTZConfigurationOptions>
                <tt:Spaces>
                    <tt:PanTiltLimits min="-1.0" max="1.0"/>
                    <tt:ZoomLimits min="0.0" max="1.0"/>
                </tt:Spaces>
            </tptz:PTZConfigurationOptions>
        </tptz:GetConfigurationOptionsResponse>
    </SOAP-ENV:Body>
</SOAP-ENV:Envelope>"#;

        let result = parse_capabilities(xml);
        assert!(result.is_ok());

        let caps = result.unwrap();
        assert!(caps.presets);
    }
}
