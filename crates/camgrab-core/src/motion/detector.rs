use crate::motion::zones::{DetectionZone, ZoneManager};
use chrono::{DateTime, Utc};
use image::GrayImage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MotionError {
    #[error("Frame dimensions do not match: expected {expected_width}x{expected_height}, got {actual_width}x{actual_height}")]
    DimensionMismatch {
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Zone validation error: {0}")]
    ZoneError(#[from] crate::motion::zones::ZoneError),
}

/// Sensitivity presets for motion detection
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Sensitivity {
    Low,
    Medium,
    High,
    Custom(f64),
}

impl Sensitivity {
    pub fn threshold(&self) -> f64 {
        match self {
            Sensitivity::Low => 0.10,    // 10% change required
            Sensitivity::Medium => 0.05, // 5% change required
            Sensitivity::High => 0.02,   // 2% change required
            Sensitivity::Custom(value) => *value,
        }
    }
}

/// Configuration for motion detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionConfig {
    /// Pixel difference threshold (0.0-1.0), how different a pixel must be
    pub threshold: f64,

    /// Minimum percentage of changed area to trigger detection (0.0-100.0)
    pub min_area_percent: f64,

    /// Number of consecutive frames above threshold required
    pub consecutive_frames: u32,

    /// Minimum time between motion events
    #[serde(with = "humantime_serde")]
    pub cooldown: Duration,

    /// Optional detection zones
    pub zones: Vec<DetectionZone>,

    /// Sensitivity preset
    pub sensitivity: Sensitivity,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            threshold: 0.05,
            min_area_percent: 1.0,
            consecutive_frames: 2,
            cooldown: Duration::from_secs(5),
            zones: Vec::new(),
            sensitivity: Sensitivity::Medium,
        }
    }
}

impl MotionConfig {
    pub fn validate(&self) -> Result<(), MotionError> {
        if !(0.0..=1.0).contains(&self.threshold) {
            return Err(MotionError::InvalidConfig(
                "threshold must be between 0.0 and 1.0".to_string(),
            ));
        }

        if !(0.0..=100.0).contains(&self.min_area_percent) {
            return Err(MotionError::InvalidConfig(
                "min_area_percent must be between 0.0 and 100.0".to_string(),
            ));
        }

        if self.consecutive_frames == 0 {
            return Err(MotionError::InvalidConfig(
                "consecutive_frames must be at least 1".to_string(),
            ));
        }

        Ok(())
    }
}

// Custom serde module for Duration
mod humantime_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

/// Bounding box of motion region
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl BoundingBox {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// Motion detection event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionEvent {
    pub timestamp: DateTime<Utc>,
    pub score: f64,
    pub zone_scores: HashMap<String, f64>,
    pub bounding_box: Option<BoundingBox>,
    pub frame_index: u64,
}

impl MotionEvent {
    pub fn new(frame_index: u64, score: f64) -> Self {
        Self {
            timestamp: Utc::now(),
            score,
            zone_scores: HashMap::new(),
            bounding_box: None,
            frame_index,
        }
    }

    #[must_use]
    pub fn with_zones(mut self, zone_scores: HashMap<String, f64>) -> Self {
        self.zone_scores = zone_scores;
        self
    }

    #[must_use]
    pub fn with_bounding_box(mut self, bbox: BoundingBox) -> Self {
        self.bounding_box = Some(bbox);
        self
    }
}

/// Statistics about the detector
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectorStats {
    pub frames_processed: u64,
    pub events_triggered: u64,
    pub avg_score: f64,
    pub uptime: Duration,
}

/// Core motion detection engine
pub struct MotionDetector {
    config: MotionConfig,
    previous_frame: Option<GrayImage>,
    frame_dimensions: Option<(u32, u32)>,
    consecutive_count: u32,
    last_event_time: Option<Instant>,
    zone_manager: Option<ZoneManager>,
    stats: DetectorStats,
    start_time: Instant,
    frame_count: u64,
    total_score: f64,
}

impl MotionDetector {
    /// Create a new motion detector with the given configuration
    pub fn new(config: MotionConfig) -> Result<Self, MotionError> {
        config.validate()?;

        Ok(Self {
            config,
            previous_frame: None,
            frame_dimensions: None,
            consecutive_count: 0,
            last_event_time: None,
            zone_manager: None,
            stats: DetectorStats::default(),
            start_time: Instant::now(),
            frame_count: 0,
            total_score: 0.0,
        })
    }

