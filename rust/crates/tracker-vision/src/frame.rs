/// An 8-bit luma image in orientation-corrected source pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct GrayFrame {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl GrayFrame {
    pub fn new(width: usize, height: usize, data: Vec<u8>) -> Self {
        assert_eq!(data.len(), width * height, "luma buffer size");
        Self { width, height, data }
    }
    pub fn filled(width: usize, height: usize, value: u8) -> Self {
        Self::new(width, height, vec![value; width * height])
    }
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> u8 {
        self.data[y * self.width + x]
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: u8) {
        self.data[y * self.width + x] = v;
    }
    /// True when a square patch of `radius` around (x, y) lies fully inside the image.
    pub fn fits(&self, x: isize, y: isize, radius: usize) -> bool {
        let r = radius as isize;
        x - r >= 0 && y - r >= 0 && x + r < self.width as isize && y + r < self.height as isize
    }
}
