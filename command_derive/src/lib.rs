use std::result;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, ToTokens};
use syn::{
    braced, bracketed, parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    spanned::Spanned,
    token, LitStr, ReturnType, Token,
};

// Parse types for #[brigadier]

#[derive(Debug)]
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
    pub output: ReturnType,
}

impl Parse for CommandSignature {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _fn_token: Token![fn] = input.parse()?;
        let ident = input.parse()?;

        let content;
        parenthesized!(content in input);

        Ok(Self {
            ident,
            arguments: Punctuated::parse_terminated(&content)?,
            output: input.parse()?,
        })
    }
}

struct CommandFn {
    pub sig: CommandSignature,
    pub _block: syn::Block,
}

impl Parse for CommandFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            sig: input.parse()?,
            _block: input.parse()?,
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
        modifiers: Punctuated<syn::Expr, Token![;]>,
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
            let aliases = if input.peek(token::Bracket) {
                let content;
                bracketed!(content in input);

                // Parse punctuated sequence of LitStr
                let punctuated: Punctuated<LitStr, Token![,]> =
                    Punctuated::parse_terminated(&content)?;

                // Create alias vec
                let mut aliases = Vec::with_capacity(punctuated.len());
                for alias in punctuated {
                    aliases.push(alias);
                }

                aliases
            } else {
                // Single LitStr, create mono vec
                let string: LitStr = input.parse()?;
                vec![string]
            };

            Ok(Self::Literal { aliases })
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

