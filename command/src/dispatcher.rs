use std::collections::{HashMap, BTreeMap};

use bytemuck::NoUninit;
use bytes::BufMut;

// Parse State

pub struct ParseState<'a> {
    current_alignment: usize,
    arguments: Vec<u8>,
    words: Vec<&'a str>,
    cursor: usize,
}

impl<'a> ParseState<'a> {
    fn new(input: &'a str) -> Self {
        let words = input
            .split(' ')
            .filter(|&x| !x.is_empty())
            .collect::<Vec<&str>>();

        Self {
            current_alignment: 0,
            arguments: Vec::new(),
            words,
            cursor: 0,
        }
    }

    pub(crate) fn peek<'b>(&'b self) -> Option<&'a str> {
        if self.is_finished() {
            None
        } else {
            Some(self.words[self.cursor])
        }
    }

    pub(crate) fn pop<'b>(&'b mut self) -> Option<&'a str> {
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

    pub(crate) fn push_str(&mut self, arg: &str) {
        let raw_slice: u128 = unsafe { std::mem::transmute(arg) };
        self.push_arg(raw_slice);
    }

    pub(crate) fn push_ref<T>(&mut self, arg: &T) {
        let raw_reference: u64 = unsafe { std::mem::transmute(arg) };
        self.push_arg(raw_reference);
    }

    pub(crate) fn push_arg<T: NoUninit>(&mut self, arg: T) {
        let alignment = std::mem::align_of_val(&arg);
        let bytes: &[u8] = bytemuck::bytes_of(&arg);
        self.push_bytes(bytes, alignment)
    }

    fn push_bytes(&mut self, bytes: &[u8], alignment: usize) {
        if alignment > self.current_alignment {
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
        }

        self.arguments.put_slice(&bytes);
    }
}

// Node implemenatations

pub struct RootDispatchNode {
    pub(crate) literals: HashMap<&'static str, DispatchNode>,
    pub(crate) aliases: HashMap<&'static str, &'static str>,
}

impl RootDispatchNode {
    pub fn dispatch(&self, input: &str) {
        let parse_state = ParseState::new(input);
        self.dispatch_parse_state(parse_state);
    }

    pub fn dispatch_with_context<T>(&self, input: &str, context: &T) {
        let mut parse_state = ParseState::new(input);
        parse_state.push_ref(context);
        self.dispatch_parse_state(parse_state);
    }

    fn dispatch_parse_state(&self, mut parse_state: ParseState) {
        if let Some(word) = parse_state.pop() {
            if let Some(aliased) = self.aliases.get(word) {
                // Aliased literal

                let literal = self
                    .literals
                    .get(*aliased)
                    .expect("literal must exist if it has an alias");

                if literal.dispatch(&mut parse_state) {
                    return;
                } else {
                    panic!("command failed to execute successfully");
                }
            } else {
                // Non-aliased

                let literal = self.literals.get(word);

                if let Some(literal) = literal {
                    if literal.dispatch(&mut parse_state) {
                        return;
                    } else {
                        panic!("command failed to execute successfully");
                    }
                } else {
                    panic!("unknown command!");
                }
            }
        } else {
            panic!("empty command");
        }
    }
}

pub(crate) struct DispatchNode {
    pub(crate) literals: BTreeMap<&'static str, DispatchNode>,
    pub(crate) aliases: BTreeMap<&'static str, &'static str>,
    pub(crate) parsers: Vec<ArgumentNode>,
    pub(crate) executor: Option<fn(&[u8])>,
}

impl DispatchNode {
    fn dispatch(&self, remaining: &mut ParseState) -> bool {
        if let Some(next_word) = remaining.pop() {
            // There is some input remaining

            if let Some(aliased) = self.aliases.get(next_word) {
                // Literal match via alias, dispatch to there
                let literal = self
                    .literals
                    .get(*aliased)
                    .expect("literal must exist if it has an alias");

                literal.dispatch(remaining)
            } else if let Some(literal) = self.literals.get(next_word)
            {
                // Literal match, dispatch to there
                literal.dispatch(remaining)
            } else {
                // No literal match, try to parse the input
                for arg in &self.parsers {
                    let prev_cursor = remaining.cursor;

                    let (parse_result, command_result) = arg.parse(next_word, remaining);
                    if parse_result {
                        return command_result;
                    }

                    // Parse failed, try next parser
                    // Also debug assert that the cursor didn't change
                    debug_assert!(
                        remaining.cursor == prev_cursor,
                        "cursor was updated by an argument node that failed"
                    );
                }
                false // No parsers accepted the input
            }
        } else {
            // There is no input remaining, see if this node is an executor

            if let Some(executor) = self.executor {
                // This node is an executor, lets execute!
                //let argument = remaining.arguments.as_slice() as *const _ as *const _;
                executor(remaining.arguments.as_slice());
                true
            } else {
                // Node isn't an executor, input *should* have had more remaining
                false
            }
        }
    }
}

// Argument node

pub(crate) struct ArgumentNode {
    pub(crate) parse: fn(&str, &mut ParseState) -> anyhow::Result<()>,
    pub(crate) dispatch_node: DispatchNode,
}

impl ArgumentNode {
    fn parse(&self, word: &str, remaining: &mut ParseState) -> (bool, bool) {
        // Try to parse a value
        let parse_result = (self.parse)(word, remaining);

        if parse_result.is_ok() {
            // Use the result on the parse node
            (true, self.dispatch_node.dispatch(remaining))
        } else {
            (false, false) // failed to parse
        }
    }
}
