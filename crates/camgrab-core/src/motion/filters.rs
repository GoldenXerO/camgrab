use image::GrayImage;

/// Trait for noise reduction filters
pub trait NoiseFilter: Send + Sync {
    fn apply(&self, frame: &mut GrayImage);
}

/// Gaussian blur filter for noise reduction
#[derive(Debug, Clone)]
pub struct GaussianBlur {
    pub kernel_size: u32,
    pub sigma: f64,
}

impl GaussianBlur {
    pub fn new(kernel_size: u32, sigma: f64) -> Self {
        Self { kernel_size, sigma }
    }

    /// Generate 1D Gaussian kernel
    fn generate_kernel(&self) -> Vec<f64> {
        let size = self.kernel_size as i32;
        let center = size / 2;
        let mut kernel = Vec::with_capacity(self.kernel_size as usize);

        let mut sum = 0.0;
        for i in 0..size {
            let x = (i - center) as f64;
            let value = (-x * x / (2.0 * self.sigma * self.sigma)).exp();
            kernel.push(value);
            sum += value;
        }

        // Normalize
        for value in &mut kernel {
            *value /= sum;
        }

        kernel
    }
}

impl NoiseFilter for GaussianBlur {
    fn apply(&self, frame: &mut GrayImage) {
        if self.kernel_size < 3 || self.kernel_size % 2 == 0 {
            return; // Invalid kernel size
        }

        let kernel = self.generate_kernel();
        let width = frame.width();
        let height = frame.height();
        let radius = (self.kernel_size / 2) as i32;

        // Create temporary buffer
        let mut temp = frame.clone();

        // Horizontal pass
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0.0;
                for i in 0..self.kernel_size as i32 {
                    let px = (x as i32 + i - radius).clamp(0, width as i32 - 1) as u32;
                    let pixel = temp.get_pixel(px, y)[0] as f64;
                    sum += pixel * kernel[i as usize];
                }
                frame.put_pixel(x, y, image::Luma([sum.round() as u8]));
            }
        }

        // Vertical pass
        temp.clone_from(frame);
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0.0;
                for i in 0..self.kernel_size as i32 {
                    let py = (y as i32 + i - radius).clamp(0, height as i32 - 1) as u32;
                    let pixel = temp.get_pixel(x, py)[0] as f64;
                    sum += pixel * kernel[i as usize];
                }
                frame.put_pixel(x, y, image::Luma([sum.round() as u8]));
            }
        }
    }
}

/// Median filter for salt-and-pepper noise
#[derive(Debug, Clone)]
pub struct MedianFilter {
    pub kernel_size: u32,
}

impl MedianFilter {
    pub fn new(kernel_size: u32) -> Self {
        Self { kernel_size }
    }
}

impl NoiseFilter for MedianFilter {
    fn apply(&self, frame: &mut GrayImage) {
        if self.kernel_size < 3 || self.kernel_size % 2 == 0 {
            return; // Invalid kernel size
        }

        let width = frame.width();
        let height = frame.height();
        let radius = (self.kernel_size / 2) as i32;
        let temp = frame.clone();

        for y in 0..height {
            for x in 0..width {
                let mut values = Vec::new();

                // Collect neighborhood pixels
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        let px = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                        let py = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                        values.push(temp.get_pixel(px, py)[0]);
                    }
                }

                // Find median
                values.sort_unstable();
                let median = values[values.len() / 2];
                frame.put_pixel(x, y, image::Luma([median]));
            }
        }
    }
}

/// Erosion followed by dilation to remove small noise
#[derive(Debug, Clone)]
pub struct ErosionDilation {
    pub iterations: u32,
}

impl ErosionDilation {
    pub fn new(iterations: u32) -> Self {
        Self { iterations }
    }

    fn erode(frame: &GrayImage) -> GrayImage {
        let width = frame.width();
        let height = frame.height();
        let mut result = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let mut min_val = 255u8;

                // 3x3 kernel
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let px = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                        let py = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                        min_val = min_val.min(frame.get_pixel(px, py)[0]);
                    }
                }

                result.put_pixel(x, y, image::Luma([min_val]));
            }
        }

        result
    }

    fn dilate(frame: &GrayImage) -> GrayImage {
        let width = frame.width();
        let height = frame.height();
        let mut result = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let mut max_val = 0u8;

                // 3x3 kernel
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let px = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                        let py = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                        max_val = max_val.max(frame.get_pixel(px, py)[0]);
                    }
                }

                result.put_pixel(x, y, image::Luma([max_val]));
            }
        }

        result
    }
}

