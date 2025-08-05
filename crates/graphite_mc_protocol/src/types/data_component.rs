use std::{borrow::Cow, f32::consts::E, fmt::Debug, fs::read, hash::Hasher, mem::ManuallyDrop, sync::{atomic::{AtomicBool, AtomicI32, Ordering}, Arc}};

use anyhow::bail;
use enumset::EnumSet;
use graphite_binary::{nbt::{EncodedNBT, NBT}, slice_serialization::*};
use graphite_mc_constants::{builtin::{self, ConsumeEffectType, DataComponentType, SoundEvent}, item::Item, types::{self, EquipmentSlot}};

use crate::types::hash_ops::HashOps;

use super::{encoded_text::CachedTextComponent, text::TextComponent, GlobalPosition, ItemStack, SoundTypeOwned};

pub trait DataComponentChecksum {
    fn checksum(&self) -> i32;
}

fn hash_sound(map: &mut super::hash_ops::HashOpsMap, key: &'static str, sound: &SoundTypeOwned) {
    match sound {
        SoundTypeOwned::Event(sound_event) => {
            map.put_string(key, sound_event.location());
        },
        SoundTypeOwned::Direct(sound_id) => {
            hash_resource_location(map, key, sound_id)
        },
    }
}

fn hash_resource_location(map: &mut super::hash_ops::HashOpsMap, key: &'static str, value: &str) {
    if value.contains(":") {
        map.put_string(key, value);
    } else {
        map.put_string(key, &format!("minecraft:{}", value));
    }
}

pub trait DataComponentTrait<'r, 'd: 'r>: SliceSerializable<'r, 'd, Self> + PartialEq + Debug + Clone + DataComponentChecksum {
    const TYPE: DataComponentType;
    unsafe fn union_into(union: DataComponentUnion) -> Self;
    unsafe fn union_ref(union: &DataComponentUnion) -> &Self;
    unsafe fn union_mut_ref(union: &mut DataComponentUnion) -> &mut Self;
    fn to_union(self) -> DataComponentUnion;
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub(crate) struct MaxStackSize {
        pub(crate) size: usize as VarInt
    }
}

impl MaxStackSize {
    pub const fn get(self) -> usize {
        self.size
    }
}

impl DataComponentChecksum for MaxStackSize {
    fn checksum(&self) -> i32 {
        HashOps::hash_int(self.size as i32)
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct MaxDamage {
        max_damage: usize as VarInt,
    }
}

impl MaxDamage {
    pub const fn new(max_damage: usize) -> Option<Self> {
        if max_damage >= 1 && max_damage <= i32::MAX as usize {
            Some(Self { max_damage })
        } else {
            None
        }
    }

    pub const fn get(self) -> usize {
        self.max_damage
    }
}

impl DataComponentChecksum for MaxDamage {
    fn checksum(&self) -> i32 {
        HashOps::hash_int(self.max_damage as i32)
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct Damage {
        damage: usize as VarInt
    }
}

impl Damage {
    pub const fn new(damage: usize) -> Option<Self> {
        if damage <= i32::MAX as usize {
            Some(Self { damage })
        } else {
            None
        }
    }

    pub const fn get(self) -> usize {
        self.damage
    }
}

impl DataComponentChecksum for Damage {
    fn checksum(&self) -> i32 {
        HashOps::hash_int(self.damage as i32)
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct Unbreakable;
}

impl DataComponentChecksum for Unbreakable {
    fn checksum(&self) -> i32 {
        HashOps::hash_empty()
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct ItemName {
        pub text: CachedTextComponent
    }    
}

impl ItemName {
    pub fn new(text: TextComponent<'static>) -> Self {
        Self {
            text: text.into()
        }
    }
}

impl DataComponentChecksum for ItemName {
    fn checksum(&self) -> i32 {
        self.text.text.checksum()
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct ItemModel {
        pub id: Cow<'static, str> as StaticSizedString<256>
    }    
}

impl DataComponentChecksum for ItemModel {
    fn checksum(&self) -> i32 {
        HashOps::hash_string(&self.id)
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct Lore {
        lines: Vec<CachedTextComponent> as SizedArray<CachedTextComponent, 256>
    }
}

impl Lore {
    pub fn new(text: Vec<TextComponent<'static>>) -> Self {
        let mut cached_text = Vec::with_capacity(text.len());
        for line in text {
            cached_text.push(line.into());
        }
        Self {
            lines: cached_text
        }
    }

