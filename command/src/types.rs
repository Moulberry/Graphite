use std::{result, alloc::Layout};

use bytemuck::NoUninit;
use bytes::BufMut;

#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct SpannedWord<'a> {
    pub span: Span,
    pub word: &'a str
}

pub  struct ParseState<'a> {
    finalized: bool,
    argument_layout: Layout,
    arguments: Vec<u8>,
    argument_spans: Vec<Span>,
    pub(crate) words: Vec<SpannedWord<'a>>,
    pub(crate) cursor: usize,
    pub(crate) full_span: Span
}

impl<'a> ParseState<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        let mut words = Vec::new();
        let mut start = 0;
        let mut end = 0;
        let mut in_word = false;
        for (index, char) in input.chars().enumerate() {
            if char.is_whitespace() {
                if in_word {
                    words.push(
                        SpannedWord { span: Span { start, end }, word: &input[start..=end] }
                    )
                }
                in_word = false;
            } else {
                if !in_word {
                    in_word = true;
                    start = index;
                }
                end = index;
            }
        }
        if in_word {
            words.push(
                SpannedWord { span: Span { start, end }, word: &input[start..=end] }
            )
        }

        Self {
            finalized: false,
            argument_layout: unsafe { Layout::from_size_align_unchecked(0, 1) },
            arguments: Vec::new(),
            argument_spans: Vec::new(),
            words,
            cursor: 0,
            full_span: Span { start: 0, end }
        }
    }

    pub(crate) fn get_arguments(&mut self) -> (&[u8], &[Span]) {
        debug_assert!(!self.finalized);
        self.finalized = true;

        // Get size and align of layout
        let len = self.argument_layout.size();
        let align = self.argument_layout.align();
        debug_assert_eq!(len, self.arguments.len(), "layout length must match data length");

        // Compute padding (code from Layout::padding_needed_for)
        let padding = len.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1);
        let padding = padding.wrapping_sub(len);

        // Extend arguments with padding
        self.arguments.resize(len + padding, 0);

        (self.arguments.as_slice(), self.argument_spans.as_slice())
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn peek_input<'b>(&'b self) -> Option<SpannedWord<'a>> {
        if self.is_finished() {
            None
        } else {
            Some(self.words[self.cursor])
        }
    }

    pub(crate) fn pop_input<'b>(&'b mut self) -> Option<SpannedWord<'a>> {
        if self.is_finished() {
            None
        } else {
            self.cursor += 1;
            Some(self.words[self.cursor - 1])
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.cursor >= self.words.len()
    }

    pub(crate) fn advance(&mut self, advance: usize) {
        self.cursor += advance;
        debug_assert!(self.cursor <= self.words.len());
    }

    pub(crate) fn push_str(&mut self, arg: &str, span: Span) {
        let raw_slice: u128 = unsafe { std::mem::transmute(arg) };
        self.push_arg(raw_slice, span);
    }

    pub(crate) fn push_ref<T>(&mut self, arg: &T, span: Span) {
        let raw_reference: usize = unsafe { std::mem::transmute(arg) };
        self.push_arg(raw_reference, span);
    }

    pub(crate) fn push_arg<T: NoUninit>(&mut self, arg: T, span: Span) {
        // Get layout for argument
        let arg_layout = Layout::new::<T>();

        // Update layout
        let (new_layout, offset) = self.argument_layout.extend(arg_layout).unwrap();
        self.argument_layout = new_layout;
        self.arguments.resize(offset, 0);

        // Put bytes and span
        let bytes: &[u8] = bytemuck::bytes_of(&arg);
        debug_assert_eq!(arg_layout.size(), bytes.len());
        self.arguments.put_slice(bytes);
        self.argument_spans.push(span);
    }
}

pub type DispatchFunction = fn(&[u8], &[Span]) -> CommandDispatchResult;

pub type CommandResult = result::Result<(), String>;

#[derive(Debug)]
pub enum CommandParseResult {
    Ok,
    Err {
        span: Span,
        errmsg: String,
        continue_parsing: bool
    },
}

#[derive(Debug)]
pub enum CommandDispatchResult {
    Success(CommandResult),
    ParseError {
        span: Span,
        errmsg: String,
        continue_parsing: bool
    },
    UnknownCommand,
    IncompleteCommand,
    TooManyArguments
}