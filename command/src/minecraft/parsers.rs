use std::str::FromStr;

use bytemuck::NoUninit;
use protocol::types::{CommandNodeParser, StringParserMode};
use thiserror::Error;

use crate::types::{ParseState, SpannedWord, CommandParseResult};

pub trait MinecraftParser {
    fn get_parse_func(&self) -> fn(SpannedWord, &mut ParseState) -> CommandParseResult;
    fn get_brigadier_parser(&self) -> CommandNodeParser;
    fn is_equal(&self, other: Self) -> bool;
}

// Numeric parsers

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum NumericParser {
    U8 { min: u8, max: u8 },
    U16 { min: u16, max: u16 },
}

impl MinecraftParser for NumericParser {
    fn get_parse_func(&self) -> fn(SpannedWord, &mut ParseState) -> CommandParseResult {
        match self {
            NumericParser::U8 { min: _, max: _} => parse_from_string::<u8>,
            NumericParser::U16 { min: _, max: _} => parse_from_string::<u16>,
        }
    }

    fn get_brigadier_parser(&self) -> CommandNodeParser {
        match self {
            NumericParser::U8 { min, max} => CommandNodeParser::Integer {
                min: Some(*min as _),
                max: Some(*max as _),
            },
            NumericParser::U16 { min, max} => CommandNodeParser::Integer {
                min: Some(*min as _),
                max: Some(*max as _),
            },
        }
    }

    fn is_equal(&self, other: Self) -> bool {
        *self == other
    }
}

#[derive(Debug, Error)]
#[error("failed to parse from string")]
pub struct ParseFromStringError;

fn parse_from_string<T: FromStr + Ord + NoUninit>(
    input: SpannedWord,
    state: &mut ParseState
) -> CommandParseResult {
    match input.word.parse::<T>() {
        Ok(parsed) => {
            state.push_arg(parsed, input.span);
            CommandParseResult::Ok
        },
        Err(_) => CommandParseResult::Err { span: input.span, errmsg: "failed to parse from string".into(), continue_parsing: true }
    }
}

// String parsers

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StringParser {
    Word,
}

impl MinecraftParser for StringParser {
    fn get_parse_func(&self) -> fn(SpannedWord, &mut ParseState) -> CommandParseResult {
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

fn parse_word(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
    state.push_str(input.word, input.span);
    CommandParseResult::Ok
}