    pub fn push(&mut self, text: TextComponent<'static>) {
        self.lines.push(text.into());
    }
}

impl DataComponentChecksum for Lore {
    fn checksum(&self) -> i32 {
        let mut list = HashOps::start_list();
        for line in &self.lines {
            list.add_raw_checksum(line.text.checksum());
        }
        list.finish()
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct Rarity {
        pub rarity: types::Rarity as AttemptFrom<Single, u8>
    }
}

impl DataComponentChecksum for Rarity {
    fn checksum(&self) -> i32 {
        HashOps::hash_string(self.rarity.into())
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct Equippable {
        pub inner: Box<EquippableInner>,
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct EquippableInner {
        pub slot: EquipmentSlot as AttemptFrom<Single, u8>,
        pub sound: SoundTypeOwned,
        pub asset_id: Option<Cow<'static, str>> as Option<StaticSizedString>,
        pub camera_overlay: Option<Cow<'static, str>> as Option<StaticSizedString>,
        pub _always_false_entity_types: bool as Single,
        pub dispensable: bool as Single,
        pub swappable: bool as Single,
        pub damage_on_hurt: bool as Single,
        pub equip_on_interact: bool as Single
    }
}

impl DataComponentChecksum for Equippable {
    fn checksum(&self) -> i32 {
        let mut map = HashOps::start_map();

        map.put_string("slot", self.inner.slot.into());
        if self.inner.sound != SoundTypeOwned::Event(SoundEvent::ItemArmorEquipGeneric) {
            hash_sound(&mut map, "equip_sound", &self.inner.sound);
        }
        if let Some(asset_id) = self.inner.asset_id.as_ref() {
            hash_resource_location(&mut map, "asset_id", &asset_id);
        }
        if let Some(camera_overlay) = self.inner.camera_overlay.as_ref() {
            hash_resource_location(&mut map, "camera_overlay", &camera_overlay);
        }
        if !self.inner.dispensable {
            map.put_boolean("dispensable", false);
        }
        if !self.inner.swappable {
            map.put_boolean("swappable", false);
        }
        if !self.inner.damage_on_hurt {
            map.put_boolean("damage_on_hurt", false);
        }
        if self.inner.equip_on_interact {
            map.put_boolean("equip_on_interact", true);
        }

        map.finish()
    }
}



#[derive(Clone, PartialEq, Debug)]
pub enum CustomModelData {
    Float(f32),
    Flag(bool),
    String(String),
    Color(i32),
    Empty,
    Arbitrary(Box<ArbitraryCustomModelData>)
}

impl CustomModelData {
    pub fn from_vecs(floats: Vec<f32>, flags: Vec<bool>, mut strings: Vec<String>, colors: Vec<i32>) -> Self {
        let sum = floats.len() + flags.len() + strings.len() + colors.len();
        if sum == 0 {
            Self::Empty
        } else if sum == 1 {
            if floats.len() == 1 {
                Self::Float(floats[0])
            } else if flags.len() == 1 {
                Self::Flag(flags[0])
            } else if strings.len() == 1 {
                Self::String(strings.remove(0))
            } else if colors.len() == 1 {
                Self::Color(colors[0])
            } else {
                unreachable!()
            }
        } else {
            Self::Arbitrary(Box::new(ArbitraryCustomModelData {
                floats,
                flags,
                strings,
                colors
            }))
        }
    }
}

impl <'r, 'a:'r> graphite_binary::slice_serialization::SliceSerializable<'r, 'a> for CustomModelData {
    type CopyType =  &'r CustomModelData;
    fn read(bytes: &mut &'a[u8]) -> anyhow::Result<CustomModelData> {
        let arbitrary = ArbitraryCustomModelData::read(bytes)?;

        Ok(Self::from_vecs(arbitrary.floats, arbitrary.flags, arbitrary.strings, arbitrary.colors))
    }
    
    fn get_write_size(custom_model_data: &'r CustomModelData) -> usize {
        let mut size = 0;
        match custom_model_data {
            CustomModelData::Float(float) => {
                size += <Single as SliceSerializable<u8>>::get_write_size(1);
                size += <BigEndian as SliceSerializable<f32>>::get_write_size(*float);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
            },
            CustomModelData::Flag(flag) => {
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(1);
                size += <Single as SliceSerializable<bool>>::get_write_size(*flag);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
            },
            CustomModelData::String(string) => {
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(1);
                size += <SizedString as SliceSerializable<String>>::get_write_size(string);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
            },
            CustomModelData::Color(color) => {
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(1);
                size += <BigEndian as SliceSerializable<i32>>::get_write_size(*color);
            },
            CustomModelData::Empty => {
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
                size += <Single as SliceSerializable<u8>>::get_write_size(0);
            },
            CustomModelData::Arbitrary(arbitrary_custom_model_data) => {
                size += ArbitraryCustomModelData::get_write_size(arbitrary_custom_model_data);
            },
        }
        size
    }

    unsafe fn write<'bytes>(mut bytes: &'bytes mut [u8], custom_model_data: &'r CustomModelData) ->  &'bytes mut [u8] {
        match custom_model_data {
            CustomModelData::Float(float) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 1);
                bytes = <BigEndian as SliceSerializable<f32>>::write(bytes, *float);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
            },
            CustomModelData::Flag(flag) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 1);
                bytes = <Single as SliceSerializable<bool>>::write(bytes, *flag);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
            },
            CustomModelData::String(string) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 1);
                bytes = <SizedString as SliceSerializable<String>>::write(bytes, string);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
            },
            CustomModelData::Color(color) => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 1);
                bytes = <BigEndian as SliceSerializable<i32>>::write(bytes, *color);
            },
            CustomModelData::Empty => {
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
                bytes = <Single as SliceSerializable<u8>>::write(bytes, 0);
            },
            CustomModelData::Arbitrary(arbitrary_custom_model_data) => {
                bytes = ArbitraryCustomModelData::write(bytes, arbitrary_custom_model_data);
            },
        }
        bytes
    }

