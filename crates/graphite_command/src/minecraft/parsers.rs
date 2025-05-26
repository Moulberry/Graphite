use std::{fmt::Display, str::FromStr};

use bytemuck::NoUninit;
use graphite_mc_protocol::types::{CommandNodeParser, StringParserMode};
use thiserror::Error;

use crate::types::{CommandParseResult, ParseErrorType, ParseState, SpannedWord};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MinecraftParser {
    F32 { min: f32, max: f32 },
    F64 { min: f64, max: f64 },
    U8 { min: u8, max: u8 },
    U16 { min: u16, max: u16 },
    U32 { min: u32, max: u32 },
    U64 { min: u64, max: u64 },
    I8 { min: i8, max: i8 },
    I16 { min: i16, max: i16 },
    I32 { min: i32, max: i32 },
    I64 { min: i64, max: i64 },
    USize { min: usize, max: usize },
    ISize { min: isize, max: isize },
    Word,
    Bool
}

impl MinecraftParser {
    pub fn get_parse_func(&self) -> Box<dyn Fn(SpannedWord, &mut ParseState) -> CommandParseResult> {
        match *self {
            MinecraftParser::F32 { min, max } => Box::new(move |input, state| {
                parse_from_string::<f32>(input, state, min, max)
            }),
            MinecraftParser::F64 { min, max } => Box::new(move |input, state| {
                parse_from_string::<f64>(input, state, min, max)
            }),
            MinecraftParser::U8 { min, max } => Box::new(move |input, state| {
                parse_from_string::<u8>(input, state, min, max)
            }),
            MinecraftParser::U16 { min, max } => Box::new(move |input, state| {
                parse_from_string::<u16>(input, state, min, max)
            }),
            MinecraftParser::U32 { min, max } => Box::new(move |input, state| {
                parse_from_string::<u32>(input, state, min, max)
            }),
            MinecraftParser::U64 { min, max } => Box::new(move |input, state| {
                parse_from_string::<u64>(input, state, min, max.min(i64::MAX as u64))
            }),
            MinecraftParser::I8 { min, max } => Box::new(move |input, state| {
                parse_from_string::<i8>(input, state, min, max)
            }),
            MinecraftParser::I16 { min, max } => Box::new(move |input, state| {
                parse_from_string::<i16>(input, state, min, max)
            }),
            MinecraftParser::I32 { min, max } => Box::new(move |input, state| {
                parse_from_string::<i32>(input, state, min, max)
            }),
            MinecraftParser::I64 { min, max } => Box::new(move |input, state| {
                parse_from_string::<i64>(input, state, min, max)
            }),
            MinecraftParser::USize { min, max } => Box::new(move |input, state| {
                parse_from_string::<usize>(input, state, min, max.min(i64::MAX as usize))
            }),
            MinecraftParser::ISize { min, max } => Box::new(move |input, state| {
                parse_from_string::<isize>(input, state, min, max)
            }),
            MinecraftParser::Word => Box::new(parse_word),
            MinecraftParser::Bool => Box::new(parse_bool)
        }
    }

    pub fn get_brigadier_parser(&self) -> CommandNodeParser {
        match self {
            MinecraftParser::F32 { min, max } => CommandNodeParser::Float {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::F64 { min, max } => CommandNodeParser::Double {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::U8 { min, max } => CommandNodeParser::Integer {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::U16 { min, max } => CommandNodeParser::Integer {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::U32 { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::U64 { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::I8 { min, max } => CommandNodeParser::Integer {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::I16 { min, max } => CommandNodeParser::Integer {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::I32 { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::I64 { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::USize { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::ISize { min, max } => CommandNodeParser::Long {
                min: (*min).try_into().ok(),
                max: (*max).try_into().ok(),
            },
            MinecraftParser::Word => CommandNodeParser::String {
                mode: StringParserMode::SingleWord,
            },
            MinecraftParser::Bool => CommandNodeParser::Bool { },
        }
    }
}

#[derive(Debug, Error)]
#[error("failed to parse from string")]
pub struct ParseFromStringError;

fn parse_from_string<T: FromStr + Display + PartialOrd + NoUninit>(
    input: SpannedWord,
    state: &mut ParseState,
    min: T,
    max: T
) -> CommandParseResult {
    match input.word.parse::<T>() {
        Ok(parsed) => {
            if parsed < min {
                return CommandParseResult::Err {
                    span: input.span,
                    error: ParseErrorType::IntegerTooSmall {
                        min: format!("{}", min),
                        found: format!("{}", parsed)
                    },
                    continue_parsing: true
                }
            } else if parsed > max {
                return CommandParseResult::Err {
                    span: input.span,
                    error: ParseErrorType::IntegerTooBig {
                        max: format!("{}", max),
                        found: format!("{}", parsed)
                    },
                    continue_parsing: true
                }
            }
            state.push_arg(parsed, input.span);
            CommandParseResult::Ok
        }
        Err(_) => CommandParseResult::Err {
            span: input.span,
            error: ParseErrorType::ExpectedInteger,
            continue_parsing: true,
        },
    }
}

fn parse_word(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
    state.push_str(input.word, input.span);
    CommandParseResult::Ok
}

fn parse_bool(input: SpannedWord, state: &mut ParseState) -> CommandParseResult {
    match input.word.parse::<bool>() {
        Ok(parsed) => {
            state.push_arg(parsed, input.span);
            CommandParseResult::Ok
        },
        Err(_) => {
            CommandParseResult::Err {
                span: input.span,
                error: ParseErrorType::ExpectedBoolean,
                continue_parsing: true
            }
        },
    }
}
