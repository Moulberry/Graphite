use std::result;

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
    pub(crate) current_alignment: usize,
    pub(crate) arguments: Vec<u8>,
    pub(crate) argument_spans: Vec<Span>,
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
            current_alignment: 0,
            arguments: Vec::new(),
            argument_spans: Vec::new(),
            words,
            cursor: 0,
            full_span: Span { start: 0, end }
        }
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
        let raw_reference: u64 = unsafe { std::mem::transmute(arg) };
        self.push_arg(raw_reference, span);
    }

    pub(crate) fn push_arg<T: NoUninit>(&mut self, arg: T, span: Span) {
        let alignment = std::mem::align_of_val(&arg);
        let bytes: &[u8] = bytemuck::bytes_of(&arg);
        self.argument_spans.push(span);
        self.push_bytes(bytes, alignment)
    }

    fn push_bytes(&mut self, bytes: &[u8], alignment: usize) {
        match alignment.cmp(&self.current_alignment) {
            std::cmp::Ordering::Greater => {
                if self.current_alignment > 0 {
                    // todo: document this better
                    // realign
                    debug_assert!(
                        self.arguments.len() % self.current_alignment == 0,
                        "arguments are not aligned?"
                    );
    
                    let resize_factor = alignment / self.current_alignment;
    
                    // Resize to new alignment
                    self.arguments
                        .resize(self.arguments.len() * resize_factor, 0);
    
                    let argument_count = self.arguments.len() / self.current_alignment;
                    for i in (0..argument_count).rev() {
                        let data = &self.arguments
                            [i * self.current_alignment..(i + 1) * self.current_alignment];
    
                        unsafe {
                            let src = data.as_ptr();
                            let dst = self.arguments.as_mut_ptr().add(i * alignment);
                            std::ptr::copy_nonoverlapping(src, dst, self.current_alignment);
                        }
                    }
                }
                self.current_alignment = alignment;
                self.arguments.put_slice(bytes);
            },
            std::cmp::Ordering::Less => {
                self.arguments.put_slice(bytes);
                self.arguments.resize(self.arguments.len() + self.current_alignment - alignment, 0);
            },
            std::cmp::Ordering::Equal => {
                self.arguments.put_slice(bytes);
            },
        }
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