    #[inline(always)]
    fn as_copy_type(t: &'r CustomModelData) -> Self::CopyType {
        t
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct ArbitraryCustomModelData {
        pub floats: Vec<f32> as SizedArray<BigEndian>,
        pub flags: Vec<bool> as SizedArray<Single>,
        pub strings: Vec<String> as SizedArray<SizedString>,
        pub colors: Vec<i32> as SizedArray<BigEndian>,
    }
}

impl DataComponentChecksum for CustomModelData {
    fn checksum(&self) -> i32 {
        let mut floats = HashOps::start_list();
        let mut flags = HashOps::start_list();
        let mut strings = HashOps::start_list();
        let mut colors = HashOps::start_list();
        match self {
            CustomModelData::Float(value) => floats.add_float(*value),
            CustomModelData::Flag(value) => flags.add_boolean(*value),
            CustomModelData::String(value) => strings.add_string(value),
            CustomModelData::Color(value) => colors.add_int(*value),
            CustomModelData::Empty => {},
            CustomModelData::Arbitrary(arbitrary_custom_model_data) => {
                for value in &arbitrary_custom_model_data.floats {
                    floats.add_float(*value)
                }
                for value in &arbitrary_custom_model_data.flags {
                    flags.add_boolean(*value)
                }
                for value in &arbitrary_custom_model_data.strings {
                    strings.add_string(value)
                }
                for value in &arbitrary_custom_model_data.colors {
                    colors.add_int(*value)
                }
            },
        }
        let mut list = HashOps::start_list();
        list.add_list(floats);
        list.add_list(flags);
        list.add_list(strings);
        list.add_list(colors);
        list.finish()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TooltipDisplay {
    pub hide_tooltip: bool,
    pub hidden_components: EnumSet<DataComponentType>
}

impl <'r,'a:'r>graphite_binary::slice_serialization::SliceSerializable<'r,'a> for TooltipDisplay {
    type CopyType =  TooltipDisplay;

    #[inline(always)]
    fn as_copy_type(t: &'r TooltipDisplay) -> Self::CopyType {
        *t
    }

    fn read(bytes: &mut &'a[u8]) -> anyhow::Result<TooltipDisplay>{
        let hide_tooltip =  Single::read(bytes)?;
        let hidden_count = VarInt::read(bytes)?;

        let mut hidden_components = EnumSet::empty();
        for _ in 0..hidden_count {
            let component_type: u8 = VarInt::read(bytes)?;
            let component_type: DataComponentType = component_type.try_into()?;

            hidden_components |= component_type;
        }

        Ok(Self {
            hide_tooltip,
            hidden_components,
        })
    }

    fn get_write_size(tooltip_display: TooltipDisplay) -> usize {
        <Single as SliceSerializable<bool>>::get_write_size(tooltip_display.hide_tooltip)
            + 5
            + tooltip_display.hidden_components.len() as usize
    }

    unsafe fn write<'bytes>(mut bytes: &'bytes mut [u8], tooltip_display: TooltipDisplay) ->  &'bytes mut [u8]{
        bytes = <Single as SliceSerializable<bool>>::write(bytes, tooltip_display.hide_tooltip);

        bytes = <VarInt as SliceSerializable<i32>>::write(bytes, tooltip_display.hidden_components.len() as i32);
        for hidden_component in tooltip_display.hidden_components {
            bytes = <VarInt as SliceSerializable<u16>>::write(bytes, hidden_component as u16);
        }

        bytes
    }

}

impl DataComponentChecksum for TooltipDisplay {
    fn checksum(&self) -> i32 {
        let mut map = HashOps::start_map();
        if self.hide_tooltip {
            map.put_boolean("hide_tooltip", self.hide_tooltip);
        }

        if !self.hidden_components.is_empty() {
            let mut hidden_components = HashOps::start_list();
            for component in self.hidden_components {
                let name: &'static str = component.into();
                hidden_components.add_string(&format!("minecraft:{}", name));
            }
            map.put_list("hidden_components", hidden_components);
        }

        map.finish()
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct RepairCost {
        cost: usize as VarInt
    }
}

impl RepairCost {
    pub const fn new(cost: usize) -> Option<Self> {
        if cost <= i32::MAX as usize {
            Some(Self { cost })
        } else {
            None
        }
    }