    /// Feed a frame to the detector and check for motion
    pub fn feed_frame(&mut self, frame: &GrayImage) -> Result<Option<MotionEvent>, MotionError> {
        self.frame_count = self.frame_count.saturating_add(1);
        self.stats.frames_processed = self.stats.frames_processed.saturating_add(1);
        self.stats.uptime = self.start_time.elapsed();

        let width = frame.width();
        let height = frame.height();

        // Initialize dimensions and zone manager on first frame
        if self.frame_dimensions.is_none() {
            self.frame_dimensions = Some((width, height));

            if !self.config.zones.is_empty() {
                let zone_manager = ZoneManager::new(self.config.zones.clone(), width, height);
                zone_manager.validate_zones()?;
                self.zone_manager = Some(zone_manager);
            }
        }

        // Check dimensions match
        let (expected_width, expected_height) = self
            .frame_dimensions
            .expect("frame_dimensions must be initialized above on first frame");
        if width != expected_width || height != expected_height {
            return Err(MotionError::DimensionMismatch {
                expected_width,
                expected_height,
                actual_width: width,
                actual_height: height,
            });
        }

        // Need at least 2 frames to detect motion
        let Some(ref prev_frame) = self.previous_frame else {
            self.previous_frame = Some(frame.clone());
            return Ok(None);
        };

        // Compute frame difference
        let motion_mask = self.compute_difference(prev_frame, frame);

        // Calculate motion score
        let score = Self::calculate_motion_score(&motion_mask);
        self.total_score += score;

        // Update average score
        if self.stats.frames_processed > 0 {
            self.stats.avg_score = self.total_score / self.stats.frames_processed as f64;
        }

        // Check if motion detected
        let threshold = self.config.sensitivity.threshold();
        let motion_detected = score >= threshold && (score * 100.0) >= self.config.min_area_percent;

        if motion_detected {
            self.consecutive_count = self.consecutive_count.saturating_add(1);
        } else {
            self.consecutive_count = 0;
        }

        // Update previous frame
        self.previous_frame = Some(frame.clone());

        // Check if we should trigger an event
        if self.consecutive_count >= self.config.consecutive_frames {
            // Check cooldown
            if let Some(last_time) = self.last_event_time {
                if last_time.elapsed() < self.config.cooldown {
                    return Ok(None);
                }
            }

            // Create motion event
            let mut event = MotionEvent::new(self.frame_count, score);

            // Compute zone scores if zones are configured
            if let Some(ref zone_manager) = self.zone_manager {
                let zone_scores = zone_manager.compute_zone_scores(&motion_mask);
                event = event.with_zones(zone_scores);
            }

            // Calculate bounding box
            if let Some(bbox) = Self::calculate_bounding_box(&motion_mask) {
                event = event.with_bounding_box(bbox);
            }

            self.last_event_time = Some(Instant::now());
            self.stats.events_triggered = self.stats.events_triggered.saturating_add(1);
            self.consecutive_count = 0; // Reset after triggering

            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    /// Compute absolute difference between frames
    fn compute_difference(&self, prev: &GrayImage, current: &GrayImage) -> GrayImage {
        let width = current.width();
        let height = current.height();
        let mut diff = GrayImage::new(width, height);

        let threshold_u8 = (self.config.threshold * 255.0) as u8;

        for y in 0..height {
            for x in 0..width {
                let prev_pixel = prev.get_pixel(x, y)[0];
                let curr_pixel = current.get_pixel(x, y)[0];

                let diff_val = prev_pixel.abs_diff(curr_pixel);

                // Apply threshold to create binary mask
                let binary_val = if diff_val > threshold_u8 { 255 } else { 0 };
                diff.put_pixel(x, y, image::Luma([binary_val]));
            }
        }

        diff
    }

    /// Calculate percentage of pixels with motion
    fn calculate_motion_score(motion_mask: &GrayImage) -> f64 {
        let width = motion_mask.width();
        let height = motion_mask.height();
        let total_pixels = width as u64 * height as u64;

        if total_pixels == 0 {
            return 0.0;
        }

        let mut motion_pixels = 0u64;

        for y in 0..height {
            for x in 0..width {
                if motion_mask.get_pixel(x, y)[0] > 0 {
                    motion_pixels = motion_pixels.saturating_add(1);
                }
            }
        }

        motion_pixels as f64 / total_pixels as f64
    }

    /// Calculate bounding box of motion region
    fn calculate_bounding_box(motion_mask: &GrayImage) -> Option<BoundingBox> {
        let width = motion_mask.width();
        let height = motion_mask.height();

        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        let mut found = false;

        for y in 0..height {
            for x in 0..width {
                if motion_mask.get_pixel(x, y)[0] > 0 {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }

        if !found {
            return None;
        }

        let bbox_width = max_x.saturating_sub(min_x).saturating_add(1);
        let bbox_height = max_y.saturating_sub(min_y).saturating_add(1);

        Some(BoundingBox::new(min_x, min_y, bbox_width, bbox_height))
    }

    /// Reset detector state
    pub fn reset(&mut self) {
        self.previous_frame = None;
        self.frame_dimensions = None;
        self.consecutive_count = 0;
        self.last_event_time = None;
        self.zone_manager = None;
        self.stats = DetectorStats::default();
        self.start_time = Instant::now();
        self.frame_count = 0;
        self.total_score = 0.0;
    }

    /// Get detector statistics
    pub fn stats(&self) -> DetectorStats {
        let mut stats = self.stats.clone();
        stats.uptime = self.start_time.elapsed();
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_config_validation() {
        let mut config = MotionConfig::default();
        assert!(config.validate().is_ok());

        config.threshold = 1.5;
        assert!(config.validate().is_err());

        config.threshold = 0.5;
        config.min_area_percent = 150.0;
        assert!(config.validate().is_err());

        config.min_area_percent = 5.0;
        config.consecutive_frames = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_sensitivity_threshold() {
        assert_eq!(Sensitivity::Low.threshold(), 0.10);
        assert_eq!(Sensitivity::Medium.threshold(), 0.05);
        assert_eq!(Sensitivity::High.threshold(), 0.02);
        assert_eq!(Sensitivity::Custom(0.15).threshold(), 0.15);
    }

    #[test]
    fn test_bounding_box_area() {
        let bbox = BoundingBox::new(10, 10, 50, 30);
        assert_eq!(bbox.area(), 1500);
    }

    #[test]
    fn test_detector_creation() {
        let config = MotionConfig::default();
        let detector = MotionDetector::new(config);
        assert!(detector.is_ok());

        let mut invalid_config = MotionConfig::default();
        invalid_config.threshold = 2.0;
        let detector = MotionDetector::new(invalid_config);
        assert!(detector.is_err());
    }

    #[test]
    fn test_no_motion_on_identical_frames() {
        let config = MotionConfig::default();
        let mut detector = MotionDetector::new(config).unwrap();

        let frame1 = GrayImage::from_raw(10, 10, vec![100u8; 100]).unwrap();
        let frame2 = GrayImage::from_raw(10, 10, vec![100u8; 100]).unwrap();

        let result1 = detector.feed_frame(&frame1);
        assert!(result1.is_ok());
        assert!(result1.unwrap().is_none());

        let result2 = detector.feed_frame(&frame2);
        assert!(result2.is_ok());
        assert!(result2.unwrap().is_none());
    }

    #[test]
    fn test_motion_detection_on_different_frames() {
        let mut config = MotionConfig::default();
        config.threshold = 0.05;
        config.min_area_percent = 1.0;
        config.consecutive_frames = 1;

        let mut detector = MotionDetector::new(config).unwrap();

        let frame1 = GrayImage::from_raw(10, 10, vec![0u8; 100]).unwrap();
        let frame2 = GrayImage::from_raw(10, 10, vec![255u8; 100]).unwrap();

        detector.feed_frame(&frame1).unwrap();
        let result = detector.feed_frame(&frame2).unwrap();

        assert!(result.is_some());
        let event = result.unwrap();
        assert_eq!(event.score, 1.0); // 100% different
    }

    #[test]
    fn test_consecutive_frames_requirement() {
        let mut config = MotionConfig::default();
        config.consecutive_frames = 3;
        config.threshold = 0.05;

        let mut detector = MotionDetector::new(config).unwrap();

        // Use alternating frames to ensure continuous motion detection
        let frame1 = GrayImage::from_raw(10, 10, vec![100u8; 100]).unwrap();
        let frame2 = GrayImage::from_raw(10, 10, vec![200u8; 100]).unwrap();

        detector.feed_frame(&frame1).unwrap();

        // First motion frame - no event yet (count = 1)
        let result = detector.feed_frame(&frame2).unwrap();
        assert!(result.is_none());

        // Second motion frame - no event yet (count = 2)
        let result = detector.feed_frame(&frame1).unwrap();
        assert!(result.is_none());

        // Third motion frame - should trigger (count = 3)
        let result = detector.feed_frame(&frame2).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_cooldown_enforcement() {
        let mut config = MotionConfig::default();
        config.consecutive_frames = 1;
        config.cooldown = Duration::from_secs(2);
        config.threshold = 0.05;

        let mut detector = MotionDetector::new(config).unwrap();

        let frame1 = GrayImage::from_raw(10, 10, vec![0u8; 100]).unwrap();
        let frame2 = GrayImage::from_raw(10, 10, vec![255u8; 100]).unwrap();

        detector.feed_frame(&frame1).unwrap();

        // First motion event
        let result = detector.feed_frame(&frame2).unwrap();
        assert!(result.is_some());

        // Second motion event should be blocked by cooldown
        detector.feed_frame(&frame1).unwrap();
        let result = detector.feed_frame(&frame2).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_bounding_box_calculation() {
        let mut config = MotionConfig::default();
        config.consecutive_frames = 1;
        config.threshold = 0.05;

        let mut detector = MotionDetector::new(config).unwrap();

        // Create a frame with motion in specific region
        let data1 = vec![0u8; 100];
        let mut data2 = vec![0u8; 100];

        // Add motion in region (5,5) to (7,7)
        for y in 5..8 {
            for x in 5..8 {
                data2[y * 10 + x] = 255;
            }
        }

        let frame1 = GrayImage::from_raw(10, 10, data1).unwrap();
        let frame2 = GrayImage::from_raw(10, 10, data2).unwrap();

        detector.feed_frame(&frame1).unwrap();
        let result = detector.feed_frame(&frame2).unwrap();

        assert!(result.is_some());
        let event = result.unwrap();
        assert!(event.bounding_box.is_some());

        let bbox = event.bounding_box.unwrap();
        assert_eq!(bbox.x, 5);
        assert_eq!(bbox.y, 5);
        assert_eq!(bbox.width, 3);
        assert_eq!(bbox.height, 3);
    }

    #[test]
    fn test_frame_dimension_mismatch() {
        let config = MotionConfig::default();
        let mut detector = MotionDetector::new(config).unwrap();

        let frame1 = GrayImage::from_raw(10, 10, vec![0u8; 100]).unwrap();
        let frame2 = GrayImage::from_raw(20, 20, vec![0u8; 400]).unwrap();

        detector.feed_frame(&frame1).unwrap();
        let result = detector.feed_frame(&frame2);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            MotionError::DimensionMismatch { .. }
        ));
    }

    #[test]
    fn test_detector_reset() {
        let config = MotionConfig::default();
        let mut detector = MotionDetector::new(config).unwrap();

        let frame = GrayImage::from_raw(10, 10, vec![100u8; 100]).unwrap();
        detector.feed_frame(&frame).unwrap();

        assert_eq!(detector.stats().frames_processed, 1);

        detector.reset();

        assert_eq!(detector.stats().frames_processed, 0);
        assert!(detector.previous_frame.is_none());
        assert!(detector.frame_dimensions.is_none());
    }

    #[test]
    fn test_detector_stats() {
        let mut config = MotionConfig::default();
        config.consecutive_frames = 1;
        config.threshold = 0.05;

        let mut detector = MotionDetector::new(config).unwrap();

        let frame1 = GrayImage::from_raw(10, 10, vec![0u8; 100]).unwrap();
        let frame2 = GrayImage::from_raw(10, 10, vec![255u8; 100]).unwrap();

        detector.feed_frame(&frame1).unwrap();
        detector.feed_frame(&frame2).unwrap();

        let stats = detector.stats();
        assert_eq!(stats.frames_processed, 2);
        assert_eq!(stats.events_triggered, 1);
        assert!(stats.avg_score > 0.0);
    }

    #[test]
    fn test_partial_frame_change() {
        let mut config = MotionConfig::default();
        config.consecutive_frames = 1;
        config.threshold = 0.05;
        config.min_area_percent = 5.0; // Need at least 5% change

        let mut detector = MotionDetector::new(config).unwrap();

        let data1 = vec![100u8; 100];
        let mut data2 = vec![100u8; 100];

        // Change only 3 pixels (3% of 100)
        data2[0] = 200;
        data2[1] = 200;
        data2[2] = 200;

        let frame1 = GrayImage::from_raw(10, 10, data1).unwrap();
        let frame2 = GrayImage::from_raw(10, 10, data2).unwrap();

        detector.feed_frame(&frame1).unwrap();
        let result = detector.feed_frame(&frame2).unwrap();

        // Should not trigger (only 3% changed, need 5%)
        assert!(result.is_none());
    }
}
