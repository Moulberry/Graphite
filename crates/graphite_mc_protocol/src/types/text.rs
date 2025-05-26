use std::{borrow::Cow, hash::Hasher};

use anyhow::{bail, Context};
use crc32c::Crc32cHasher;
use graphite_binary::nbt::{self, CompoundRef, CompoundRefMut, EncodedNBT, NBTRef, NBT, TAG_COMPOUND_ID};
use once_cell::sync::Lazy;

use super::hash_ops::{HashOps, HashOpsMap};

#[derive(Default, Clone, Debug, PartialEq)]
pub struct TextComponent<'a> {
    pub inner: TextComponentContent<'a>,
    pub color: Option<TextColor>,
    pub shadow_color: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub strikethrough: Option<bool>,
    pub obfuscated: Option<bool>,
    pub font: Option<Cow<'a, str>>,
    pub insertion: Option<Cow<'a, str>>,
    pub extra: Vec<Cow<'a, TextComponent<'a>>>,
}

#[derive(Clone, Copy, Debug)]
pub enum TextColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
    Custom(u32)
}

#[derive(PartialEq, Clone, Debug)]
pub enum TextComponentContent<'a> {
    Literal {
        text: Cow<'a, str>
    },
    Translatable {
        translate: Cow<'a, str>,
        with: Vec<Cow<'a, TextComponent<'a>>>
    },
    Keybind {
        keybind: Cow<'a, str>
    }
}

impl <'a> Default for TextComponentContent<'a> {
    fn default() -> Self {
        Self::Literal { text: Cow::Borrowed("") }
    }
}

impl TextColor {
    pub fn to_rgb(self) -> u32 {
        match self {
            TextColor::Black => 0x000000,
            TextColor::DarkBlue => 0x0000AA,
            TextColor::DarkGreen => 0x00AA00,
            TextColor::DarkAqua => 0x00AAAA,
            TextColor::DarkRed => 0xAA0000,
            TextColor::DarkPurple => 0xAA00AA,
            TextColor::Gold => 0xFFAA00,
            TextColor::Gray => 0xAAAAAA,
            TextColor::DarkGray => 0x555555,
            TextColor::Blue => 0x5555FF,
            TextColor::Green => 0x55FF55,
            TextColor::Aqua => 0x55FFFF,
            TextColor::Red => 0xFF5555,
            TextColor::LightPurple => 0xFF55FF,
            TextColor::Yellow => 0xFFFF55,
            TextColor::White => 0xFFFFFF,
            TextColor::Custom(rgb) => rgb & 0xFFFFFF,
        }
    }

    pub fn to_str(self) -> Cow<'static, str> {
        match self {
            TextColor::Black => Cow::Borrowed("black"),
            TextColor::DarkBlue => Cow::Borrowed("dark_blue"),
            TextColor::DarkGreen => Cow::Borrowed("dark_green"),
            TextColor::DarkAqua => Cow::Borrowed("dark_aqua"),
            TextColor::DarkRed => Cow::Borrowed("dark_red"),
            TextColor::DarkPurple => Cow::Borrowed("dark_purple"),
            TextColor::Gold => Cow::Borrowed("gold"),
            TextColor::Gray => Cow::Borrowed("gray"),
            TextColor::DarkGray => Cow::Borrowed("dark_gray"),
            TextColor::Blue => Cow::Borrowed("blue"),
            TextColor::Green => Cow::Borrowed("green"),
            TextColor::Aqua => Cow::Borrowed("aqua"),
            TextColor::Red => Cow::Borrowed("red"),
            TextColor::LightPurple => Cow::Borrowed("light_purple"),
            TextColor::Yellow => Cow::Borrowed("yellow"),
            TextColor::White => Cow::Borrowed("white"),
            TextColor::Custom(rgb) => Cow::Owned(format!("#{:0>6X}", rgb & 0xFFFFFF)),
        }
    }
}