    pub const fn get(self) -> usize {
        self.cost
    }
}

impl DataComponentChecksum for RepairCost {
    fn checksum(&self) -> i32 {
        HashOps::hash_int(self.cost as i32)
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct CreativeSlotLock;
}

impl DataComponentChecksum for CreativeSlotLock {
    fn checksum(&self) -> i32 {
        HashOps::hash_empty()
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct EnchantmentGlintOverride {
        pub glint: bool as Single
    }
}

impl DataComponentChecksum for EnchantmentGlintOverride {
    fn checksum(&self) -> i32 {
        HashOps::hash_boolean(self.glint)
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct DyedColor {
        pub rgb: i32 as BigEndian,
    }
}

impl DataComponentChecksum for DyedColor {
    fn checksum(&self) -> i32 {
        HashOps::hash_int(self.rgb)
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct MapColor {
        pub rgb: i32 as BigEndian
    }
}

impl DataComponentChecksum for MapColor {
    fn checksum(&self) -> i32 {
        HashOps::hash_int(self.rgb)
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct MapId {
        pub id: i32 as VarInt
    }
}

impl DataComponentChecksum for MapId {
    fn checksum(&self) -> i32 {
        HashOps::hash_int(self.id)
    }
}

slice_serializable! {
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct BaseColor {
        pub color: types::DyeColor as AttemptFrom<Single, u8>
    }
}

impl DataComponentChecksum for BaseColor {
    fn checksum(&self) -> i32 {
        HashOps::hash_string(self.color.into())
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct UseCooldown {
        pub seconds: f32 as BigEndian,
        pub group: Option<Cow<'static, str>> as Option<StaticSizedString<256>>,
    }
}

impl DataComponentChecksum for UseCooldown {
    fn checksum(&self) -> i32 {
        let mut map = HashOps::start_map();
        map.put_float("seconds", self.seconds);
        if let Some(group) = self.group.as_ref() {
            map.put_string("cooldown_group", &*group);
        }
        map.finish()
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct BundleContents {
        pub items: Vec<ItemStack> as SizedArray<ItemStack, 64>,
    }
}

impl DataComponentChecksum for BundleContents {
    fn checksum(&self) -> i32 {
        let mut items = HashOps::start_list();
        for item in &self.items {
            items.add_raw_checksum(item.checksum());
        }
        items.finish()
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct Consumable {
        pub inner: Box<ConsumableInner>
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct ConsumableInner {
        pub consume_seconds: f32 as BigEndian,
        pub animation: types::ItemUseAnimation as AttemptFrom<Single, u8>,
        pub sound: SoundTypeOwned,
        pub has_consume_particles: bool as Single,
        pub consume_effects: Vec<ConsumeEffectType> as SizedArray<AttemptFrom<Single, u8>, 16>,
    }
}

impl DataComponentChecksum for Consumable {
    fn checksum(&self) -> i32 {
        let mut map = HashOps::start_map();
        if self.inner.consume_seconds != 1.6 {
            map.put_float("consume_seconds", self.inner.consume_seconds);
        }
        if self.inner.animation != types::ItemUseAnimation::Eat {
            map.put_string("animation", self.inner.animation.into());
        }
        if self.inner.sound != SoundTypeOwned::Event(SoundEvent::EntityGenericEat) {
            hash_sound(&mut map, "sound", &self.inner.sound);
        }
        if !self.inner.has_consume_particles {
            map.put_boolean("has_consume_particles", false);
        }
        if !self.inner.consume_effects.is_empty() {
            let mut consume_list = HashOps::start_list();
            for consume_effect in &self.inner.consume_effects {
                let name: &'static str = consume_effect.into();
                consume_list.add_string(&format!("minecraft:{}", name));
            }
            map.put_list("on_consume_effects", consume_list);
        }

        map.finish()
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct LodestoneTracker {
        pub inner: Box<LodestoneTrackerInner>
    }
}

slice_serializable! {
    #[derive(Clone, PartialEq, Debug)]
    pub struct LodestoneTrackerInner {
        pub target: Option<GlobalPosition>,
        pub tracked: bool as Single
    }
}

impl DataComponentChecksum for LodestoneTracker {
    fn checksum(&self) -> i32 {
        let mut map = HashOps::start_map();
        if let Some(ref target) = self.inner.target {
            let mut target_map = HashOps::start_map();
            target_map.put_string("dimension", &*target.dimension);
            let position_hash = HashOps::hash_int_list(&[target.position.x, target.position.y, target.position.z]);
            target_map.put_raw_checksum("pos", position_hash);
            map.put_map("target", target_map);
        }
        if !self.inner.tracked {
            map.put_boolean("tracked", false);
        }
        map.finish()
    }
}

macro_rules! define_data_components {
    (copy { $($t1:ident $(,)?)* }, manually_drop { $($t2:ident $(,)?)* }, ignored { $($t3:ident $(,)?)* }) => {
        paste::paste! {
            #[doc(hidden)]
            pub union DataComponentUnion {
                $(
                    [<$t1:snake>]: $t1,
                )*
                $(
                    [<$t2:snake>]: ManuallyDrop<$t2>,
                )*
            }

            impl DataComponentUnion {
                unsafe fn manually_drop(self, component_type: DataComponentType) {
                    match component_type {
                        $(
                            DataComponentType::$t2 => std::mem::drop(ManuallyDrop::into_inner(self.[<$t2:snake>])),
                        )*
                        $( DataComponentType::$t1 => {}, )*
                        $( DataComponentType::$t3 => {}, )*
                        _ => {}
                    }
                }
                unsafe fn equals(&self, other: &Self, component_type: DataComponentType) -> bool {
                    match component_type {
                        $(
                            DataComponentType::$t1 => self.[<$t1:snake>] == other.[<$t1:snake>],
                        )*
                        $(
                            DataComponentType::$t2 => self.[<$t2:snake>] == other.[<$t2:snake>],
                        )*
                        _ => panic!("unsupported component type: {:?}", component_type)
                    }
                }
                unsafe fn as_fmt_debug(&self, component_type: DataComponentType) -> &dyn std::fmt::Debug {
                    match component_type {
                        $(
                            DataComponentType::$t1 => &self.[<$t1:snake>],
                        )*
                        $(
                            DataComponentType::$t2 => &self.[<$t2:snake>],
                        )*
                        _ => panic!("unsupported component type: {:?}", component_type)
                    }
                }
                unsafe fn as_checksum(&self, component_type: DataComponentType) -> &dyn DataComponentChecksum {
                    match component_type {
                        $(
                            DataComponentType::$t1 => &self.[<$t1:snake>],
                        )*
                        $(
                            DataComponentType::$t2 => &*self.[<$t2:snake>],
                        )*
                        _ => panic!("unsupported component type: {:?}", component_type)
                    }
                }
                unsafe fn clone(&self, component_type: DataComponentType) -> Self {
                    match component_type {
                        $(
                            DataComponentType::$t1 => Self { [<$t1:snake>]: self.[<$t1:snake>].clone() },
                        )*
                        $(
                            DataComponentType::$t2 => Self { [<$t2:snake>]: self.[<$t2:snake>].clone() },
                        )*
                        _ => panic!("unsupported component type: {:?}", component_type)
                    }
                }
                fn read(component_type: DataComponentType, bytes: &mut &[u8]) -> anyhow::Result<Self> {
                    match component_type {
                        $(
                            DataComponentType::$t1 => Ok(Self { [<$t1:snake>]: $t1::read(bytes)? }),
                        )*
                        $(
                            DataComponentType::$t2 => Ok(Self { [<$t2:snake>]: ManuallyDrop::new($t2::read(bytes)?) }),
                        )*
                        _ => bail!("unsupported component type: {:?}", component_type)
                    }
                }
                unsafe fn write<'b>(&self, component_type: DataComponentType, bytes: &'b mut [u8]) -> &'b mut [u8] {
                    match component_type {
                        $(
                            DataComponentType::$t1 => $t1::write(bytes, $t1::as_copy_type(&self.[<$t1:snake>])),
                        )*
                        $(
                            DataComponentType::$t2 => $t2::write(bytes, $t2::as_copy_type(&self.[<$t2:snake>])),
                        )*
                        _ => panic!("unsupported component type: {:?}", component_type)
                    }
                }
                unsafe fn get_write_size(&self, component_type: DataComponentType) -> usize {
                    match component_type {
                        $(
                            DataComponentType::$t1 => $t1::get_write_size($t1::as_copy_type(&self.[<$t1:snake>])),
                        )*
                        $(
                            DataComponentType::$t2 => $t2::get_write_size($t2::as_copy_type(&self.[<$t2:snake>])),
                        )*
                        _ => panic!("unsupported component type: {:?}", component_type)
                    }
                }
            }

            $(
                impl <'r, 'd: 'r> DataComponentTrait<'r, 'd> for $t1 {
                    const TYPE: DataComponentType = DataComponentType::$t1;
                    unsafe fn union_into(union: DataComponentUnion) -> Self {
                        union.[<$t1:snake>]
                    }
                    unsafe fn union_ref(union: &DataComponentUnion) -> &Self {
                        &union.[<$t1:snake>]
                    }
                    unsafe fn union_mut_ref(union: &mut DataComponentUnion) -> &mut Self {
                        &mut union.[<$t1:snake>]
                    }
                    fn to_union(self) -> DataComponentUnion {
                        DataComponentUnion {
                            [<$t1:snake>]: self
                        }
                    }
                }
            )*
            $(
                impl <'r, 'd: 'r> DataComponentTrait<'r, 'd> for $t2 {
                    const TYPE: DataComponentType = DataComponentType::$t2;
                    unsafe fn union_into(union: DataComponentUnion) -> Self {
                        ManuallyDrop::into_inner(union.[<$t2:snake>])
                    }
                    unsafe fn union_ref(union: &DataComponentUnion) -> &Self {
                        &union.[<$t2:snake>]
                    }
                    unsafe fn union_mut_ref(union: &mut DataComponentUnion) -> &mut Self {
                        &mut union.[<$t2:snake>]
                    }
                    fn to_union(self) -> DataComponentUnion {
                        DataComponentUnion {
                            [<$t2:snake>]: ManuallyDrop::new(self)
                        }
                    }
                }
            )*
        }
    };
}

// This doesn't matter, but is useful to keep track of the memory footprint of the union
static_assertions::const_assert_eq!(std::mem::size_of::<DataComponentUnion>(), 32);

define_data_components! {
    copy {
        MaxStackSize,
        MaxDamage,
        Damage,
        Unbreakable,
        Rarity,
        TooltipDisplay,
        RepairCost,
        CreativeSlotLock,
        EnchantmentGlintOverride,
        DyedColor,
        MapColor,
        MapId,
        BaseColor,
    },
    manually_drop {
        ItemName,
        ItemModel,
        Lore,
        Equippable,
        CustomModelData,
        // CanPlaceOn,
        // CanBreak,
        // AttributeModifiers,
        // Food,
        // Tool,
        // StoredEnchantments,
        // ChargedProjectiles,
        BundleContents,
        Consumable,
        // PotionContents,
        // SuspiciousStewEffects,
        // WritableBookContent,
        // WrittenBookContent,
        // EntityData,
        // BucketEntityData,
        // BlockEntityData,
        LodestoneTracker,
        // FireworkExplosion,
        // Fireworks,
        // Profile,
        // NoteBlockSound,
        // BannerPatterns,
        // Container,
        // BlockState,
        // Bees,
        UseCooldown
    },
    ignored {
        CustomData,
        IntangibleProjectile,
        MapDecorations,
        DebugStickState,
        Recipes,
        Lock,
        ContainerLoot,
        PotDecorations,
    }
}

struct DataComponentEntry {
    key: DataComponentType,
    value: DataComponentUnion,
    checksum: AtomicI32,
    checksum_calculated: AtomicBool
}

impl DataComponentEntry {
    pub fn unset_checksum(&self) {
        self.checksum_calculated.store(false, Ordering::Relaxed);
        self.checksum.store(i32::MIN, Ordering::Relaxed);
    }

