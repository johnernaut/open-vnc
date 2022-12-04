pub struct ScreenShot {
    pub width: u16,
    pub height: u16,
    pub data: Vec<u8>,
}

pub struct ServerInit {
    pub resolution: Resolution,
    pub pixel_format: PixelFormat,
    pub name: String,
}

impl ServerInit {
    pub fn new(width: u16, height: u16, name: String) -> Self {
        Self {
            resolution: Resolution { width, height },
            pixel_format: PixelFormat {
                bits_per_pixel: 16,
                depth: 16,
                big_endian: false,
                true_color_flag: 1,
                red_max: 0x1f,
                green_max: 0x1f,
                blue_max: 0x1f,
                red_shift: 0xa,
                green_shift: 0x5,
                blue_shift: 0,
            },
            name,
        }
    }
}

pub struct Resolution {
    pub width: u16,
    pub height: u16,
}

pub struct PixelFormat {
    pub bits_per_pixel: u8,
    pub depth: u8,
    pub big_endian: bool,
    pub true_color_flag: u8,
    pub red_max: u16,
    pub green_max: u16,
    pub blue_max: u16,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
