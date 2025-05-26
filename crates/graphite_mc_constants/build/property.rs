use std::collections::HashMap;
use std::{fmt::Write as _, collections::BTreeMap};
use std::io::Write;

use anyhow::bail;
use convert_case::{Case, Casing};
use indexmap::IndexMap;
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Property {
    #[serde(rename_all = "camelCase")]
    Int {
        values: Vec<i32>,
        default_value: i32,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_value: Option<i32>,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_method: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Bool {
        default_value: bool,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_value: Option<bool>,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_method: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    String {
        values: Vec<String>,
        default_value: String,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_value: Option<String>,
        // #[serde(skip_serializing_if = "Option::is_none")]
        // placement_method: Option<String>,
    },
}


#[derive(Default)]
pub struct PropertyWriter {
    already_aliased: IndexMap<String, Vec<(String, Vec<String>)>>,
    definitions: IndexMap<String, Vec<String>>,
    aliases: IndexMap<(String, Vec<String>), String>,
    code: IndexMap<String, String>,

    placement_method_returns: IndexMap<String, String>,
}

impl PropertyWriter {
    pub fn define_property(
        &mut self,
        name: &String,
        values: &Vec<String>,
        placement_method: Option<String>,
    ) -> anyhow::Result<()> {
        if let Some(previous_aliases) = self.already_aliased.get_mut(name) {
            for (alias, previous_alias_value) in previous_aliases.iter() {
                if previous_alias_value == values {
                    // Already defined, no need to do anything
                    if let Some(placement_method) = placement_method {
                        let old = self
                            .placement_method_returns
                            .insert(placement_method.clone(), alias.clone());
                        if let Some(old) = old {
                            if old.as_str() != alias.as_str() {
                                bail!(
                                    "Duplicate placement method `{}`, for both {} and {} (1)",
                                    placement_method,
                                    old,
                                    alias
                                )
                            }
                        }
                    }
                    return Ok(());
                }
            }

            let alias = Self::resolve_clash(name, values)?;
            previous_aliases.push((alias.clone(), values.clone()));
            self.code.insert(alias.clone(), Self::codegen(values));
            self.aliases
                .insert((name.clone(), values.clone()), alias.clone());
            if let Some(placement_method) = placement_method {
                let old = self
                    .placement_method_returns
                    .insert(placement_method.clone(), alias.clone());
                if let Some(old) = old {
                    if old.as_str() != alias.as_str() {
                        bail!(
                            "Duplicate placement method `{}`, for both {} and {} (2)",
                            placement_method,
                            old,
                            alias
                        )
                    }
                }
            }
            return Ok(());
        }

        if let Some(defined) = self.definitions.get(name) {
            if defined == values {
                // Already defined, no need to do anything
                if let Some(placement_method) = placement_method {
                    let old = self
                        .placement_method_returns
                        .insert(placement_method.clone(), name.clone());
                    if let Some(old) = old {
                        if old.as_str() != name.as_str() {
                            bail!(
                                "Duplicate placement method `{}`, for both {} and {} (3)",
                                placement_method,
                                old,
                                name
                            )
                        }
                    }
                }
                Ok(())
            } else {
                // Already defined, but with different values... need to alias

                let mut alias_values = Vec::new();

                // Move previous definition
                let previous_code = self.code.remove(name).unwrap();
                let alias = Self::resolve_clash(name, defined)?;
                self.code.insert(alias.clone(), previous_code);
                self.aliases
                    .insert((name.clone(), defined.clone()), alias.clone());
                for (_, old_name) in &mut self.placement_method_returns {
                    if old_name.as_str() == name.as_str() {
                        old_name.clear();
                        old_name.push_str(alias.as_str());
                    }
                }
                alias_values.push((alias.clone(), defined.clone()));

                // Write new definition
                let alias = Self::resolve_clash(name, values)?;
                self.code.insert(alias.clone(), Self::codegen(values));
                self.aliases
                    .insert((name.clone(), values.clone()), alias.clone());
                if let Some(placement_method) = placement_method {
                    let old = self
                        .placement_method_returns
                        .insert(placement_method.clone(), alias.clone());
                    if let Some(old) = old {
                        if old.as_str() != alias.as_str() {
                            bail!(
                                "Duplicate placement method `{}`, for both {} and {} (4)",
                                placement_method,
                                old,
                                alias
                            )
                        }
                    }
                }
                alias_values.push((alias.clone(), values.clone()));

                // Insert already aliased
                self.already_aliased.insert(name.clone(), alias_values);

                Ok(())
            }
        } else {
            self.code.insert(String::from(name), Self::codegen(values));
            self.definitions.insert(name.clone(), values.clone());
            if let Some(placement_method) = placement_method {
                let old = self
                    .placement_method_returns
                    .insert(placement_method.clone(), name.clone());
                if let Some(old) = old {
                    if old.as_str() != name.as_str() {
                        bail!(
                            "Duplicate placement method `{}`, for both {} and {} (5)",
                            placement_method,
                            old,
                            name
                        )
                    }
                }
            }
            Ok(())
        }
    }

    fn resolve_clash(name: &str, values: &[String]) -> anyhow::Result<String> {
        let values: Vec<&str> = values
            .iter()
            .map(|f| f.as_str())
            .collect();

        match name {
            "facing" => match values.as_slice() {
                ["north", "east", "south", "west", "up", "down"] => {
                    return Ok(String::from("Facing"))
                }
                ["down", "north", "south", "west", "east"] => {
                    return Ok(String::from("DirectionOrDown"))
                }
                ["north", "south", "west", "east"] => return Ok(String::from("Direction")),
                _ => {}
            },
            "half" => match values.as_slice() {
                ["top", "bottom"] => return Ok(String::from("Half")),
                ["upper", "lower"] => return Ok(String::from("UpperOrLower")),
                _ => {}
            },
            "shape" => match values.as_slice() {
                ["north_south", "east_west", "ascending_east", "ascending_west", "ascending_north", "ascending_south", "south_east", "south_west", "north_west", "north_east"] => {
                    return Ok(String::from("RailShape"))
                }
                ["north_south", "east_west", "ascending_east", "ascending_west", "ascending_north", "ascending_south"] => {
                    return Ok(String::from("StraightRailShape"))
                }
                ["straight", "inner_left", "inner_right", "outer_left", "outer_right"] => {
                    return Ok(String::from("StairShape"))
                }
                _ => {}
            },
            "type" => match values.as_slice() {
                ["normal", "sticky"] => return Ok(String::from("PistonType")),
                ["single", "left", "right"] => return Ok(String::from("ChestType")),
                ["top", "bottom", "double"] => return Ok(String::from("SlabType")),
                _ => {}
            },
            "axis" => match values.as_slice() {
                ["x", "y", "z"] => return Ok(String::from("Axis3D")),
                ["x", "z"] => return Ok(String::from("Axis2D")),
                _ => {}
            },
            "mode" => match values.as_slice() {
                ["compare", "subtract"] => return Ok(String::from("ComparatorMode")),
                ["save", "load", "corner", "data"] => {
                    return Ok(String::from("StructureBlockMode"))
                },
                ["start", "log", "fail", "accept"] => return Ok(String::from("TestBlockMode")),
                _ => {}
            },
            "north" => match values.as_slice() {
                ["up", "side", "none"] => return Ok(String::from("WireConnection")),
                ["none", "low", "tall"] => return Ok(String::from("WallConnection")),
                _ => {}
            },
            "east" => match values.as_slice() {
                ["up", "side", "none"] => return Ok(String::from("WireConnection")),
                ["none", "low", "tall"] => return Ok(String::from("WallConnection")),
                _ => {}
            },
            "south" => match values.as_slice() {
                ["up", "side", "none"] => return Ok(String::from("WireConnection")),
                ["none", "low", "tall"] => return Ok(String::from("WallConnection")),
                _ => {}
            },
            "west" => match values.as_slice() {
                ["up", "side", "none"] => return Ok(String::from("WireConnection")),
                ["none", "low", "tall"] => return Ok(String::from("WallConnection")),
                _ => {}
            },
            _ => {}
        }
        bail!(
            "missing aliasing strategy for `{}` with values `{:?}`",
            name,
            values
        )
    }

    pub fn codegen(values: &Vec<String>) -> String {
        let mut code = String::new();
        code.push_str(" {\n");
        for value in values {
            code.push('\t');
            code.push_str(&value.to_case(Case::Pascal));
            code.push_str(",\n");
        }
        code.push_str("}\n\n");
        code
    }

    pub fn get_enum_code(&self) -> String {
        let mut code = String::new();
        for (enum_name, enum_def) in self.code.iter() {
            code.push_str("#[repr(u8)]\n#[derive(Clone, Copy, Debug, PartialEq, Eq, strum_macros::EnumString)]\n#[strum(serialize_all = \"snake_case\")]\npub enum ");
            code.push_str(&enum_name.to_case(Case::Pascal));
            code.push_str(enum_def);
        }
        code
    }

    pub fn get_placement_method_returns(&self) -> &IndexMap<String, String> {
        &self.placement_method_returns
    }

    pub fn get_aliases(&self) -> &IndexMap<(String, Vec<String>), String> {
        &self.aliases
    }

    pub fn get_parameter_name(&self, name: &String, values: &[String]) -> String {
        if let Some(name) = self.aliases.get(&(name.clone(), values.to_owned())) {
            name.clone()
        } else {
            name.to_case(Case::Pascal)
        }
    }
}