    pub fn checksum(&self) -> i32 {
        if self.checksum_calculated.load(Ordering::Relaxed) {
            self.checksum.load(Ordering::Relaxed)
        } else {
            let checksum = unsafe { self.value.as_checksum(self.key).checksum() };
            self.checksum.store(checksum, Ordering::Relaxed);
            self.checksum_calculated.store(true, Ordering::Relaxed);
            checksum
        }
    }
}

#[derive(Default, Clone)]
pub struct DataComponentMap {
    elements: Option<Arc<Vec<DataComponentEntry>>>,
    present: EnumSet<DataComponentType>,
    removed: EnumSet<DataComponentType>
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for DataComponentMap {
    type CopyType = &'r DataComponentMap;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Self> {
        let added_count: i32 = VarInt::read(bytes)?;
        if added_count < 0 {
            bail!("added count must not be negative, got {}", added_count);
        }
        if added_count > DataComponentType::COUNT as i32 {
            bail!("too many added data components, maximum is {}, got {}", DataComponentType::COUNT, added_count)
        }

        let removed_count: i32 = VarInt::read(bytes)?;
        if removed_count < 0 {
            bail!("removed count must not be negative, got {}", removed_count);
        }
        if removed_count > DataComponentType::COUNT as i32 {
            bail!("too many removed data components, maximum is {}, got {}", DataComponentType::COUNT, removed_count)
        }

        let mut elements: Vec<DataComponentEntry> = Vec::with_capacity(added_count as usize);
        let mut present_components = EnumSet::empty();

        for _ in 0..added_count {
            let component_type: u8 = VarInt::read(bytes)?;
            let component_type: DataComponentType = component_type.try_into()?;
            
            let result = elements.binary_search_by_key(&(component_type as u8),
                |e| e.key as u8);
            match result {
                Ok(_) => bail!("duplicate component type: {:?}", component_type),
                Err(absent) => {
                    let value = DataComponentUnion::read(component_type, bytes)?;
                    elements.insert(absent, DataComponentEntry {
                        key: component_type,
                        value,
                        checksum: AtomicI32::new(i32::MIN),
                        checksum_calculated: AtomicBool::new(false)
                    });
                    present_components |= component_type;
                },
            }
        }

        let mut removed_components = EnumSet::empty();
        for _ in 0..removed_count {
            let component_type: u8 = VarInt::read(bytes)?;
            let component_type: DataComponentType = component_type.try_into()?;

            removed_components |= component_type;
        }

        if elements.is_empty() {
            Ok(Self {
                elements: None,
                present: present_components,
                removed: removed_components
            })
        } else {
            Ok(Self {
                elements: Some(Arc::new(elements)),
                present: present_components,
                removed: removed_components
            })
        }
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        // Write counts
        let len = if let Some(elements) = &data.elements {
            elements.len()
        } else {
            0
        };
        bytes = <VarInt as SliceSerializable<i32>>::write(bytes, len as i32);
        bytes = <VarInt as SliceSerializable<i32>>::write(bytes, data.removed.len() as i32);

        // Write added components
        if let Some(elements) = &data.elements {
            for entry in elements.iter() {
                bytes = <VarInt as SliceSerializable<u16>>::write(bytes, entry.key as u16);
                bytes = entry.value.write(entry.key, bytes);
            }
        }

        // Write removed components
        for removed in data.removed {
            bytes = <VarInt as SliceSerializable<u16>>::write(bytes, removed as u16);
        }

        bytes
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        // Counts
        let len = if let Some(elements) = &data.elements {
            elements.len()
        } else {
            0
        };
        let mut size = <VarInt as SliceSerializable<i32>>::get_write_size(len as i32);
        let removed_count = data.removed.len();
        size += <VarInt as SliceSerializable<i32>>::get_write_size(removed_count as i32);

        // Added components
        if let Some(elements) = &data.elements {
            for entry in elements.iter() {
                size += <VarInt as SliceSerializable<u16>>::get_write_size(entry.key as u16);
                size += unsafe { entry.value.get_write_size(entry.key) };
            }
        }

        // Removed components
        size += removed_count as usize;

        size
    }
}

impl DataComponentMap {
    pub const fn new() -> Self {
        Self {
            elements: None,
            present: EnumSet::empty(),
            removed: EnumSet::empty()
        }
    }

