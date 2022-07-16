use std::str::FromStr;

use bytemuck::NoUninit;
use protocol::types::{CommandNodeParser, StringParserMode};
use thiserror::Error;

use crate::dispatcher::ParseState;

pub(crate) trait MinecraftParser {
    fn get_parse_func(&self) -> fn(&str, &mut ParseState) -> anyhow::Result<()>;
    fn get_brigadier_parser(&self) -> CommandNodeParser;
    fn is_equal(&self, other: Self) -> bool;
}

// Numeric parsers

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum NumericParser {
    U8,
    U16,
}

impl MinecraftParser for NumericParser {
    fn get_parse_func(&self) -> fn(&str, &mut ParseState) -> anyhow::Result<()> {
        match self {
            NumericParser::U8 => parse_from_string::<u8>,
            NumericParser::U16 => parse_from_string::<u16>,
        }
    }

    fn get_brigadier_parser(&self) -> CommandNodeParser {
        match self {
            NumericParser::U8 => CommandNodeParser::Integer {
                min: Some(u8::MIN as _),
                max: Some(u8::MAX as _),
            },
            NumericParser::U16 => CommandNodeParser::Integer {
                min: Some(u16::MIN as _),
                max: Some(u16::MAX as _),
            },
        }
    }

    fn is_equal(&self, other: Self) -> bool {
        *self == other
    }
}

#[derive(Debug, Error)]
#[error("failed to parse from string")]
struct ParseFromStringError;

fn parse_from_string<T: FromStr + NoUninit>(
    input: &str,
    state: &mut ParseState,
) -> anyhow::Result<()> {
    match input.parse::<T>() {
        Ok(parsed) => {
            state.push_arg(parsed);
            Ok(())
        }
        Err(_) => Err(ParseFromStringError.into()),
    }
}

// String parsers

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StringParser {
    Word,
}

impl MinecraftParser for StringParser {
    fn get_parse_func(&self) -> fn(&str, &mut ParseState) -> anyhow::Result<()> {
        match self {
            StringParser::Word => parse_word,
        }
    }

    fn get_brigadier_parser(&self) -> CommandNodeParser {
        match self {
            StringParser::Word => CommandNodeParser::String {
                mode: StringParserMode::SingleWord,
            },
        }
    }

    fn is_equal(&self, other: Self) -> bool {
        *self == other
    }
}

fn parse_word(input: &str, state: &mut ParseState) -> anyhow::Result<()> {
    state.push_str(input);
    Ok(())
}
