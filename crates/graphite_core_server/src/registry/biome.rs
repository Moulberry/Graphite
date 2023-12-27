use graphite_binary::nbt::CompoundRefMut;

pub struct Biome {
    pub has_precipitation: bool,
    pub temperature: f32,
    pub temperature_modifier: TemperatureModifier,
    pub downfall: f32,
    pub effects: BiomeEffects
}

impl Biome {
    pub fn write(&self, mut compound: CompoundRefMut) {
        compound.insert_byte("has_precipitation", self.has_precipitation as i8);
        compound.insert_float("temperature", self.temperature);
        compound.insert_string("temperature_modifier", self.temperature_modifier.as_str().to_owned());
        compound.insert_float("downfall", self.downfall);
        self.effects.write(compound.create_compound("effects"));
    }
}

impl Default for Biome {
    fn default() -> Self {
        Self {
            has_precipitation: true,
            temperature: 0.8,
            temperature_modifier: TemperatureModifier::None,
            downfall: 0.4,
            effects: BiomeEffects::default()
        }
    }
}

pub struct BiomeEffects {
    pub fog_color: u32,
    pub water_color: u32,
    pub water_fog_color: u32,
    pub sky_color: u32,
    pub foliage_color: Option<u32>,
    pub grass_color: Option<u32>,
    pub grass_color_modifier: GrassColorModifier,
    pub particle: Option<ParticleEffects>,
    pub ambient_sound: Option<AmbientSound>,
    pub mood_sound: Option<MoodSound>,
    pub additions_sound: Option<AdditionsSound>,
    pub music: Option<Music>,
}

impl Default for BiomeEffects {
    fn default() -> Self {
        Self {
            fog_color: 0xc0d8ff,
            water_color: 0x3f76e4,
            water_fog_color: 0x050533,
            sky_color: 0x78a7ff,
            foliage_color: None,
            grass_color: None,
            grass_color_modifier: GrassColorModifier::None,
            particle: None,
            ambient_sound: None,
            mood_sound: None,
            additions_sound: None,
            music: None
        }
    }
}

impl BiomeEffects {
    pub fn write(&self, mut compound: CompoundRefMut) {
        compound.insert_int("fog_color", self.fog_color as i32);
        compound.insert_int("water_color", self.water_color as i32);
        compound.insert_int("water_fog_color", self.water_fog_color as i32);
        compound.insert_int("sky_color", self.sky_color as i32);
        if let Some(foliage_color) = self.foliage_color {
            compound.insert_int("foliage_color", foliage_color as i32);
        }
        if let Some(grass_color) = self.grass_color {
            compound.insert_int("grass_color", grass_color as i32);
        }
        if self.grass_color_modifier != GrassColorModifier::None {
            compound.insert_string("grass_color_modifier", self.grass_color_modifier.as_str().to_owned());
        }
        if let Some(particle) = &self.particle {
            particle.write(compound.create_compound("particle"));
        }
        if let Some(ambient_sound) = &self.ambient_sound {
            match ambient_sound {
                AmbientSound::Soundtrack { sound_id } => {
                    compound.insert_string("ambient_sound", sound_id.clone());
                },
                AmbientSound::SoundtrackAndRange { sound_id, range } => {
                    let mut ambient_sound = compound.create_compound("ambient_sound");
                    ambient_sound.insert_string("sound_id", sound_id.clone());
                    ambient_sound.insert_float("range", *range);
                },
            }
        }
        if let Some(mood_sound) = &self.mood_sound {
            mood_sound.write(compound.create_compound("mood_sound"));
        }
        if let Some(additions_sound) = &self.additions_sound {
            additions_sound.write(compound.create_compound("additions_sound"));
        }
        if let Some(music) = &self.music {
            music.write(compound.create_compound("music"));
        }
    }
}

pub struct ParticleEffects {
    particle_type: String,
    probability: f32
}


impl ParticleEffects {
    pub fn write(&self, mut compound: CompoundRefMut) {
        let mut options = compound.create_compound("options");
        options.insert_string("type", self.particle_type.clone());

        compound.insert_float("probability", self.probability);
    }
}

pub enum AmbientSound {
    Soundtrack { sound_id: String },
    SoundtrackAndRange { sound_id: String, range: f32 }
}

pub struct MoodSound {
    sound: String,
    tick_delay: i32,
    block_search_extent: i32,
    offset: f64
}


impl MoodSound {
    pub fn write(&self, mut compound: CompoundRefMut) {
        compound.insert_string("sound", self.sound.clone());
        compound.insert_int("tick_delay", self.tick_delay);
        compound.insert_int("block_search_extent", self.block_search_extent);
        compound.insert_double("offset", self.offset);
    }
}

pub struct AdditionsSound {
    sound: String,
    tick_chance: f64
}

impl AdditionsSound {
    pub fn write(&self, mut compound: CompoundRefMut) {
        compound.insert_string("sound", self.sound.clone());
        compound.insert_double("tick_chance", self.tick_chance);
    }
}

pub struct Music {
    sound: String,
    min_delay: i32,
    max_delay: i32,
    replace_current_music: bool
}

impl Music {
    pub fn write(&self, mut compound: CompoundRefMut) {
        compound.insert_string("sound", self.sound.clone());
        compound.insert_int("min_delay", self.min_delay);
        compound.insert_int("max_delay", self.max_delay);
        compound.insert_byte("replace_current_music", self.replace_current_music as i8);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TemperatureModifier {
    None,
    Frozen // pockets of warm temperature (0.2) to be randomly distributed throughout the biome
}

impl TemperatureModifier {
    pub fn as_str(self) -> &'static str {
        match self {
            TemperatureModifier::None => "none",
            TemperatureModifier::Frozen => "frozen",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GrassColorModifier {
    None,
    DarkForest, // a darker, and less saturated shade of the color
    Swamp // override with two fixed colors (#4C763C and #6A7039), randomly distributed throughout the biome
}

impl GrassColorModifier {
    pub fn as_str(self) -> &'static str {
        match self {
            GrassColorModifier::None => "none",
            GrassColorModifier::DarkForest => "dark_forest",
            GrassColorModifier::Swamp => "swamp",
        }
    }
}