    pub fn present(&self) -> EnumSet<DataComponentType> {
        self.present
    }

    fn copy_if_needed(&mut self) -> &mut Vec<DataComponentEntry> {
        if let Some(elements) = &mut self.elements {
            if Arc::strong_count(&elements) > 1 {
                let mut new_elements = Vec::with_capacity(elements.len());
    
                for entry in elements.iter() {
                    //(*t, unsafe { v.clone(*t) })
                    new_elements.push(DataComponentEntry {
                        key: entry.key,
                        value: unsafe { entry.value.clone(entry.key) },
                        checksum: AtomicI32::new(entry.checksum.load(Ordering::Relaxed)),
                        checksum_calculated: AtomicBool::new(entry.checksum_calculated.load(Ordering::Relaxed)),
                    });
                }
        
                self.elements = Some(Arc::new(new_elements));
            }
        } else {
            self.elements = Some(Arc::new(Vec::new()));
            self.present = EnumSet::empty();
        }

        Arc::get_mut(self.elements.as_mut().unwrap()).unwrap()
    }

    fn search<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&self) -> Result<usize, usize> {
        if let Some(elements) = &self.elements {
            elements.binary_search_by_key(&(T::TYPE as u16), |entry| {
                entry.key as u16
            })
        } else {
            Err(0)
        }
    }

    pub fn set<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self, t: T) -> Option<T> {
        self.removed.remove(T::TYPE);
        match self.search::<T>() {
            Ok(present) => {
                let elements = self.copy_if_needed();
                let entry = &mut elements[present];
                debug_assert_eq!(T::TYPE, entry.key);

                // SAFETY: Binary search ensures that component type is correct
                let existing = unsafe { T::union_mut_ref(&mut entry.value) };
                let existing_value = std::mem::replace(existing, t);

                entry.unset_checksum();
                Some(existing_value)
            },
            Err(absent) => {
                let elements = self.copy_if_needed();
                elements.insert(absent, DataComponentEntry {
                    key: T::TYPE,
                    value: t.to_union(),
                    checksum: AtomicI32::new(i32::MIN),
                    checksum_calculated: AtomicBool::new(false)
                });
                self.present |= T::TYPE;
                None
            },
        }
    }

    pub fn has<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&self) -> bool {
        return self.present.contains(T::TYPE);
    }

    pub fn get<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&self) -> Option<&T> {
        if !self.present.contains(T::TYPE) {
            return None;
        }
        match self.search::<T>() {
            Ok(present) => {
                let entry = &self.elements.as_ref().unwrap()[present];
                debug_assert_eq!(T::TYPE, entry.key);

                // SAFETY: Binary search ensures that component type is correct
                Some(unsafe { T::union_ref(&entry.value) })
            },
            Err(_) => None,
        }
    }

    pub fn get_mut<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self) -> Option<&mut T> {
        if !self.present.contains(T::TYPE) {
            return None;
        }
        match self.search::<T>() {
            Ok(present) => {
                let elements = self.copy_if_needed();
                let entry = &mut elements[present];
                debug_assert_eq!(T::TYPE, entry.key);

                entry.unset_checksum();

                // SAFETY: Binary search ensures that component type is correct
                Some(unsafe { T::union_mut_ref(&mut entry.value) })
            },
            Err(_) => None,
        }
    }

    pub fn retain_only(&mut self, types: EnumSet<DataComponentType>) {
        let Some(elements) = &mut self.elements else {
            return;
        };

        let mut index = 0;
        while index < elements.len() {
            if !types.contains(elements[index].key) {
                break;
            }

            index += 1;
        }

        if index >= elements.len() {
            return;
        }

        let mut present = self.present;
        let elements = self.copy_if_needed();

        while index < elements.len() {
            if !types.contains(elements[index].key) {
                let entry = elements.remove(index);
                present.remove(entry.key);
                unsafe {
                    entry.value.manually_drop(entry.key);
                }
            } else {
                index += 1;
            }
        }

        self.present = present;
    }

    pub fn remove<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self) -> bool {
        if !self.present.contains(T::TYPE) {
            return false;
        }
        match self.search::<T>() {
            Ok(present) => {
                let elements = self.copy_if_needed();

                let entry = elements.remove(present);
                self.present.remove(T::TYPE);
                debug_assert_eq!(T::TYPE, entry.key);

                std::mem::drop(unsafe { T::union_into(entry.value) });
                true
            },
            Err(_) => false,
        }
    }

