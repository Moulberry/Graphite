use std::result;

use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token, LitStr, Token,
};
use thiserror::Error;

struct SimpleArg {
    pub ident: syn::Ident,
    pub ty: syn::Type,
}

impl Parse for SimpleArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident = input.parse()?;
        let _colon_token: Token![:] = input.parse()?;
        let ty = input.parse()?;

        Ok(Self { ident, ty })
    }
}

struct CommandSignature {
    pub ident: syn::Ident,
    pub arguments: Punctuated<SimpleArg, Token![,]>,
}

impl Parse for CommandSignature {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _fn_token: Token![fn] = input.parse()?;
        let ident = input.parse()?;

        let content;
        parenthesized!(content in input);

        let arguments = Punctuated::parse_terminated(&content)?;

        Ok(Self { ident, arguments })
    }
}

struct CommandFn {
    pub sig: CommandSignature,
    pub block: syn::Block,
}

#[derive(Debug, Error)]
#[error("you did an oopsie")]
struct MyError;

impl Parse for CommandFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            sig: input.parse()?,
            block: input.parse()?,
        })
    }
}

#[derive(Debug)]
enum BrigadierAttribute {
    Literal {
        aliases: Vec<LitStr>,
    },
    Argument {
        span: Span,
        modifiers: Punctuated<syn::Expr, Token![:]>,
    },
}

impl Parse for BrigadierAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let span = input.span();
        if input.peek(token::Brace) {
            let content;
            braced!(content in input);

            Ok(Self::Argument {
                span,
                modifiers: Punctuated::parse_terminated(&content)?, // todo: be more specific than syn::Expr
            })
        } else {
            let string: LitStr = input.parse()?;
            Ok(Self::Literal {
                aliases: vec![string],
            })
        }
    }
}

struct BrigadierAttributes {
    pub attributes: syn::punctuated::Punctuated<BrigadierAttribute, Token![,]>,
}

impl Parse for BrigadierAttributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            attributes: Punctuated::parse_terminated(input)?,
        })
    }
}

macro_rules! check_result {
    ($span:expr, $command_identifier:expr => $($arg:tt)*) => {
        match $($arg)* {
            Ok(v) => v,
            Err(err) => {
                throw_error!($span, $command_identifier => err.to_string());
            }
        }
    };
}

macro_rules! throw_error {
    ($span:expr, $command_identifier:expr => $($arg:tt)*) => {
        // Create a dummy root to suppress unrelated errors
        // about the root not existing
        let id = $command_identifier;
        let dummy_root = quote::quote!(
            let #id = minecraft::MinecraftRootDispatchNode {
                literals: HashMap::new(),
                aliases: HashMap::new()
            };
        );

        // Create compile error with span
        let msg = format!("brigadier: {}", $($arg)*);
        let error = quote::quote_spanned!($span => compile_error!(#msg););

        let mut tokens = TokenStream::new();
        tokens.extend::<TokenStream>(error.into());
        tokens.extend::<TokenStream>(dummy_root.into());
        println!("{:?}", tokens);
        return tokens;
    };
}

#[proc_macro_attribute]
pub fn brigadier(attr: TokenStream, item: TokenStream) -> TokenStream {
    println!("attr: \"{}\"", attr.to_string());
    println!("item: \"{}\"", item.to_string());

    let cloned_item = item.clone();
    let input = parse_macro_input!(cloned_item as CommandFn);
    let command_identifier = input.sig.ident;

    let function_argument_count = input.sig.arguments.len();

    let attributes = parse_macro_input!(attr as BrigadierAttributes);

    // Validate attributes
    let mut attribute_argument_count = 0;
    for attribute in &attributes.attributes {
        match attribute {
            BrigadierAttribute::Literal { aliases } => {
                for litstr in aliases {
                    let value = litstr.value();
                    check_result!(litstr.span(), command_identifier => check_literal(&value));
                }
            },
            BrigadierAttribute::Argument { span: _, modifiers: _ } => {
                attribute_argument_count += 1
            }
        }
    }

    // Error message if attribute argument count != function argument count
    if attribute_argument_count != function_argument_count {
        let attribute_count_description = if attribute_argument_count == 1 {
            "1 attribute argument".into()
        } else {
            format!("{} attribute arguments", attribute_argument_count)
        };

        let function_count_description = if function_argument_count == 1 {
            "is 1 function argument".into()
        } else {
            format!("are {} function arguments", function_argument_count)
        };

        let error_msg = format!("{}, but there {}",attribute_count_description, function_count_description);

        if attribute_argument_count > function_argument_count {
            // Find attribute span to put error message on

            let mut index = 0;
            for attribute in attributes.attributes {
                match attribute {
                    BrigadierAttribute::Literal { aliases: _ } => (),
                    BrigadierAttribute::Argument { span, modifiers: _ } => {
                        index += 1;
                        if index > function_argument_count {
                            throw_error!(span, command_identifier => error_msg);
                        }
                    }
                }
            }

            unreachable!()
        } else {
            // Put error message on function parameter span
            let farg = &input.sig.arguments[attribute_argument_count];
            throw_error!(farg.ident.span(), command_identifier => error_msg);
        }
    }

    let command_identifier_parse = format!("{}__parse", command_identifier.to_string());
    let command_identifier_parse: proc_macro2::TokenStream = command_identifier_parse.parse().unwrap();

    // Create endpoint dispatch node
    let dispatch_node = quote::quote!(
        minecraft::MinecraftDispatchNode {
            literals: BTreeMap::new(),
            aliases: BTreeMap::new(),
            numeric_parser: None,
            string_parser: None,
            executor: Some(#command_identifier_parse),
        }
    );

    // Build actual dispatch node based on attribute arguments
    for attribute in attributes.attributes {
        match attribute {
            BrigadierAttribute::Literal { aliases } => {
                
            },
            BrigadierAttribute::Argument { span: _, modifiers: _ } => {
                
            }
        }
    }

    // Create parse function (raw bytes => arguments => command function)
    let parse_function = quote::quote!(
        fn #command_identifier_parse(data: &[u8]) {
            #[repr(C)]
            struct Data(u8);
    
            debug_assert_eq!(data.len(), std::mem::size_of::<Data>());
            let data: &Data = unsafe { std::mem::transmute(data as *const _ as *const ()) };
    
            #command_identifier(data.0);
        }
    );

    let dispatch_node = quote::quote!(
        let #command_identifier = minecraft::MinecraftRootDispatchNode {
            literals: {
                let mut map = HashMap::with_capacity(1);
                map.insert("hello", minecraft::MinecraftDispatchNode {
                    literals: BTreeMap::new(),
                    aliases: BTreeMap::new(),
                    numeric_parser: Some(MinecraftArgumentNode {
                        name: "number",
                        parse: minecraft::NumericParser::U8,
                        dispatch_node: Box::from(#dispatch_node),
                    }),
                    string_parser: None,
                    executor: None,
                });
                map
            },
            aliases: HashMap::new()
        };
    );

    let mut tokens = TokenStream::new();
    tokens.extend::<TokenStream>(item);
    tokens.extend::<TokenStream>(parse_function.into());
    tokens.extend::<TokenStream>(dispatch_node.into());
    tokens
}

fn check_literal(literal: &str) -> result::Result<(), &str> {
    for char in literal.chars() {
        if char == ' ' {
            return Err("literal cannot contain a space")
        }
    }
    Ok(())
}