use std::str::FromStr;

use bytemuck::NoUninit;
use graphite_mc_protocol::types::{CommandNodeParser, StringParserMode};
use thiserror::Error;

use crate::types::{CommandParseResult, ParseState, SpannedWord};

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
    U64 { min: u64, max: u64 },
    USize { min: usize, max: usize },
    ISize { min: isize, max: isize },
}

impl MinecraftParser for NumericParser {
    fn get_parse_func(&self) -> fn(SpannedWord, &mut ParseState) -> CommandParseResult {
        match self {
            NumericParser::U8 { min: _, max: _ } => parse_from_string::<u8>,
            NumericParser::U16 { min: _, max: _ } => parse_from_string::<u16>,
            NumericParser::U64 { min: _, max: _ } => parse_from_string::<u64>,
            NumericParser::USize { min: _, max: _ } => parse_from_string::<usize>,
            NumericParser::ISize { min: _, max: _ } => parse_from_string::<isize>,
        }
    }

    fn get_brigadier_parser(&self) -> CommandNodeParser {
        match self {
            NumericParser::U8 { min, max } => CommandNodeParser::Integer {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            NumericParser::U16 { min, max } => CommandNodeParser::Integer {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            NumericParser::U64 { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            NumericParser::USize { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            NumericParser::ISize { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
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
    state: &mut ParseState,
) -> CommandParseResult {
    match input.word.parse::<T>() {
        Ok(parsed) => {
            state.push_arg(parsed, input.span);
            CommandParseResult::Ok
        }
        Err(_) => CommandParseResult::Err {
            span: input.span,
            errmsg: "failed to parse from string".into(),
            continue_parsing: true,
        },
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