    pub fn remove_fully<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self) {
        self.remove::<T>();
        self.removed |= T::TYPE;
    }

    pub fn take<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self) -> Option<T> {
        if !self.present.contains(T::TYPE) {
            return None;
        }
        match self.search::<T>() {
            Ok(present) => {
                let elements = self.copy_if_needed();

                let entry = elements.remove(present);
                self.present.remove(T::TYPE);
                debug_assert_eq!(T::TYPE, entry.key);

                // SAFETY: Binary search ensures that component type is correct
                Some(unsafe { T::union_into(entry.value) })
            },
            Err(_) => None,
        }
    }

    pub fn checksum(&self, item: Item) -> i32 {
        let properties = item.get_properties();

        let mut map = HashOps::start_map();

        if let Some(elements) = &self.elements {
            for entry in elements.iter() {
                let id: &'static str = entry.key.into();
                let checksum = entry.checksum();

                let default_checksum = properties.default_component_hashes[entry.key];
                if let Some(default_checksum) = default_checksum {
                    if default_checksum == checksum {
                        continue;
                    }
                }

                map.put_raw_checksum(&format!("minecraft:{}", id), checksum);
            }
        }

        for removed in self.removed {
            let id: &'static str = removed.into();

            let default_checksum = properties.default_component_hashes[removed];
            if default_checksum.is_none() {
                continue;
            }

            map.put_empty(&format!("!minecraft:{}", id));
        }

        map.finish()
    }
    
    pub(crate) fn raw_hashed_equals(&self, item: Item, added_components: &Vec<(DataComponentType, i32)>, removed_components: EnumSet<DataComponentType>) -> bool {
        let properties = item.get_properties();
        if self.removed & properties.default_components != removed_components {
            println!("Removed component stuff is wrong");
            return false;
        }
        // Note added_components may not have more components than self, but it can have less
        // This is because vanilla removes components that match the default value
        let Some(elements) = &self.elements else {
            return added_components.is_empty();
        };
        let elements = &**elements;
        if added_components.len() > elements.len() {
            return false;
        }
        let mut added_component_index = 0;
        for entry in elements.iter() {
            let checksum = entry.checksum();

            let compare_checksum ;
            if added_component_index >= added_components.len() {
                let default_hash = properties.default_component_hashes[entry.key];
                if let Some(default_hash) = default_hash {
                    compare_checksum = default_hash;
                } else {
                    return false;
                }
            } else {
                let (added_type, added_checksum) = added_components[added_component_index];
                if added_type != entry.key {
                    let default_hash = properties.default_component_hashes[entry.key];
                    if let Some(default_hash) = default_hash {
                        compare_checksum = default_hash;
                    } else {
                        return false;
                    }
                } else {
                    compare_checksum = added_checksum;
                    added_component_index += 1;
                }
            }

            if checksum != compare_checksum {
                println!("Checksum mismatch for {:?}. Server {}, client {}. Item: {:?}", entry.key, checksum, compare_checksum, self);
                return false;
            }
        }
        if added_component_index < added_components.len() {
            return false;
        }

        return true;
    }
}

impl Drop for DataComponentMap {
    fn drop(&mut self) {
        let Some(elements) = &mut self.elements else {
            return;
        };
        let Some(inner) = Arc::get_mut(elements) else {
            return;
        };
        for entry in inner.drain(..) {
            unsafe {
                entry.value.manually_drop(entry.key);
            }
        }
    }
}

impl PartialEq for DataComponentMap {
    fn eq(&self, other: &Self) -> bool {
        if self.removed != other.removed || self.present != other.present {
            return false;
        }

        let Some(self_elements) = &self.elements else {
            return other.elements.is_none() || other.elements.as_ref().unwrap().len() == 0;
        };
        let Some(other_elements) = &other.elements else {
            return self.elements.is_none() || self.elements.as_ref().unwrap().len() == 0;
        };

        if Arc::ptr_eq(self_elements, other_elements) {
            return true;
        }

        if self_elements.len() != other_elements.len() {
            return false
        }

        for (entry1, entry2) in self_elements.iter().zip(other_elements.iter()) {
            if entry1.key != entry2.key {
                return false;
            }
            if unsafe { !entry1.value.equals(&entry2.value, entry1.key) } {
                return false;
            }
        }

        true
    }
}

impl Debug for DataComponentMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug_list = f.debug_list();
        if let Some(elements) = &self.elements {
            for entry in elements.iter() {
                debug_list.entry(unsafe { entry.value.as_fmt_debug(entry.key) });
            }
        }
        for removed in self.removed {
            struct NotDataComponentTypeFormatHelper(DataComponentType);
            impl Debug for NotDataComponentTypeFormatHelper {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("!")?;
                    self.0.fmt(f)
                }
            }
            debug_list.entry(&NotDataComponentTypeFormatHelper(removed));
        }
        debug_list.finish()
    }
}

#[derive(Default, Clone)]
pub struct DataComponentPredicate {
    elements: Option<Arc<Vec<(DataComponentType, DataComponentUnion)>>>,
}