impl PartialEq for TextColor {
    fn eq(&self, other: &Self) -> bool {
        self.to_rgb() == other.to_rgb()
    }
}

pub static ENCODED_EMPTY: Lazy<EncodedNBT> = Lazy::new(|| {
    TextComponent::default().to_encoded_nbt()
});

impl <'a> TextComponent<'a> {
    pub fn empty_encoded() -> EncodedNBT {
        ENCODED_EMPTY.clone()
    }

    pub fn literal(text: &str) -> TextComponent<'_> {
        TextComponent {
            inner: TextComponentContent::Literal {
                text: Cow::Borrowed(text)
            },
            ..Default::default()
        }
    }

    pub fn literal_owned(text: String) -> TextComponent<'static> {
        TextComponent {
            inner: TextComponentContent::Literal {
                text: Cow::Owned(text)
            },
            ..Default::default()
        }
    }

    pub fn translatable(translate: &'a str, with: Vec<Cow<'a, TextComponent<'a>>>) -> TextComponent<'a> {
        TextComponent {
            inner: TextComponentContent::Translatable {
                translate: translate.into(),
                with
            },
            ..Default::default()
        }
    }

    pub fn translatable0(translate: &'a str) -> TextComponent<'a> {
        TextComponent {
            inner: TextComponentContent::Translatable {
                translate: translate.into(),
                with: vec![]
            },
            ..Default::default()
        }
    }

    pub fn translatable0_owned(translate: String) -> TextComponent<'a> {
        TextComponent {
            inner: TextComponentContent::Translatable {
                translate: translate.into(),
                with: vec![]
            },
            ..Default::default()
        }
    }

    pub fn translatable1(translate: &'a str, with1: impl IntoTextComponent<'a>) -> TextComponent<'a> {
        TextComponent {
            inner: TextComponentContent::Translatable {
                translate: translate.into(),
                with: vec![Cow::Owned(with1.into_text_component())]
            },
            ..Default::default()
        }
    }

    pub fn translatable2(translate: &'a str, with1: impl IntoTextComponent<'a>, with2: impl IntoTextComponent<'a>) -> TextComponent<'a> {
        TextComponent {
            inner: TextComponentContent::Translatable {
                translate: translate.into(),
                with: vec![Cow::Owned(with1.into_text_component()), Cow::Owned(with2.into_text_component())]
            },
            ..Default::default()
        }
    }

    pub fn translatable3(translate: &'a str, with1: impl IntoTextComponent<'a>, with2: impl IntoTextComponent<'a>,
            with3: impl IntoTextComponent<'a>) -> TextComponent<'a> {
        TextComponent {
            inner: TextComponentContent::Translatable {
                translate: translate.into(),
                with: vec![Cow::Owned(with1.into_text_component()), Cow::Owned(with2.into_text_component()), Cow::Owned(with3.into_text_component())]
            },
            ..Default::default()
        }
    }

    pub fn keybind(keybind: &'a str) -> TextComponent<'a> {
        TextComponent {
            inner: TextComponentContent::Keybind {
                keybind: Cow::Borrowed(keybind)
            },
            ..Default::default()
        }
    }

    #[inline(always)]
    pub fn unstyle_lore(mut self) -> Self {
        self.color = Some(TextColor::White);
        self.italic = Some(false);
        self
    }

    #[inline(always)]
    pub fn bold(mut self) -> Self {
        self.bold = Some(true);
        self
    }

    #[inline(always)]
    pub fn font(mut self, font: &'a str) -> Self {
        self.font = Some(Cow::Borrowed(font));
        self
    }

    #[inline(always)]
    pub fn color(mut self, rgb: u32) -> Self {
        self.color = Some(TextColor::Custom(rgb));
        self
    }

    #[inline(always)]
    pub fn color3(mut self, red: u32, green: u32, blue: u32) -> Self {
        let red = red.clamp(0x00, 0xFF);
        let green = green.clamp(0x00, 0xFF);
        let blue = blue.clamp(0x00, 0xFF);
        self.color = Some(TextColor::Custom(red << 16 | green << 8 | blue));
        self
    }

    #[inline(always)]
    pub fn text_color(mut self, text_color: TextColor) -> Self {
        self.color = Some(text_color);
        self
    }


    #[inline(always)]
    pub fn shadow_color(mut self, argb: u32) -> Self {
        self.shadow_color = Some(argb);
        self
    }

    #[inline(always)]
    pub fn black(mut self) -> Self {
        self.color = Some(TextColor::Black);
        self
    }

    #[inline(always)]
    pub fn dark_blue(mut self) -> Self {
        self.color = Some(TextColor::DarkBlue);
        self
    }

    #[inline(always)]
    pub fn dark_green(mut self) -> Self {
        self.color = Some(TextColor::DarkGreen);
        self
    }

    #[inline(always)]
    pub fn dark_aqua(mut self) -> Self {
        self.color = Some(TextColor::DarkAqua);
        self
    }

    #[inline(always)]
    pub fn dark_red(mut self) -> Self {
        self.color = Some(TextColor::DarkRed);
        self
    }

    #[inline(always)]
    pub fn dark_purple(mut self) -> Self {
        self.color = Some(TextColor::DarkPurple);
        self
    }

    #[inline(always)]
    pub fn gold(mut self) -> Self {
        self.color = Some(TextColor::Gold);
        self
    }

    #[inline(always)]
    pub fn gray(mut self) -> Self {
        self.color = Some(TextColor::Gray);
        self
    }

    #[inline(always)]
    pub fn dark_gray(mut self) -> Self {
        self.color = Some(TextColor::DarkGray);
        self
    }

    #[inline(always)]
    pub fn blue(mut self) -> Self {
        self.color = Some(TextColor::Blue);
        self
    }

    #[inline(always)]
    pub fn green(mut self) -> Self {
        self.color = Some(TextColor::Green);
        self
    }

    #[inline(always)]
    pub fn aqua(mut self) -> Self {
        self.color = Some(TextColor::Aqua);
        self
    }

    #[inline(always)]
    pub fn red(mut self) -> Self {
        self.color = Some(TextColor::Red);
        self
    }

    #[inline(always)]
    pub fn light_purple(mut self) -> Self {
        self.color = Some(TextColor::LightPurple);
        self
    }

    #[inline(always)]
    pub fn yellow(mut self) -> Self {
        self.color = Some(TextColor::Yellow);
        self
    }

    #[inline(always)]
    pub fn white(mut self) -> Self {
        self.color = Some(TextColor::White);
        self
    }

    #[inline(always)]
    pub fn italic(mut self) -> Self {
        self.italic = Some(true);
        self
    }

    #[inline(always)]
    pub fn append(mut self, other: impl IntoTextComponent<'a>) -> Self {
        self.extra.push(Cow::Owned(other.into_text_component()));
        self
    }

    pub fn write_encoded_nbt(&self, bytes: &mut Vec<u8>, name: Option<&str>, write_start: bool) {
        if write_start {
            nbt::encode_raw::start_compound_tag(bytes, name);
        }

        self.inner.write_encoded_nbt(bytes);

        if let Some(color) = self.color.as_ref() {
            nbt::encode_raw::push_str(bytes, Some("color"), &color.to_str());
        }
        if let Some(shadow_color) = self.shadow_color.as_ref() {
            nbt::encode_raw::push_int(bytes, Some("shadow_color"), *shadow_color as i32);
        }
        if let Some(bold) = self.bold {
            nbt::encode_raw::push_byte(bytes, Some("bold"), bold as i8);
        }
        if let Some(italic) = self.italic {
            nbt::encode_raw::push_byte(bytes, Some("italic"), italic as i8);
        }
        if let Some(underlined) = self.underlined {
            nbt::encode_raw::push_byte(bytes, Some("underlined"), underlined as i8);
        }
        if let Some(strikethrough) = self.strikethrough {
            nbt::encode_raw::push_byte(bytes, Some("strikethrough"), strikethrough as i8);
        }
        if let Some(obfuscated) = self.obfuscated {
            nbt::encode_raw::push_byte(bytes, Some("obfuscated"), obfuscated as i8);
        }
        if let Some(font) = self.font.as_ref() {
            nbt::encode_raw::push_str(bytes, Some("font"), font);
        }
        if let Some(insertion) = self.insertion.as_ref() {
            nbt::encode_raw::push_str(bytes, Some("insertion"), insertion);
        }

        if !self.extra.is_empty() {
            nbt::encode_raw::start_list(bytes, Some("extra"), TAG_COMPOUND_ID, self.extra.len() as i32);
            for extra in &self.extra {
                extra.write_encoded_nbt(bytes, None, false);
            }
        }

        nbt::encode_raw::end_compound_tag(bytes);
    }

    pub fn to_encoded_nbt(&self) -> EncodedNBT {
        self.to_encoded_nbt_with_size_hint(0)
    }

    pub fn to_encoded_nbt_with_size_hint(&self, size: usize) -> EncodedNBT {
        let mut bytes = Vec::with_capacity(size);
        self.write_encoded_nbt(&mut bytes, None, true);
        EncodedNBT::new_from_raw_bytes(bytes)
    }

    pub fn from_nbt(nbt: NBTRef<'_>) -> anyhow::Result<TextComponent<'static>> {
        match nbt {
            nbt::NBTRef::String(string) => {
                Ok(Self::literal_owned(string.clone()))
            },
            nbt::NBTRef::List(list_ref) => {
                let mut extra = Vec::with_capacity(list_ref.len());
                for child in list_ref.iter() {
                    extra.push(Cow::Owned(Self::from_nbt(child)?));
                }
                Ok(TextComponent {
                    inner: TextComponentContent::Literal { text: Cow::Borrowed("") },
                    extra,
                    ..Default::default()
                })
            },
            nbt::NBTRef::Compound(compound) => {
                let inner = TextComponentContent::from_nbt(compound)?;

                let color = if let Some(color) = compound.find_string("color") {
                    if color.starts_with('#') {
                        let res = u32::from_str_radix(&color[1..], 16)?;
                        Some(TextColor::Custom(res))
                    } else {
                        match color.as_str() {
                            "black" => Some(TextColor::Black),
                            "dark_blue" => Some(TextColor::DarkBlue),
                            "dark_green" => Some(TextColor::DarkGreen),
                            "dark_aqua" => Some(TextColor::DarkAqua),
                            "dark_red" => Some(TextColor::DarkRed),
                            "dark_purple" => Some(TextColor::DarkPurple),
                            "gold" => Some(TextColor::Gold),
                            "gray" => Some(TextColor::Gray),
                            "dark_gray" => Some(TextColor::DarkGray),
                            "blue" => Some(TextColor::Blue),
                            "green" => Some(TextColor::Green),
                            "aqua" => Some(TextColor::Aqua),
                            "red" => Some(TextColor::Red),
                            "light_purple" => Some(TextColor::LightPurple),
                            "yellow" => Some(TextColor::Yellow),
                            "white" => Some(TextColor::White),
                            _ => bail!("Invalid color value: {}", color)
                        }
                    }
                } else {
                    None
                };
                let shadow_color = if let Some(argb) = compound.find_int("shadow_color") {
                    Some(*argb as u32)
                } else if let Some(list) = compound.find_list_of_any("shadow_color") {
                    if list.len() != 4 {
                        bail!("shadow_color list must be size 4");
                    }
                    let a: f32 = list.get_numeric(0).context("shadow_color must be numeric")?;
                    let r: f32 = list.get_numeric(1).context("shadow_color must be numeric")?;
                    let g: f32 = list.get_numeric(2).context("shadow_color must be numeric")?;
                    let b: f32 = list.get_numeric(3).context("shadow_color must be numeric")?;
                    let a = (a * 255.0) as u32;
                    let r = (r * 255.0) as u32;
                    let g = (g * 255.0) as u32;
                    let b = (b * 255.0) as u32;
                    Some(a << 24 | r << 16 | g << 7 | b)
                } else {
                    None
                };
                let bold = compound.find_byte("bold").map(|v| *v != 0);
                let italic = compound.find_byte("italic").map(|v| *v != 0);
                let underlined = compound.find_byte("underlined").map(|v| *v != 0);
                let strikethrough = compound.find_byte("strikethrough").map(|v| *v != 0);
                let obfuscated = compound.find_byte("obfuscated").map(|v| *v != 0);
                let font = compound.find_string("font").map(|v| Cow::Owned(v.clone()));
                let insertion = compound.find_string("insertion").map(|v| Cow::Owned(v.clone()));

                let mut extra = Vec::new();
                if let Some(extra_list) = compound.find_list_of_any("extra") {
                    extra.reserve_exact(extra_list.len());
                    for compound in extra_list.iter() {
                        extra.push(Cow::Owned(Self::from_nbt(compound)?));
                    }
                }

                Ok(TextComponent {
                    inner,
                    color,
                    shadow_color,
                    bold,
                    italic,
                    underlined,
                    strikethrough,
                    obfuscated,
                    font,
                    insertion,
                    extra,
                })
            },
            _ => bail!("Unexpected tag type: {:?}", nbt.tag_type())
        }
    }

    pub fn checksum(&self) -> i32 {
        match self.checksum_inner() {
            ChecksumOutput::String(string) => HashOps::hash_string(string),
            ChecksumOutput::Map(hash_ops_map) => hash_ops_map.finish(),
        }
    }

    fn checksum_inner(&self) -> ChecksumOutput<'_> {
        let mut map = HashOps::start_map();

        if let Some(color) = self.color.as_ref() {
            map.put_string("color", &color.to_str());
        }
        if let Some(shadow_color) = self.shadow_color {
            map.put_int("shadow_color", shadow_color as i32);
        }
        if let Some(bold) = self.bold {
            map.put_boolean("bold", bold);
        }
        if let Some(italic) = self.italic {
            map.put_boolean("italic", italic);
        }
        if let Some(underlined) = self.underlined {
            map.put_boolean("underlined", underlined);
        }
        if let Some(strikethrough) = self.strikethrough {
            map.put_boolean("strikethrough", strikethrough);
        }
        if let Some(obfuscated) = self.obfuscated {
            map.put_boolean("obfuscated", obfuscated);
        }
        if let Some(font) = self.font.as_ref() {
            map.put_string("font", &font);
        }
        if let Some(insertion) = self.insertion.as_ref() {
            map.put_string("insertion", &insertion);
        }

        if !self.extra.is_empty() {
            let mut list = HashOps::start_list();
            for extra in &self.extra {
                match extra.checksum_inner() {
                    ChecksumOutput::String(string) => list.add_string(string),
                    ChecksumOutput::Map(hash_ops_map) => list.add_map(hash_ops_map),
                }
            }
            map.put_list("extra", list);
        }

        match &self.inner {
            TextComponentContent::Literal { text } => {
                if map.len() == 0 {
                    return ChecksumOutput::String(&text)
                }
                map.put_string("text", &text);
            },
            TextComponentContent::Translatable { translate, with } => {
                map.put_string("translate", &translate);

                if !with.is_empty() {
                    let mut list = HashOps::start_list();
                    for with in with {
                        match with.checksum_inner() {
                            ChecksumOutput::String(string) => list.add_string(string),
                            ChecksumOutput::Map(hash_ops_map) => list.add_map(hash_ops_map),
                        }
                    }
                    map.put_list("with", list);
                }
            },
            TextComponentContent::Keybind { keybind } => {
                map.put_string("keybind", &keybind);
            },
        }

        ChecksumOutput::Map(map)
    }

    pub fn type_str(&self) -> &'static str {
        match &self.inner {
            TextComponentContent::Literal { text: _ } => "text",
            TextComponentContent::Translatable { translate: _, with: _ } => "translatable",
            TextComponentContent::Keybind { keybind: _ } => "keybind",
        }
    }
}

enum ChecksumOutput<'a> {
    String(&'a str),
    Map(HashOpsMap)
}

impl <'a> TextComponentContent<'a> {
    pub fn from_nbt(compound: CompoundRef<'_>) -> anyhow::Result<TextComponentContent<'static>> {
        if compound.contains_key("") && compound.len() == 1 {
            if let Some(text) = compound.find_string("") {
                return Ok(TextComponentContent::Literal {
                    text: Cow::Owned(text.clone())
                });
            }
        }

        let type_hint = match compound.find_string("type") {
            Some(hint) => hint.as_str(),
            None => {
                // Try to infer type from fields
                if compound.contains_key("text") {
                    "text"
                } else if compound.contains_key("translate") {
                    "translatable"
                } else if compound.contains_key("keybind") {
                    "keybind"
                } else {
                    bail!("Unable to determine type");
                }
            },
        };

        match type_hint {
            "text" => {
                let Some(text) = compound.find_string("text") else {
                    bail!("Missing 'text' field");
                };

                Ok(TextComponentContent::Literal {
                    text: Cow::Owned(text.clone())
                })
            },
            "translatable" => {
                let Some(translate) = compound.find_string("translate") else {
                    bail!("Missing 'translate' field");
                };

                let mut with = Vec::new();
                if let Some(with_list) = compound.find_list("with", TAG_COMPOUND_ID) {
                    with.reserve_exact(with_list.len());
                    for child in with_list.iter() {
                        with.push(Cow::Owned(TextComponent::from_nbt(child)?));
                    }
                }

                Ok(TextComponentContent::Translatable {
                    translate: Cow::Owned(translate.clone()),
                    with
                })
            },
            "keybind" => {
                let Some(keybind) = compound.find_string("keybind") else {
                    bail!("Missing 'keybind' field");
                };

                Ok(TextComponentContent::Keybind {
                    keybind: Cow::Owned(keybind.clone())
                })
            },
            _ => {
                bail!("Unknown component type '{}'", type_hint);
            }
        }
    }

    pub fn write_encoded_nbt(&self, bytes: &mut Vec<u8>) {
        match self {
            TextComponentContent::Literal { text } => {
                nbt::encode_raw::push_str(bytes, Some("text"), text);
            },
            TextComponentContent::Translatable { translate, with } => {
                nbt::encode_raw::push_str(bytes, Some("translate"), translate);

                if !with.is_empty() {
                    nbt::encode_raw::start_list(bytes, Some("with"), TAG_COMPOUND_ID, with.len() as i32);
                    for with in with {
                        with.write_encoded_nbt(bytes, None, false);
                    }
                }
            },
            TextComponentContent::Keybind { keybind } => {
                nbt::encode_raw::push_str(bytes, Some("keybind"), keybind);
            },
        }
    }
}

pub trait IntoTextComponent<'a> {
    fn into_text_component(self) -> TextComponent<'a>;
}

impl <'a> IntoTextComponent<'a> for TextComponent<'a> {
    fn into_text_component(self) -> TextComponent<'a> {
        self
    }
}

impl <'a> IntoTextComponent<'a> for &'a str {
    fn into_text_component(self) -> TextComponent<'a> {
        TextComponent::literal(self)
    }
}

impl <'a> IntoTextComponent<'a> for String {
    fn into_text_component(self) -> TextComponent<'a> {
        TextComponent {
            inner: TextComponentContent::Literal {
                text: Cow::Owned(self)
            },
            ..Default::default()
        }
    }
}