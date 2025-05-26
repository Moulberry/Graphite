use graphite_binary::nbt::{NBT, TAG_STRING_ID};
use graphite_mc_protocol::configuration;

use crate::registry::{dimension_type::DimensionType, damage_type::{DamageType, DamageTypeEffect}, biome::Biome};

pub struct Registries {
    pub chat_type: ChatTypeRegistry,
    pub dimension_type: DimensionTypeRegistry,
    pub damage_type: DamageTypeRegistry,
    pub biomes: BiomeRegistry,
    pub painting_variant: PaintingVariantRegistry,
    pub wolf_variant: WolfVariantRegistry,
    pub wolf_sound_variant: WolfSoundVariantRegistry,
    pub cat_variant: AnimalVariantRegistry,
    pub chicken_variant: AnimalVariantRegistry,
    pub cow_variant: AnimalVariantRegistry,
    pub frog_variant: AnimalVariantRegistry,
    pub pig_variant: AnimalVariantRegistry,
}

impl Default for Registries {
    fn default() -> Self {
        Self {
            chat_type: Default::default(),
            dimension_type: Default::default(),
            damage_type: Default::default(),
            biomes: Default::default(),
            painting_variant: Default::default(),
            wolf_variant: Default::default(),
            wolf_sound_variant: Default::default(),
            cat_variant: AnimalVariantRegistry::new_single_valued("cat_variant", "tabby", "entity/cat/tabby"),
            chicken_variant: AnimalVariantRegistry::new_single_valued("chicken_variant", "temperate", "entity/chicken/temperate_chicken"),
            cow_variant: AnimalVariantRegistry::new_single_valued("cow_variant", "temperate", "entity/cow/temperate_cow"),
            frog_variant: AnimalVariantRegistry::new_single_valued("frog_variant", "temperate", "entity/frog/temperate_frog"),
            pig_variant: AnimalVariantRegistry::new_single_valued("pig_variant", "temperate", "entity/pig/temperate_pig"),
        }
    }
}

pub trait Registry {
    fn registry_name(&self) -> &str;

    fn create_packet(&self) -> configuration::clientbound::RegistryData<'_> {
        configuration::clientbound::RegistryData {
            registry: self.registry_name(),
            entries: self.create_entries(),
        }
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry>;
}

// Chat type

#[derive(Default)]
pub struct ChatTypeRegistry {}

impl Registry for ChatTypeRegistry {
    fn registry_name(&self) -> &'static str {
        "minecraft:chat_type"
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry> {
        Vec::new()
    }
}

// Dimension type

pub struct DimensionTypeRegistry {
    pub dimensions: Vec<(String, DimensionType)>
}

impl Registry for DimensionTypeRegistry {
    fn registry_name(&self) -> &'static str {
        "minecraft:dimension_type"
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry> {
        let mut vec = Vec::new();
        for (name, dimension) in self.dimensions.iter() {
            vec.push(configuration::clientbound::PackedRegistryEntry {
                id: name,
                data: Some(dimension.to_nbt().into()),
            });
        }
        vec
    }
}

impl Default for DimensionTypeRegistry {
    fn default() -> Self {
        Self {
            dimensions: vec![("graphite:default_dimension_type".into(), DimensionType::default())]
        }
    }
}

// Damage type

pub struct DamageTypeRegistry {
    damage_types: Vec<(String, DamageType)>
}

impl Registry for DamageTypeRegistry {
    fn registry_name(&self) -> &'static str {
        "minecraft:damage_type"
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry> {
        let mut vec = Vec::new();
        for (name, damage_type) in self.damage_types.iter() {
            vec.push(configuration::clientbound::PackedRegistryEntry {
                id: name,
                data: Some(damage_type.to_nbt().into()),
            });
        }
        vec
    }
}

