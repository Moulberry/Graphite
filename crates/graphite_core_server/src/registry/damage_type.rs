use graphite_binary::nbt::CompoundRefMut;

#[derive(Copy, Clone, Debug)]
pub enum DamageTypeEffect {
    Hurt,
    Thorns,
    Drowning,
    Burning,
    Poking,
    Freezing
}

impl DamageTypeEffect {
    pub fn as_str(self) -> &'static str {
        match self {
            DamageTypeEffect::Hurt => "hurt",
            DamageTypeEffect::Thorns => "thorns",
            DamageTypeEffect::Drowning => "drowning",
            DamageTypeEffect::Burning => "burning",
            DamageTypeEffect::Poking => "poking",
            DamageTypeEffect::Freezing => "freezing",
        }
    }
}

pub struct DamageType(pub DamageTypeEffect);

impl DamageType {
    pub fn write(&self, mut compound: CompoundRefMut) {
        compound.insert_string("message_id", "".into());
        compound.insert_string("scaling", "never".into());
        compound.insert_float("exhaustion", 0.0);
        compound.insert_string("effects", self.0.as_str().into());
    }
}