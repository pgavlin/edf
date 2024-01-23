pub trait FontStyle: Clone {
    fn font_name(&self) -> &str;
    fn em_px(&self) -> u16;

    fn line_height(&self) -> u16;
    fn baseline(&self) -> u16;
}