impl Default for DamageTypeRegistry {
    fn default() -> Self {
        Self {
            damage_types: vec![
                ("minecraft:generic".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:in_fire".into(), DamageType(DamageTypeEffect::Burning)),
                ("minecraft:campfire".into(), DamageType(DamageTypeEffect::Burning)),
                ("minecraft:lightning_bolt".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:on_fire".into(), DamageType(DamageTypeEffect::Burning)),
                ("minecraft:lava".into(), DamageType(DamageTypeEffect::Burning)),
                ("minecraft:hot_floor".into(), DamageType(DamageTypeEffect::Burning)),
                ("minecraft:in_wall".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:cramming".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:drown".into(), DamageType(DamageTypeEffect::Drowning)),
                ("minecraft:starve".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:cactus".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:fall".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:ender_pearl".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:fly_into_wall".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:out_of_world".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:magic".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:wither".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:dragon_breath".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:dry_out".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:sweet_berry_bush".into(), DamageType(DamageTypeEffect::Poking)),
                ("minecraft:freeze".into(), DamageType(DamageTypeEffect::Freezing)),
                ("minecraft:stalagmite".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:outside_border".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:generic_kill".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:player_attack".into(), DamageType(DamageTypeEffect::Hurt)),
            ]
        }
    }
}

// Biomes

pub struct BiomeRegistry {
    biomes: Vec<(String, Biome)>
}

impl Registry for BiomeRegistry {
    fn registry_name(&self) -> &'static str {
        "minecraft:worldgen/biome"
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry> {
        let mut vec = Vec::new();
        for (name, biome) in self.biomes.iter() {
            vec.push(configuration::clientbound::PackedRegistryEntry {
                id: name,
                data: Some(biome.to_nbt().into()),
            });
        }
        vec
    }
}

impl Default for BiomeRegistry {
    fn default() -> Self {
        Self {
            biomes: vec![("minecraft:plains".into(), Biome::default())]
        }
    }
}

// Painting Variant

pub struct PaintingVariantRegistry {
    paintings: Vec<(String, PaintingVariant)>
}

impl PaintingVariant {
    pub fn to_nbt(&self) -> NBT {
        let mut nbt = NBT::new();
        let mut compound = nbt.as_compound_mut().unwrap();
        compound.insert_byte("width", self.width as i8);
        compound.insert_byte("height", self.height as i8);
        compound.insert_string("asset_id", self.asset_id.to_owned());
        nbt
    }
}

pub struct PaintingVariant {
    width: i32,
    height: i32,
    asset_id: &'static str,
}

impl Registry for PaintingVariantRegistry {
    fn registry_name(&self) -> &'static str {
        "minecraft:painting_variant"
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry> {
        let mut vec = Vec::new();
        for (name, painting_variant) in &self.paintings {
            vec.push(configuration::clientbound::PackedRegistryEntry {
                id: name,
                data: Some(painting_variant.to_nbt().into()),
            });
        }
        vec
    }
}

impl Default for PaintingVariantRegistry {
    fn default() -> Self {
        Self {
            paintings: vec![("empty".to_string(), PaintingVariant {
                width: 1,
                height: 1,
                asset_id: "empty"
            })]
        }
    }
}

// Wolf Variant

pub struct WolfVariantRegistry {
    wolves: Vec<(String, WolfVariant)>
}

impl WolfVariant {
    pub fn to_nbt(&self) -> NBT {
        let mut nbt = NBT::new();
        let mut compound = nbt.as_compound_mut().unwrap();

        let mut assets = compound.create_compound("assets");
        assets.insert_string("wild", self.wild_texture.to_owned());
        assets.insert_string("tame", self.tame_texture.to_owned());
        assets.insert_string("angry", self.angry_texture.to_owned());
        compound.create_list("spawn_conditions", TAG_STRING_ID);

        nbt
    }
}

pub struct WolfVariant {
    wild_texture: &'static str,
    tame_texture: &'static str,
    angry_texture: &'static str,
}

impl Registry for WolfVariantRegistry {
    fn registry_name(&self) -> &'static str {
        "minecraft:wolf_variant"
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry> {
        let mut vec = Vec::new();
        for (name, wolf_variant) in &self.wolves {
            vec.push(configuration::clientbound::PackedRegistryEntry {
                id: name,
                data: Some(wolf_variant.to_nbt().into()),
            });
        }
        vec
    }
}

impl Default for WolfVariantRegistry {
    fn default() -> Self {
        Self {
            wolves: vec![("pale".to_string(), WolfVariant {
                wild_texture: "entity/wolf/wolf_angry",
                tame_texture: "entity/wolf/wolf_tame",
                angry_texture: "entity/wolf/wolf"
            })]
        }
    }
}

// Wolf Sound Variant

pub struct WolfSoundVariantRegistry {
    wolf_sounds: Vec<(String, WolfSoundVariant)>
}

impl WolfSoundVariant {
    pub fn to_nbt(&self) -> NBT {
        let mut nbt = NBT::new();
        let mut compound = nbt.as_compound_mut().unwrap();

        compound.insert_string("ambient_sound", self.ambient_sound.to_owned());
        compound.insert_string("death_sound", self.death_sound.to_owned());
        compound.insert_string("growl_sound", self.growl_sound.to_owned());
        compound.insert_string("hurt_sound", self.hurt_sound.to_owned());
        compound.insert_string("pant_sound", self.pant_sound.to_owned());
        compound.insert_string("whine_sound", self.whine_sound.to_owned());
        
        nbt
    }
}

pub struct WolfSoundVariant {
    ambient_sound: &'static str,
    death_sound: &'static str,
    growl_sound: &'static str,
    hurt_sound: &'static str,
    pant_sound: &'static str,
    whine_sound: &'static str,
}

impl Registry for WolfSoundVariantRegistry {
    fn registry_name(&self) -> &'static str {
        "minecraft:wolf_sound_variant"
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry> {
        let mut vec = Vec::new();
        for (name, wolf_variant) in &self.wolf_sounds {
            vec.push(configuration::clientbound::PackedRegistryEntry {
                id: name,
                data: Some(wolf_variant.to_nbt().into()),
            });
        }
        vec
    }
}

impl Default for WolfSoundVariantRegistry {
    fn default() -> Self {
        Self {
            wolf_sounds: vec![("classic".to_string(), WolfSoundVariant {
                ambient_sound: "entity.wolf.ambient",
                death_sound: "entity.wolf.death",
                growl_sound: "entity.wolf.growl",
                hurt_sound: "entity.wolf.hurt",
                pant_sound: "entity.wolf.pant",
                whine_sound: "entity.wolf.whine",
            })]
        }
    }
}


// Animal Variant

pub struct AnimalVariantRegistry {
    name: &'static str,
    animals: Vec<(String, AnimalVariant)>
}

impl AnimalVariantRegistry {
    pub fn new_single_valued(name: &'static str, variant_name: &'static str, asset_id: &'static str) -> Self {
        Self {
            name,
            animals: vec![(variant_name.to_string(), AnimalVariant { asset_id })]
        }
    }
}

impl AnimalVariant {
    pub fn to_nbt(&self) -> NBT {
        let mut nbt = NBT::new();
        let mut compound = nbt.as_compound_mut().unwrap();

        compound.insert_string("asset_id", self.asset_id.to_owned());
        
        nbt
    }
}

pub struct AnimalVariant {
    asset_id: &'static str,
}

impl Registry for AnimalVariantRegistry {
    fn registry_name(&self) -> &str {
        &self.name
    }

    fn create_entries(&self) -> Vec<configuration::clientbound::PackedRegistryEntry> {
        let mut vec = Vec::new();
        for (name, animal) in &self.animals {
            vec.push(configuration::clientbound::PackedRegistryEntry {
                id: name,
                data: Some(animal.to_nbt().into()),
            });
        }
        vec
    }
}