use style::{
    properties::{generated::longhands::font_variant_caps::computed_value::T as FontVariantCaps, LonghandId, PropertyDeclarationId, PropertyDeclaration, PropertyDeclarationBlock},
    values::{computed, specified},
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GenericFontFamily {
    None,
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
}

impl GenericFontFamily {
    fn values(&self, list: &computed::font::FontFamilyList) -> Self {
        match list.iter().find(|f| matches!(f, computed::font::SingleFontFamily::Generic(_))) {
            Some(computed::font::SingleFontFamily::Generic(family)) => {
                match family {
                    computed::font::GenericFontFamily::None => GenericFontFamily::None,
                    computed::font::GenericFontFamily::Serif => GenericFontFamily::Serif,
                    computed::font::GenericFontFamily::SansSerif => GenericFontFamily::SansSerif,
                    computed::font::GenericFontFamily::Monospace => GenericFontFamily::Monospace,
                    computed::font::GenericFontFamily::Cursive => GenericFontFamily::Cursive,
                    computed::font::GenericFontFamily::Fantasy => GenericFontFamily::Fantasy,
                    computed::font::GenericFontFamily::SystemUi => GenericFontFamily::SystemUi,
                }
            }
            _ => GenericFontFamily::Serif,
        }
    }

    fn compute(&self, block: &PropertyDeclarationBlock) -> Self {
        let decl_id = PropertyDeclarationId::Longhand(LonghandId::FontFamily);
        if let Some((PropertyDeclaration::FontFamily(family), _)) = block.get(decl_id) {
            match family {
                specified::font::FontFamily::Values(list) => self.values(list),
                _ => todo!(),
            }
        } else {
            *self
        }
    }
}

pub struct LengthContext {
    pub pixels_per_inch: f32,
}

impl LengthContext {
    fn to_pixels(&self, length: &specified::length::AbsoluteLength) -> f32 {
        match length {
            specified::length::AbsoluteLength::Px(n) => *n,
            specified::length::AbsoluteLength::In(n) => n * self.pixels_per_inch,
            specified::length::AbsoluteLength::Cm(n) => n * self.pixels_per_inch / 2.54 ,
            specified::length::AbsoluteLength::Mm(n) => n * self.pixels_per_inch / 25.4,
            specified::length::AbsoluteLength::Q(n) => n * self.pixels_per_inch / 101.6,
            specified::length::AbsoluteLength::Pt(n) => n * self.pixels_per_inch / 72.0,
            specified::length::AbsoluteLength::Pc(n) => n * self.pixels_per_inch / 6.0,
        }
    }

    fn font_relative(&self, em_px: f32, length: &specified::length::FontRelativeLength) -> f32 {
        match length {
            specified::length::FontRelativeLength::Em(n) => n * em_px,
            _ => todo!(),
        }
    }

    fn no_calc_length(&self, em_px: f32, length: &specified::length::NoCalcLength) -> f32 {
        match length {
            specified::length::NoCalcLength::Absolute(length) => self.to_pixels(length),
            specified::length::NoCalcLength::FontRelative(length) => self.font_relative(em_px, length),
            _ => todo!(),
        }
    }

    fn length(&self, em_px: f32, length: &specified::length::LengthPercentage) -> f32  {
        match length {
            specified::length::LengthPercentage::Length(length) => self.no_calc_length(em_px, length),
            specified::length::LengthPercentage::Percentage(percentage) => percentage.clamp_to_non_negative().0 * em_px,
            _ => todo!(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FontSizePx(pub f32);

impl FontSizePx {
    fn keyword(&self, keyword: &specified::font::KeywordInfo) -> Self {
        todo!()
    }

    fn smaller(&self) -> Self {
        todo!()
    }

    fn larger(&self) -> Self {
        todo!()
    }

    fn compute(&self, block: &PropertyDeclarationBlock, context: &LengthContext) -> Self {
        let decl_id = PropertyDeclarationId::Longhand(LonghandId::FontSize);
        if let Some((PropertyDeclaration::FontSize(size), _)) = block.get(decl_id) {
            match size {
                specified::font::FontSize::Length(length) => Self(context.length(self.0, length)),
                specified::font::FontSize::Keyword(keyword) => self.keyword(keyword),
                specified::font::FontSize::Smaller => self.smaller(),
                specified::font::FontSize::Larger => self.larger(),
                _ => todo!(),
            }
        } else {
            *self
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FontAngle {
    Normal,
    Italic,
}

impl FontAngle {
    fn compute(&self, block: &PropertyDeclarationBlock) -> Self {
        let decl_id = PropertyDeclarationId::Longhand(LonghandId::FontStyle);
        if let Some((PropertyDeclaration::FontStyle(style), _)) = block.get(decl_id) {
            match style {
                specified::FontStyle::Specified(spec) => match spec {
                    specified::font::SpecifiedFontStyle::Normal => FontAngle::Normal,
                    _ => FontAngle::Italic,
                },
                _ => FontAngle::Normal,
            }
        } else {
            *self
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FontVariant {
    Normal,
    SmallCaps,
}

impl FontVariant {
    fn compute(&self, block: &PropertyDeclarationBlock) -> Self {
        let decl_id = PropertyDeclarationId::Longhand(LonghandId::FontVariantCaps);
        if let Some((PropertyDeclaration::FontVariantCaps(variant), _)) = block.get(decl_id) {
            match variant {
                FontVariantCaps::Normal => FontVariant::Normal,
                FontVariantCaps::SmallCaps => FontVariant::SmallCaps,
            }
        } else {
            *self
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FontWeight(pub f32);

impl FontWeight {
    fn new(v: f32) -> Self {
        Self(v.min(1000.0).max(1.0))
    }

    fn lighter(&self) -> Self {
        if self.0 < 600.0 {
            Self(100.0)
        } else if self.0 < 800.0 {
            Self(400.0)
        } else {
            Self(700.0)
        }
    }

    fn bolder(&self) -> Self {
        if self.0 < 400.0 {
            Self(400.0)
        } else if self.0 < 600.0 {
            Self(700.0)
        } else {
            Self(900.0)
        }
    }

    fn compute(&self, block: &PropertyDeclarationBlock) -> Self {
        let decl_id = PropertyDeclarationId::Longhand(LonghandId::FontWeight);
        if let Some((PropertyDeclaration::FontWeight(weight), _)) = block.get(decl_id) {
            match weight {
                specified::FontWeight::Absolute(w) => match w {
                    specified::font::AbsoluteFontWeight::Weight(n) => Self::new(n.get()),
                    specified::font::AbsoluteFontWeight::Normal => Self(400.0),
                    specified::font::AbsoluteFontWeight::Bold => Self(700.0),
                },
                specified::FontWeight::Lighter => self.lighter(),
                specified::FontWeight::Bolder => self.bolder(),
                _ => todo!(),
            }
        } else {
            *self
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
}

impl TextAlign {
    fn compute(&self, block: &PropertyDeclarationBlock) -> Self {
        let decl_id = PropertyDeclarationId::Longhand(LonghandId::TextAlign);
        if let Some((PropertyDeclaration::TextAlign(align), _)) = block.get(decl_id) {
            let specified::TextAlign::Keyword(kw) = align;
            match kw {
                specified::TextAlignKeyword::Left => TextAlign::Left,
                specified::TextAlignKeyword::Right => TextAlign::Right,
                specified::TextAlignKeyword::Center => TextAlign::Center,
                specified::TextAlignKeyword::Justify => TextAlign::Justify,
                _ => todo!(),
            }
        } else {
            *self
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextIndentPx(pub f32);

impl TextIndentPx {
    fn compute(&self, block: &PropertyDeclarationBlock, em_px: f32, context: &LengthContext) -> Self {
        let decl_id = PropertyDeclarationId::Longhand(LonghandId::TextIndent);
        if let Some((PropertyDeclaration::TextIndent(length), _)) = block.get(decl_id) {
            Self(context.length(em_px, length))
        } else {
            *self
        }
    }
}


#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ComputedStyle {
    pub font_family: GenericFontFamily,
    pub font_size: FontSizePx,
    pub font_style: FontAngle,
    pub font_variant: FontVariant,
    pub font_weight: FontWeight,
    pub text_align: TextAlign,
    pub text_indent: TextIndentPx,
}

impl ComputedStyle {
    pub fn new(em_px: f32) -> Self {
        Self {
            font_family: GenericFontFamily::Serif,
            font_size: FontSizePx(em_px),
            font_style: FontAngle::Normal,
            font_variant: FontVariant::Normal,
            font_weight: FontWeight(400.0),
            text_align: TextAlign::Justify,
            text_indent: TextIndentPx(0.0),
        }
    }

    pub fn compute(&self, block: &PropertyDeclarationBlock, context: &LengthContext) -> Self {
        let font_size = self.font_size.compute(block, context);
        Self {
            font_family: self.font_family.compute(block),
            font_size,
            font_style: self.font_style.compute(block),
            font_variant: self.font_variant.compute(block),
            font_weight: self.font_weight.compute(block),
            text_align: self.text_align.compute(block),
            text_indent: self.text_indent.compute(block, font_size.0, context),
        }
    }
}