impl NoiseFilter for ErosionDilation {
    fn apply(&self, frame: &mut GrayImage) {
        for _ in 0..self.iterations {
            let eroded = Self::erode(frame);
            let dilated = Self::dilate(&eroded);
            *frame = dilated;
        }
    }
}

/// Pipeline of noise filters applied in sequence
pub struct FilterPipeline {
    filters: Vec<Box<dyn NoiseFilter>>,
}

impl FilterPipeline {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn add(&mut self, filter: Box<dyn NoiseFilter>) {
        self.filters.push(filter);
    }

    pub fn apply_all(&self, frame: &mut GrayImage) {
        for filter in &self.filters {
            filter.apply(frame);
        }
    }
}

impl Default for FilterPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a default filter pipeline suitable for security camera use
pub fn default_pipeline() -> FilterPipeline {
    let mut pipeline = FilterPipeline::new();

    // Light Gaussian blur to reduce noise
    pipeline.add(Box::new(GaussianBlur::new(3, 1.0)));

    // Erosion-dilation to remove small artifacts
    pipeline.add(Box::new(ErosionDilation::new(1)));

    pipeline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaussian_blur_kernel_generation() {
        let blur = GaussianBlur::new(3, 1.0);
        let kernel = blur.generate_kernel();

        assert_eq!(kernel.len(), 3);

        // Kernel should be normalized (sum to 1.0)
        let sum: f64 = kernel.iter().sum();
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_gaussian_blur_application() {
        let mut frame = GrayImage::from_raw(
            5,
            5,
            vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        )
        .unwrap();

        let blur = GaussianBlur::new(3, 1.0);
        blur.apply(&mut frame);

        // Center pixel should be less than 255 after blur
        assert!(frame.get_pixel(2, 2)[0] < 255);

        // Neighboring pixels should have non-zero values
        assert!(frame.get_pixel(1, 2)[0] > 0);
        assert!(frame.get_pixel(3, 2)[0] > 0);
    }

    #[test]
    fn test_median_filter_removes_outlier() {
        let mut frame = GrayImage::from_raw(
            5,
            5,
            vec![
                100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 255, 100,
                100, // One outlier
                100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
            ],
        )
        .unwrap();

        let filter = MedianFilter::new(3);
        filter.apply(&mut frame);

        // Outlier should be replaced with median of neighborhood
        assert!(frame.get_pixel(2, 2)[0] < 200);
    }

    #[test]
    fn test_erosion_dilation() {
        let mut frame = GrayImage::from_raw(
            5,
            5,
            vec![
                0, 0, 0, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 0, 0,
                0, 0, 0,
            ],
        )
        .unwrap();

        let filter = ErosionDilation::new(1);
        filter.apply(&mut frame);

        // Edges should be affected by erosion-dilation
        // Center should remain bright
        assert!(frame.get_pixel(2, 2)[0] > 0);
    }

    #[test]
    fn test_filter_pipeline_application() {
        let mut frame = GrayImage::from_raw(
            5,
            5,
            vec![
                0, 0, 0, 0, 0, 0, 100, 100, 100, 0, 0, 100, 255, 100, 0, 0, 100, 100, 100, 0, 0, 0,
                0, 0, 0,
            ],
        )
        .unwrap();

        let mut pipeline = FilterPipeline::new();
        pipeline.add(Box::new(GaussianBlur::new(3, 1.0)));
        pipeline.add(Box::new(MedianFilter::new(3)));

        let original_center = frame.get_pixel(2, 2)[0];
        pipeline.apply_all(&mut frame);

        // After filtering, center should be different
        assert_ne!(frame.get_pixel(2, 2)[0], original_center);
    }

    #[test]
    fn test_default_pipeline() {
        let pipeline = default_pipeline();
        assert_eq!(pipeline.filters.len(), 2);

        let mut frame = GrayImage::from_raw(5, 5, vec![0u8; 25]).unwrap();
        pipeline.apply_all(&mut frame);
        // Should not panic
    }

    #[test]
    fn test_invalid_kernel_size_no_panic() {
        let mut frame = GrayImage::new(10, 10);

        // Even kernel size should be ignored
        let blur = GaussianBlur::new(4, 1.0);
        blur.apply(&mut frame);

        // Size < 3 should be ignored
        let blur = GaussianBlur::new(1, 1.0);
        blur.apply(&mut frame);

        // Should not panic
    }
}
