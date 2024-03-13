use graphite_binary::nbt::NBT;

pub struct TextComponent<'a> {
    pub text: &'a str,
    pub font: Option<&'a str>,
    pub color: Option<&'a str>,
}

impl <'a> TextComponent<'a> {
    pub fn to_nbt(&self) -> NBT {
        let mut nbt = NBT::new();
        nbt.insert_string("type", "text".to_owned());
        nbt.insert_string("text", self.text.to_owned());
        if let Some(font) = self.font {
            nbt.insert_string("font", font.to_owned());
        }
        if let Some(color) = self.color {
            nbt.insert_string("color", color.to_owned());
        }
        nbt
    }
}