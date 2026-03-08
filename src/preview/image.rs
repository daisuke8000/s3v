const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

pub fn is_image(name: &str) -> bool {
    super::has_extension(name, IMAGE_EXTENSIONS)
}
