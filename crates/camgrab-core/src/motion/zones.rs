use image::GrayImage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZoneError {
    #[error("Zone '{0}' is out of frame bounds")]
    OutOfBounds(String),
    #[error("Polygon zone '{0}' has fewer than 3 points")]
    InsufficientPoints(String),
    #[error("Invalid region dimensions")]
    InvalidDimensions,
}

/// A point in 2D space
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

impl Point {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// Region definition for detection zones
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Region {
    Rect {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    Polygon(Vec<Point>),
}

/// A detection zone with optional custom sensitivity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionZone {
    pub name: String,
    pub region: Region,
    pub sensitivity_override: Option<f64>,
    pub enabled: bool,
}

impl DetectionZone {
    pub fn new(name: String, region: Region) -> Self {
        Self {
            name,
            region,
            sensitivity_override: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn with_sensitivity(mut self, sensitivity: f64) -> Self {
        self.sensitivity_override = Some(sensitivity);
        self
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Manages detection zones and computes per-zone motion scores
pub struct ZoneManager {
    zones: Vec<DetectionZone>,
    frame_width: u32,
    frame_height: u32,
}

impl ZoneManager {
    /// Create a new zone manager
    pub fn new(zones: Vec<DetectionZone>, frame_width: u32, frame_height: u32) -> Self {
        Self {
            zones,
            frame_width,
            frame_height,
        }
    }

    /// Validate that all zones fit within the frame dimensions
    pub fn validate_zones(&self) -> Result<(), ZoneError> {
        for zone in &self.zones {
            match &zone.region {
                Region::Rect {
                    x,
                    y,
                    width,
                    height,
                } => {
                    if *width == 0 || *height == 0 {
                        return Err(ZoneError::InvalidDimensions);
                    }

                    // Check for overflow
                    let x_end = x.checked_add(*width).ok_or(ZoneError::InvalidDimensions)?;
                    let y_end = y.checked_add(*height).ok_or(ZoneError::InvalidDimensions)?;

                    if x_end > self.frame_width || y_end > self.frame_height {
                        return Err(ZoneError::OutOfBounds(zone.name.clone()));
                    }
                }
                Region::Polygon(points) => {
                    if points.len() < 3 {
                        return Err(ZoneError::InsufficientPoints(zone.name.clone()));
                    }

                    for point in points {
                        if point.x >= self.frame_width || point.y >= self.frame_height {
                            return Err(ZoneError::OutOfBounds(zone.name.clone()));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Compute motion scores for each enabled zone
    pub fn compute_zone_scores(&self, motion_mask: &GrayImage) -> HashMap<String, f64> {
        let mut scores = HashMap::new();

        for zone in &self.zones {
            if !zone.enabled {
                continue;
            }

            let score = Self::compute_zone_score(zone, motion_mask);
            scores.insert(zone.name.clone(), score);
        }

        scores
    }

    /// Compute motion score for a single zone
    fn compute_zone_score(zone: &DetectionZone, motion_mask: &GrayImage) -> f64 {
        let (motion_pixels, total_pixels) = match &zone.region {
            Region::Rect {
                x,
                y,
                width,
                height,
            } => Self::count_rect_pixels(*x, *y, *width, *height, motion_mask),
            Region::Polygon(points) => Self::count_polygon_pixels(points, motion_mask),
        };

        if total_pixels == 0 {
            return 0.0;
        }

        motion_pixels as f64 / total_pixels as f64
    }

    /// Count motion pixels in a rectangular region
    fn count_rect_pixels(
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        motion_mask: &GrayImage,
    ) -> (u64, u64) {
        let mut motion_pixels = 0u64;
        let mut total_pixels = 0u64;

        let img_width = motion_mask.width();
        let img_height = motion_mask.height();

        let x_end = x.saturating_add(width).min(img_width);
        let y_end = y.saturating_add(height).min(img_height);

        for py in y..y_end {
            for px in x..x_end {
                if let Some(pixel) = motion_mask.get_pixel_checked(px, py) {
                    total_pixels = total_pixels.saturating_add(1);
                    if pixel[0] > 0 {
                        motion_pixels = motion_pixels.saturating_add(1);
                    }
                }
            }
        }

        (motion_pixels, total_pixels)
    }

    /// Count motion pixels in a polygon region using ray casting
    fn count_polygon_pixels(points: &[Point], motion_mask: &GrayImage) -> (u64, u64) {
        let mut motion_pixels = 0u64;
        let mut total_pixels = 0u64;

        // Find bounding box of polygon
        let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
        let max_x = points
            .iter()
            .map(|p| p.x)
            .max()
            .unwrap_or(0)
            .min(motion_mask.width().saturating_sub(1));
        let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
        let max_y = points
            .iter()
            .map(|p| p.y)
            .max()
            .unwrap_or(0)
            .min(motion_mask.height().saturating_sub(1));

        // Check each pixel in bounding box
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                if Self::is_point_in_polygon(px, py, points) {
                    if let Some(pixel) = motion_mask.get_pixel_checked(px, py) {
                        total_pixels = total_pixels.saturating_add(1);
                        if pixel[0] > 0 {
                            motion_pixels = motion_pixels.saturating_add(1);
                        }
                    }
                }
            }
        }

        (motion_pixels, total_pixels)
    }

    /// Check if a point is inside a zone using ray casting algorithm
    pub fn is_point_in_zone(&self, zone: &DetectionZone, x: u32, y: u32) -> bool {
        match &zone.region {
            Region::Rect {
                x: rx,
                y: ry,
                width,
                height,
            } => {
                let x_end = rx.saturating_add(*width);
                let y_end = ry.saturating_add(*height);
                x >= *rx && x < x_end && y >= *ry && y < y_end
            }
            Region::Polygon(points) => Self::is_point_in_polygon(x, y, points),
        }
    }

    /// Ray casting algorithm for point-in-polygon test
    fn is_point_in_polygon(x: u32, y: u32, points: &[Point]) -> bool {
        if points.len() < 3 {
            return false;
        }

        let mut inside = false;
        let mut j = points.len() - 1;

        for i in 0..points.len() {
            let xi = points[i].x as i64;
            let yi = points[i].y as i64;
            let xj = points[j].x as i64;
            let yj = points[j].y as i64;
            let x_test = x as i64;
            let y_test = y as i64;

            // Check if point is on an edge (ray crosses)
            let intersect = ((yi > y_test) != (yj > y_test))
                && (x_test < (xj - xi) * (y_test - yi) / (yj - yi) + xi);

            if intersect {
                inside = !inside;
            }

            j = i;
        }

        inside
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_creation() {
        let point = Point::new(10, 20);
        assert_eq!(point.x, 10);
        assert_eq!(point.y, 20);
    }

    #[test]
    fn test_rect_zone_validation() {
        let zone = DetectionZone::new(
            "test".to_string(),
            Region::Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
        );

        let manager = ZoneManager::new(vec![zone], 640, 480);
        assert!(manager.validate_zones().is_ok());
    }

    #[test]
    fn test_rect_zone_out_of_bounds() {
        let zone = DetectionZone::new(
            "test".to_string(),
            Region::Rect {
                x: 600,
                y: 0,
                width: 100,
                height: 100,
            },
        );

        let manager = ZoneManager::new(vec![zone], 640, 480);
        assert!(matches!(
            manager.validate_zones(),
            Err(ZoneError::OutOfBounds(_))
        ));
    }

    #[test]
    fn test_polygon_zone_validation() {
        let zone = DetectionZone::new(
            "triangle".to_string(),
            Region::Polygon(vec![
                Point::new(0, 0),
                Point::new(100, 0),
                Point::new(50, 100),
            ]),
        );

        let manager = ZoneManager::new(vec![zone], 640, 480);
        assert!(manager.validate_zones().is_ok());
    }

    #[test]
    fn test_polygon_insufficient_points() {
        let zone = DetectionZone::new(
            "line".to_string(),
            Region::Polygon(vec![Point::new(0, 0), Point::new(100, 100)]),
        );

        let manager = ZoneManager::new(vec![zone], 640, 480);
        assert!(matches!(
            manager.validate_zones(),
            Err(ZoneError::InsufficientPoints(_))
        ));
    }

    #[test]
    fn test_point_in_rect_zone() {
        let zone = DetectionZone::new(
            "test".to_string(),
            Region::Rect {
                x: 10,
                y: 10,
                width: 50,
                height: 50,
            },
        );

        let manager = ZoneManager::new(vec![zone.clone()], 640, 480);

        assert!(manager.is_point_in_zone(&zone, 30, 30));
        assert!(manager.is_point_in_zone(&zone, 10, 10));
        assert!(!manager.is_point_in_zone(&zone, 60, 30));
        assert!(!manager.is_point_in_zone(&zone, 5, 5));
    }

    #[test]
    fn test_point_in_polygon_zone() {
        let zone = DetectionZone::new(
            "triangle".to_string(),
            Region::Polygon(vec![
                Point::new(50, 0),
                Point::new(100, 100),
                Point::new(0, 100),
            ]),
        );

        let manager = ZoneManager::new(vec![zone.clone()], 640, 480);

        // Point inside triangle
        assert!(manager.is_point_in_zone(&zone, 50, 50));

        // Points outside triangle
        assert!(!manager.is_point_in_zone(&zone, 0, 0));
        assert!(!manager.is_point_in_zone(&zone, 100, 0));
        assert!(!manager.is_point_in_zone(&zone, 150, 150));
    }

    #[test]
    fn test_zone_score_computation() {
        // Create a simple 10x10 motion mask with half the pixels set
        let mut motion_mask = GrayImage::new(10, 10);
        for y in 0..10 {
            for x in 0..5 {
                motion_mask.put_pixel(x, y, image::Luma([255u8]));
            }
        }

        let zone = DetectionZone::new(
            "test".to_string(),
            Region::Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        );

        let manager = ZoneManager::new(vec![zone], 10, 10);
        let scores = manager.compute_zone_scores(&motion_mask);

        assert_eq!(scores.len(), 1);
        assert!((scores["test"] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_disabled_zone_not_computed() {
        let mut motion_mask = GrayImage::new(10, 10);
        for y in 0..10 {
            for x in 0..10 {
                motion_mask.put_pixel(x, y, image::Luma([255u8]));
            }
        }

        let zone = DetectionZone::new(
            "test".to_string(),
            Region::Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        )
        .disabled();

        let manager = ZoneManager::new(vec![zone], 10, 10);
        let scores = manager.compute_zone_scores(&motion_mask);

        assert_eq!(scores.len(), 0);
    }

    #[test]
    fn test_zero_dimension_rect_invalid() {
        let zone = DetectionZone::new(
            "test".to_string(),
            Region::Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 100,
            },
        );

        let manager = ZoneManager::new(vec![zone], 640, 480);
        assert!(matches!(
            manager.validate_zones(),
            Err(ZoneError::InvalidDimensions)
        ));
    }
}
