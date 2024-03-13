use graphite_binary::nbt::{self, TAG_COMPOUND_ID, ListRefMut};

use crate::registry::{dimension_type::DimensionType, damage_type::{DamageType, DamageTypeEffect}, biome::Biome};

#[derive(Default)]
pub struct Registries {
    chat_type: ChatTypeRegistry,
    dimension_type: DimensionTypeRegistry,
    damage_type: DamageTypeRegistry,
    biomes: BiomeRegistry,
}

impl Registries {
    pub fn to_nbt(&self) -> nbt::NBT {
        let mut nbt = nbt::NBT::new();

        self.chat_type.write(&mut nbt);
        self.dimension_type.write(&mut nbt);
        self.damage_type.write(&mut nbt);
        self.biomes.write(&mut nbt);

        nbt
    }
}

pub trait Registry {
    const REGISTRY_TYPE: &'static str;

    fn write(&self, nbt: &mut nbt::NBT) {
        let mut compound = nbt.create_compound(Self::REGISTRY_TYPE);
        compound.insert_string("type", Self::REGISTRY_TYPE.into());
        let value = compound.create_list("value", TAG_COMPOUND_ID);
        self.write_values(value);
    }

    fn write_values(&self, _: ListRefMut);
}

// Chat type

#[derive(Default)]
pub struct ChatTypeRegistry {}

impl Registry for ChatTypeRegistry {
    const REGISTRY_TYPE: &'static str = "minecraft:chat_type";

    fn write_values(&self, _: ListRefMut) {
    }
}

// Dimension type

pub struct DimensionTypeRegistry {
    dimensions: Vec<(String, DimensionType)>
}

impl Registry for DimensionTypeRegistry {
    const REGISTRY_TYPE: &'static str = "minecraft:dimension_type";

    fn write_values(&self, mut list: ListRefMut) {
        for (index, (name, dimension)) in self.dimensions.iter().enumerate() {
            let mut compound = list.create_compound();
            compound.insert_string("name", name.clone());
            compound.insert_int("id", index as i32);

            let element = compound.create_compound("element");
            dimension.write(element);
        }
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
    const REGISTRY_TYPE: &'static str = "minecraft:damage_type";

    fn write_values(&self, mut list: ListRefMut) {
        for (index, (name, damage_type)) in self.damage_types.iter().enumerate() {
            let mut compound = list.create_compound();
            compound.insert_string("name", name.clone());
            compound.insert_int("id", index as i32);

            let element = compound.create_compound("element");
            damage_type.write(element);
        }
    }
}

impl Default for DamageTypeRegistry {
    fn default() -> Self {
        Self {
            damage_types: vec![
                ("minecraft:generic".into(), DamageType(DamageTypeEffect::Hurt)),
                ("minecraft:in_fire".into(), DamageType(DamageTypeEffect::Burning)),
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
    const REGISTRY_TYPE: &'static str = "minecraft:worldgen/biome";

    fn write_values(&self, mut list: ListRefMut) {
        for (index, (name, biome)) in self.biomes.iter().enumerate() {
            let mut compound = list.create_compound();
            compound.insert_string("name", name.clone());
            compound.insert_int("id", index as i32);

            let element = compound.create_compound("element");
            biome.write(element);
        }
    }
}

impl Default for BiomeRegistry {
    fn default() -> Self {
        Self {
            biomes: vec![("minecraft:plains".into(), Biome::default())]
        }
    }
}