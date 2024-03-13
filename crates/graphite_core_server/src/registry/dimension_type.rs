use graphite_binary::nbt::CompoundRefMut;

#[derive(Copy, Clone, Debug)]
pub enum DimensionEffects {
    // Clouds at 192, normal sky type, normal light map and normal ambient light
    Overworld,
    // No clouds, nether sky type, normal light map, constant ambient light
    TheNether,
    // No clouds, end sky type, forced light map, normal ambient light
    TheEnd
}

impl DimensionEffects {
    pub fn as_str(self) -> &'static str {
        match self {
            DimensionEffects::Overworld => "minecraft:overworld",
            DimensionEffects::TheNether => "minecraft:the_nether",
            DimensionEffects::TheEnd => "minecraft:the_end",
        }
    }
}

pub struct DimensionType {
    pub has_skylight: bool, // When true, client will calculate/predict skylight
    pub natural: bool, // When false, compasses spin randomly
    pub min_y: i32,
    pub height: i32,
    pub effects: DimensionEffects, // See enum definition
    pub ambient_light: f32, 
    pub piglin_safe: bool, // If false, piglins shake
}

impl DimensionType {
    pub fn write(&self, mut compound: CompoundRefMut) {
        compound.insert_byte("has_skylight", self.has_skylight as i8);
        compound.insert_byte("has_ceiling", 0);
        compound.insert_byte("ultrawarm", 0);
        compound.insert_byte("natural", self.natural as i8);
        compound.insert_double("coordinate_scale", 1.0);
        compound.insert_byte("bed_works", 1);
        compound.insert_byte("respawn_anchor_works", 1);
        compound.insert_int("min_y", self.min_y);
        compound.insert_int("height", self.height);
        compound.insert_int("logical_height", self.height);
        compound.insert_string("infiniburn", "#mincraft:infiniburn".into());
        compound.insert_string("effects", self.effects.as_str().to_owned());
        compound.insert_float("ambient_light", self.ambient_light);
        compound.insert_byte("piglin_safe", self.piglin_safe as i8);
        compound.insert_byte("has_raids", 0);
        compound.insert_int("monster_spawn_light_level", 0);
        compound.insert_int("monster_spawn_block_light_limit", 0);
    }
}

impl Default for DimensionType {
    fn default() -> Self {
        Self {
            has_skylight: true,
            natural: true,
            min_y: 0,
            height: 384,
            effects: DimensionEffects::Overworld,
            ambient_light: 0.0,
            piglin_safe: true
        }
    }
}