// Macros for error reporting

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
    ($span:expr, $command_identifier:expr => $($arg:tt)*) => {{
        // Create a dummy root to suppress unrelated errors
        // about the root not existing
        let id = $command_identifier;
        let dummy_root = quote::quote!(
            let #id = command::minecraft::MinecraftRootDispatchNode {
                literals: std::collections::HashMap::new(),
                aliases: std::collections::HashMap::new()
            };
        );

        // Create compile error with span
        let msg = format!("brigadier: {}", $($arg)*);
        let error = quote::quote_spanned!($span => compile_error!(#msg););

        // Emit tokens
        let mut tokens = TokenStream::new();
        tokens.extend::<TokenStream>(error.into());
        tokens.extend::<TokenStream>(dummy_root.into());
        return tokens;
    }};
    ($command_identifier:expr => $($arg:tt)*) => {{
        // Create a dummy root to suppress unrelated errors
        // about the root not existing
        let id = $command_identifier;
        let dummy_root = quote::quote!(
            let #id = command::minecraft::MinecraftRootDispatchNode {
                literals: std::collections::HashMap::new(),
                aliases: std::collections::HashMap::new()
            };
        );

        // Create compile error with span
        let msg = format!("brigadier: {}", $($arg)*);
        let error = quote::quote!(compile_error!(#msg););

        // Emit tokens
        let mut tokens = TokenStream::new();
        tokens.extend::<TokenStream>(error.into());
        tokens.extend::<TokenStream>(dummy_root.into());
        return tokens;
    }};
}

fn check_player_type_and_get_generic(type_path: &syn::TypePath) -> Option<syn::Type> {
    let segments = &type_path.path.segments;
    let last = &segments[segments.len() - 1];
    if last.ident == "Player" {
        let arguments = &last.arguments;
        match arguments {
            syn::PathArguments::AngleBracketed(generic_args) => {
                let generic_args = &generic_args.args;
                if generic_args.len() == 1 {
                    let generic_arg = &generic_args[0];
                    match generic_arg {
                        syn::GenericArgument::Type(generic_type) => {
                            return Some(generic_type.clone());
                        }
                        _ => return None,
                    }
                }
            }
            _ => return None,
        }
    }
    None
}

#[proc_macro_attribute]
pub fn brigadier(attr: TokenStream, item: TokenStream) -> TokenStream {
    let cloned_item = item.clone();
    let input = parse_macro_input!(cloned_item as CommandFn);
    let attributes = parse_macro_input!(attr as BrigadierAttributes);

    let id = input.sig.ident;

    let mut generic_player_types = vec![];

    // Validate first argument
    let has_correct_first_argument = if input.sig.arguments.is_empty() {
        false
    } else {
        let first_argument_ty = &input.sig.arguments[0].ty;

        match first_argument_ty {
            syn::Type::Reference(ref_ty) => match &*ref_ty.elem {
                syn::Type::Path(type_path) => {
                    if let Some(generic_type) = check_player_type_and_get_generic(type_path) {
                        generic_player_types.push(generic_type);
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            },
            _ => false,
        }
    };
    if !has_correct_first_argument {
        throw_error!(id => "first argument of command function must be of type `&mut Player<?>`");
    }

    let function_argument_count = input.sig.arguments.len() - 1;

    // Validate command signature
    if matches!(input.sig.output, ReturnType::Default) {
        throw_error!(id => "command function must have return type of `CommandResult`");
    }

    // Validate attributes
    let mut attribute_literal_count = 0;
    let mut attribute_argument_count = 0;
    for attribute in &attributes.attributes {
        match attribute {
            BrigadierAttribute::Literal { aliases } => {
                attribute_literal_count += 1;
                for litstr in aliases {
                    let value = litstr.value();
                    check_result!(litstr.span(), id => check_literal(&value));
                }
            }
            BrigadierAttribute::Argument {
                span: _,
                modifiers: _,
            } => {
                attribute_argument_count += 1;
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

        let error_msg = format!(
            "{}, but there {}",
            attribute_count_description, function_count_description
        );

        if attribute_argument_count > function_argument_count {
            // Find attribute span to put error message on

            let mut index = 0;
            for attribute in attributes.attributes {
                match attribute {
                    BrigadierAttribute::Literal { aliases: _ } => (),
                    BrigadierAttribute::Argument { span, modifiers: _ } => {
                        index += 1;
                        if index > function_argument_count {
                            throw_error!(span, id => error_msg);
                        }
                    }
                }
            }

            unreachable!()
        } else {
            // Put error message on function parameter span
            let farg = &input.sig.arguments[attribute_argument_count + 1];
            throw_error!(farg.ident.span(), id => error_msg);
        }
    }

    // Create endpoint dispatch node
    let command_identifier_parse = format!("{}__parse", id);
    let command_identifier_parse: proc_macro2::TokenStream =
        command_identifier_parse.parse().unwrap();
    let mut dispatch_node = quote!(
        command::minecraft::MinecraftDispatchNode {
            literals: std::collections::BTreeMap::new(),
            aliases: std::collections::BTreeMap::new(),
            numeric_parser: None,
            string_parser: None,
            executor: Some(#command_identifier_parse),
        }
    );

    let mut parse_function_data_args = quote!();
    let mut parse_function_data_args_deconstruct = quote!();
    let mut parse_function_validate_args = quote!();

    // Build actual dispatch node based on attribute arguments
    let mut attribute_literal_index = 0;
    let mut attribute_argument_index = 0;
    for attribute in attributes.attributes.iter().rev() {
        match attribute {
            BrigadierAttribute::Literal { aliases } => {
                attribute_literal_index += 1;
                let use_root = attribute_literal_index >= attribute_literal_count;

                let name = aliases[0].value();

                // Create alias map
                let mut aliases_tokens = if use_root {
                    let capacity = aliases.len() - 1;
                    quote!(let mut map = std::collections::HashMap::with_capacity(#capacity);)
                    //quote!(let mut map = std::collections::HashMap::with_capacity_and_hasher(#capacity, ahash::AHasher::default());)
                } else {
                    quote!(let mut map = std::collections::BTreeMap::new();)
                };

                if aliases.len() > 1 {
                    for alias in &aliases[1..] {
                        let alias_value = alias.value();
                        aliases_tokens = quote!(
                            #aliases_tokens
                            map.insert(#alias_value, #name);
                        )
                    }
                }

                if use_root {
                    // Update the dispatch node, using a MinecraftRootDispatchNode
                    dispatch_node = quote!(
                        let mut #id = command::minecraft::MinecraftRootDispatchNode {
                            literals: {
                                let mut map = std::collections::HashMap::with_capacity(1);
                                map.insert(#name, #dispatch_node);
                                map
                            },
                            aliases: {
                                #aliases_tokens
                                map
                            }
                        };
                    );
                } else {
                    // Update the dispatch node, using a MinecraftDispatchNode
                    dispatch_node = quote!(
                        command::minecraft::MinecraftDispatchNode {
                            literals: {
                                let mut map = std::collections::BTreeMap::new();
                                map.insert(#name, #dispatch_node);
                                map
                            },
                            aliases: {
                                #aliases_tokens
                                map
                            },
                            numeric_parser: None,
                            string_parser: None,
                            executor: None,
                        }
                    );
                }
            }
            BrigadierAttribute::Argument { span: _, modifiers } => {
                attribute_argument_index += 1;
                let function_arg_index = attribute_argument_count - attribute_argument_index;
                let function_arg = &input.sig.arguments[function_arg_index + 1];
                let function_arg_ident = &function_arg.ident;
                let ty = &function_arg.ty;

                let parser_num;
                let parser_expr;
                let parser_validate;

                let deconstruct_index =
                    proc_macro2::Literal::usize_unsuffixed(function_arg_index + 2);

                match ty {
                    syn::Type::Path(type_path) => {
                        let path_segments = &type_path.path.segments;
                        let last_segment_ident = &path_segments[path_segments.len() - 1].ident;
                        let ident_str: &str = &last_segment_ident.to_string();

                        parse_function_data_args = quote! (
                            #type_path,
                            #parse_function_data_args
                        );

                        match ident_str {
                            "u8" => {
                                (parser_num, parser_expr, parser_validate) = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(u8), quote!(U8), deconstruct_index.clone(), modifiers));
                            }
                            "u16" => {
                                (parser_num, parser_expr, parser_validate) = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(u16), quote!(U16), deconstruct_index.clone(), modifiers))
                            }
                            "u64" => {
                                (parser_num, parser_expr, parser_validate) = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(u64), quote!(U64), deconstruct_index.clone(), modifiers))
                            }
                            _ => {
                                throw_error!(ty.span(), id => "type does not correspond to a known Brigadier argument")
                            }
                        }
                    }
                    _ => {
                        throw_error!(ty.span(), id => "type does not correspond to a known Brigadier argument")
                    }
                }

                parse_function_data_args_deconstruct = quote! (
                    data.#deconstruct_index,
                    #parse_function_data_args_deconstruct
                );
                parse_function_validate_args.extend(parser_validate);

                let parser_node = quote!(
                    Some(command::minecraft::MinecraftArgumentNode {
                        name: stringify!(#function_arg_ident),
                        parse: #parser_expr,
                        dispatch_node: Box::from(#dispatch_node),
                    })
                );

                if parser_num {
                    dispatch_node = quote!(
                        command::minecraft::MinecraftDispatchNode {
                            literals: std::collections::BTreeMap::new(),
                            aliases: std::collections::BTreeMap::new(),
                            numeric_parser: #parser_node,
                            string_parser: None,
                            executor: None,
                        }
                    )
                } else {
                    dispatch_node = quote!(
                        command::minecraft::MinecraftDispatchNode {
                            literals: std::collections::BTreeMap::new(),
                            aliases: std::collections::BTreeMap::new(),
                            numeric_parser: None,
                            string_parser: #parser_node,
                            executor: None,
                        }
                    )
                }
            }
        }
    }

    let mut player_type_match_variants = quote!();
    let mut player_type_match_inner = quote!();

    for (index, player_type) in generic_player_types.iter().enumerate() {
        let player_ty_identifier = format!("player_type_{}", index);
        let player_ty_identifier: proc_macro2::TokenStream = player_ty_identifier.parse().unwrap();

        player_type_match_variants = quote!(
            #player_type_match_variants
            let #player_ty_identifier = std::any::TypeId::of::<#player_type>();
        );

        player_type_match_inner = quote!(
            #player_type_match_inner
            #player_ty_identifier => {
                command::types::CommandDispatchResult::Success(#id(unsafe { &mut *(std::mem::transmute::<*mut (), *mut Player<#player_type>>(data.0) )}, #parse_function_data_args_deconstruct))
            }
        );
    }

    // Create parse function (raw bytes => arguments => command function)
    let parse_function = quote!(
        fn #command_identifier_parse(data: &[u8], spans: &[command::types::Span]) -> command::types::CommandDispatchResult {
            #[repr(C)]
            struct Data(*mut (), std::any::TypeId, #parse_function_data_args);

            debug_assert_eq!(spans.len() - 2, #attribute_argument_count, "parse function should receive spans equal to argument count");
            debug_assert_eq!(data.len(), std::mem::size_of::<Data>(), "slice length doesn't match data size. something must have gone wrong with realignment");
            let data: &Data = unsafe { &*(data as *const _ as *const Data) };

            #parse_function_validate_args

            #player_type_match_variants
            match data.1 {
                #player_type_match_inner
                _ => {
                    panic!("Unknown player type executed command `{}`", stringify!(#id));
                }
            }
        }
    );

    let mut tokens = TokenStream::new();
    tokens.extend::<TokenStream>(item);
    tokens.extend::<TokenStream>(parse_function.into());
    tokens.extend::<TokenStream>(dispatch_node.into());
    tokens
}

fn process_num_arg(
    raw_typ: proc_macro2::TokenStream,
    parser_typ: proc_macro2::TokenStream,
    deconstruct_index: proc_macro2::Literal,
    modifiers: &Punctuated<syn::Expr, token::Semi>,
) -> result::Result<(bool, proc_macro2::TokenStream, proc_macro2::TokenStream), &'static str> {
    let mut min_expr = quote!(#raw_typ::MIN);
    let mut max_expr = quote!(#raw_typ::MAX);
    for modifier in modifiers {
        match modifier {
            syn::Expr::Range(range) => {
                if let Some(from) = &range.from {
                    min_expr = from.to_token_stream();
                }
                if let Some(to) = &range.to {
                    max_expr = to.to_token_stream();
                }
            }
            _ => return Err("invalid modifier for integer"),
        }
    }
    Ok((
        true,
        quote!(
            command::minecraft::NumericParser::#parser_typ {
                min: #min_expr,
                max: #max_expr
            }
        ),
        quote!(
            if data.#deconstruct_index < #min_expr {
                return command::types::CommandDispatchResult::ParseError {
                    span: spans[#deconstruct_index],
                    errmsg: "argument was less than min".into(),
                    continue_parsing: false
                };
            } else if data.#deconstruct_index > #max_expr {
                return command::types::CommandDispatchResult::ParseError {
                    span: spans[#deconstruct_index],
                    errmsg: "argument was greater than max".into(),
                    continue_parsing: false
                };
            }
        ),
    ))
}

fn check_literal(literal: &str) -> result::Result<(), &'static str> {
    for char in literal.chars() {
        if char == ' ' {
            return Err("literal cannot contain a space");
        }
    }
    Ok(())
}