impl <'r, 'd: 'r> SliceSerializable<'r, 'd> for DataComponentPredicate {
    type CopyType = &'r DataComponentPredicate;

    fn as_copy_type(t: &'r Self) -> Self::CopyType {
        t
    }

    fn read(bytes: &mut &'d [u8]) -> anyhow::Result<Self> {
        let count: i32 = VarInt::read(bytes)?;
        if count < 0 {
            bail!("count must not be negative, got {}", count);
        }
        if count > DataComponentType::COUNT as i32 {
            bail!("too many data components, maximum is {}, got {}", DataComponentType::COUNT, count)
        }

        let mut elements = Vec::with_capacity(count as usize);

        for _ in 0..count {
            let component_type: u8 = VarInt::read(bytes)?;
            let component_type: DataComponentType = component_type.try_into()?;
            
            let result = elements.binary_search_by_key(&(component_type as u8), |(t, _)| {
                *t as u8
            });
            match result {
                Ok(_) => bail!("duplicate component type: {:?}", component_type),
                Err(absent) => {
                    let value = DataComponentUnion::read(component_type, bytes)?;
                    elements.insert(absent, (component_type, value));
                },
            }
        }

        if elements.is_empty() {
            Ok(Self {
                elements: None
            })
        } else {
            Ok(Self {
                elements: Some(Arc::new(elements))
            })
        }
    }

    unsafe fn write(mut bytes: &mut [u8], data: Self::CopyType) -> &mut [u8] {
        // Write counts
        let len = if let Some(elements) = &data.elements {
            elements.len()
        } else {
            0
        };
        bytes = <VarInt as SliceSerializable<i32>>::write(bytes, len as i32);

        // Write added components
        if let Some(elements) = &data.elements {
            for (t, v) in elements.iter() {
                bytes = <VarInt as SliceSerializable<u16>>::write(bytes, *t as u16);
                bytes = v.write(*t, bytes);
            }
        }

        bytes
    }

    fn get_write_size(data: Self::CopyType) -> usize {
        // Counts
        let len = if let Some(elements) = &data.elements {
            elements.len()
        } else {
            0
        };
        let mut size = <VarInt as SliceSerializable<i32>>::get_write_size(len as i32);

        // Added components
        if let Some(elements) = &data.elements {
            for (t, v) in elements.iter() {
                size += <VarInt as SliceSerializable<u16>>::get_write_size(*t as u16);
                size += unsafe { v.get_write_size(*t) };
            }
        }

        size
    }
}

impl DataComponentPredicate {
    pub const fn new() -> Self {
        Self {
            elements: None
        }
    }

    fn copy_if_needed(&mut self) -> &mut Vec<(DataComponentType, DataComponentUnion)> {
        if let Some(elements) = &mut self.elements {
            if Arc::strong_count(&elements) > 1 {
                let mut new_elements = Vec::with_capacity(elements.len());
    
                for (t, v) in elements.iter() {
                    new_elements.push((*t, unsafe { v.clone(*t) }));
                }
        
                self.elements = Some(Arc::new(new_elements));
            }
        } else {
            self.elements = Some(Arc::new(Vec::new()));
        }

        Arc::get_mut(self.elements.as_mut().unwrap()).unwrap()
    }

    fn search<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&self) -> Result<usize, usize> {
        if let Some(elements) = &self.elements {
            elements.binary_search_by_key(&(T::TYPE as u16), |(t, _)| {
                *t as u16
            })
        } else {
            Err(0)
        }
    }

    pub fn set<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self, t: T) -> Option<T> {
        match self.search::<T>() {
            Ok(present) => {
                let elements = self.copy_if_needed();
                let (found, union) = &mut elements[present];
                debug_assert_eq!(T::TYPE, *found);

                // SAFETY: Binary search ensures that component type is correct
                let existing = unsafe { T::union_mut_ref(union) };

                Some(std::mem::replace(existing, t))
            },
            Err(absent) => {
                let elements = self.copy_if_needed();
                elements.insert(absent, (T::TYPE, t.to_union()));
                None
            },
        }
    }

    pub fn get<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&self) -> Option<&T> {
        match self.search::<T>() {
            Ok(present) => {
                let (found, union) = &self.elements.as_ref().unwrap()[present];
                debug_assert_eq!(T::TYPE, *found);

                // SAFETY: Binary search ensures that component type is correct
                Some(unsafe { T::union_ref(union) })
            },
            Err(_) => None,
        }
    } 

    pub fn remove<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self) -> bool {
        match self.search::<T>() {
            Ok(present) => {
                let elements = self.copy_if_needed();

                let (found, union) = elements.remove(present);
                debug_assert_eq!(T::TYPE, found);

                std::mem::drop(unsafe { T::union_into(union) });
                true
            },
            Err(_) => false,
        }
    }

    pub fn take<'r, 'd: 'r, T: DataComponentTrait<'r, 'd>>(&mut self) -> Option<T> {
        match self.search::<T>() {
            Ok(present) => {
                let elements = self.copy_if_needed();

                let (found, union) = elements.remove(present);
                debug_assert_eq!(T::TYPE, found);

                // SAFETY: Binary search ensures that component type is correct
                Some(unsafe { T::union_into(union) })
            },
            Err(_) => None,
        }
    } 
}

impl Drop for DataComponentPredicate {
    fn drop(&mut self) {
        let Some(elements) = &mut self.elements else {
            return;
        };
        let Some(inner) = Arc::get_mut(elements) else {
            return;
        };
        for (component_type, union) in inner.drain(..) {
            unsafe {
                union.manually_drop(component_type);
            }
        }
    }
}

impl PartialEq for DataComponentPredicate {
    fn eq(&self, other: &Self) -> bool {
        let Some(self_elements) = &self.elements else {
            return other.elements.is_none() || other.elements.as_ref().unwrap().len() == 0;
        };
        let Some(other_elements) = &other.elements else {
            return self.elements.is_none() || self.elements.as_ref().unwrap().len() == 0;
        };

        if self_elements.len() != other_elements.len() {
            return false
        }

        for ((t1, v1), (t2, v2)) in self_elements.iter().zip(other_elements.iter()) {
            if t1 != t2 {
                return false;
            }
            if unsafe { !v1.equals(v2, *t1) } {
                return false;
            }
        }

        true
    }
}

impl Debug for DataComponentPredicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug_list = f.debug_list();
        if let Some(elements) = &self.elements {
            for (t, v) in elements.iter() {
                debug_list.entry(unsafe { v.as_fmt_debug(*t) });
            }
        }
        debug_list.finish()
    }
}