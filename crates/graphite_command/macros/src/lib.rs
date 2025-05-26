use std::{result, collections::HashMap};

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, ToTokens};
use syn::{
    braced, bracketed, parenthesized, parse::{Parse, ParseStream}, parse_macro_input, punctuated::Punctuated, spanned::Spanned, token, Generics, LitStr, PathArguments, ReturnType, Token, WhereClause
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

impl ToTokens for SimpleArg {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = &self.ident;
        let ty = &self.ty;

        tokens.extend(quote!(#ident: #ty));
    }
}

struct CommandSignature {
    pub ident: syn::Ident,
    pub generics: Generics,
    pub arguments: Punctuated<SimpleArg, Token![,]>,
    pub output: ReturnType,
    pub where_clause: Option<WhereClause>
}

impl Parse for CommandSignature {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _fn_token: Token![fn] = input.parse()?;
        let ident = input.parse()?;

        let generics = input.parse()?;

        let content;
        parenthesized!(content in input);

        Ok(Self {
            ident,
            generics,
            arguments: Punctuated::parse_terminated(&content)?,
            output: input.parse()?,
            where_clause: input.parse()?
        })
    }
}

impl ToTokens for CommandSignature {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = &self.ident;
        let generics = &self.generics;
        let arguments = &self.arguments;
        let output = &self.output;
        let where_clause = &self.where_clause;

        tokens.extend(quote!(fn #ident #generics (#arguments) #output #where_clause));
    }
}

#[derive(Debug)]
struct SuggestionArguments {
    pub name: syn::LitStr,
    pub function: syn::Path
}

impl Parse for SuggestionArguments {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        syn::parenthesized!(content in input);

        let name = content.parse()?;
        let _: Token![,] = content.parse()?;
        let function = content.parse()?;
        Ok(Self {
            name,
            function,
        })
    }
}

struct CommandFn {
    pub attrs: Vec<syn::Attribute>,
    pub sig: CommandSignature,
    pub block: syn::Block,
}

impl ToTokens for CommandFn {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let attrs = &self.attrs;
        let sig = &self.sig;
        let block = &self.block;
        tokens.extend(quote!(#(#attrs)* #sig #block));
    }
}

impl Parse for CommandFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            attrs: input.call(syn::Attribute::parse_outer)?,
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
                modifiers: Punctuated::parse_terminated(&content)?,
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
            let #id = graphite_command::minecraft::MinecraftRootDispatchNode {
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
            let #id = graphite_command::minecraft::MinecraftRootDispatchNode {
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

#[proc_macro_attribute]
pub fn brigadier(attr: TokenStream, item: TokenStream) -> TokenStream {
    let cloned_item = item.clone();
    let mut input = parse_macro_input!(cloned_item as CommandFn);
    let attributes = parse_macro_input!(attr as BrigadierAttributes);

    let id = input.sig.ident.clone();

    // let mut generic_player_types = vec![];
    let mut resolved_bounds = HashMap::new();

    for generic in &input.sig.generics.params {
        match generic {
            syn::GenericParam::Type(ty) => {
                resolved_bounds.insert(ty.ident.clone(), ty.bounds.clone());
            },
            _ => ()
        }
    }

    if let Some(where_clause) = &input.sig.generics.where_clause {
        add_from_where_clause(where_clause.clone(), &mut resolved_bounds);
    }

    if let Some(where_clause) = &input.sig.where_clause {
        add_from_where_clause(where_clause.clone(), &mut resolved_bounds);
    }

    let first_argument_ty = &input.sig.arguments[0].ty;

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

    // Find suggestions
    let mut suggestions: HashMap<String, SuggestionArguments> = HashMap::new();
    input.attrs.retain(|attribute| {
        let Some(ident) = attribute.path.get_ident() else {
            return true;
        };

        if ident.to_string() == "suggestions" {
            let args = attribute.tokens.clone();

            let args: SuggestionArguments = syn::parse2(args).unwrap();

            let name = args.name.value().clone();
            suggestions.insert(name, args);
    
            false
        } else {
            true
        }
    });

    // Create endpoint dispatch node
    let command_identifier_parse = format!("{}_brigadier_parse", id);
    let command_identifier_parse: proc_macro2::TokenStream =
        command_identifier_parse.parse().unwrap();
    let mut dispatch_node = quote!(
        graphite_command::minecraft::MinecraftDispatchNode {
            literals: std::collections::BTreeMap::new(),
            aliases: std::collections::BTreeMap::new(),
            parsers: vec![],
            executor: Some(#command_identifier_parse),
        }
    );

    let mut parse_function_data_args = quote!();
    let mut parse_function_data_args_deconstruct = quote!();

    #[derive(Debug)]
    struct TrailingOption {
        data_arg: proc_macro2::TokenStream,
        data_arg_deconstruct: proc_macro2::TokenStream
    }
    let mut trailing_options = vec![];

    let mut can_have_trailing_option = true;

    // Build actual dispatch node based on attribute arguments
    let mut attribute_literal_index = 0;
    let mut attribute_argument_index = 0;
    for attribute in attributes.attributes.iter().rev() {
        match attribute {
            BrigadierAttribute::Literal { aliases } => {
                can_have_trailing_option = false;
                attribute_literal_index += 1;
                let use_root = attribute_literal_index >= attribute_literal_count;

                let name = aliases[0].value();

                // Create alias map
                let mut aliases_tokens = if use_root {
                    let capacity = aliases.len() - 1;
                    quote!(let mut map = std::collections::HashMap::with_capacity(#capacity);)
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
                        let mut #id = graphite_command::minecraft::MinecraftRootDispatchNode {
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
                        graphite_command::minecraft::MinecraftDispatchNode {
                            literals: {
                                let mut map = std::collections::BTreeMap::new();
                                map.insert(#name, #dispatch_node);
                                map
                            },
                            aliases: {
                                #aliases_tokens
                                map
                            },
                            parsers: vec![],
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

                let parser_expr;

                let deconstruct_index =
                    proc_macro2::Literal::usize_unsuffixed(function_arg_index);


                let mut option_arg_data_arg = None;

                match ty {
                    syn::Type::Path(type_path) => {
                        let path_segments = &type_path.path.segments;
                        let last_segment = &path_segments[path_segments.len() - 1];
                        let mut ident_str = last_segment.ident.to_string();

                        if ident_str == "Option" {
                            if !can_have_trailing_option {
                                throw_error!(ty.span(), id => "Option is not supported in this position")
                            } else {
                                match &last_segment.arguments {
                                    PathArguments::AngleBracketed(args) => {
                                        match args.args.first().unwrap() {
                                            syn::GenericArgument::Type(generic_type) => {
                                                match generic_type {
                                                    syn::Type::Path(type_path) => {
                                                        let path_segments = &type_path.path.segments;
                                                        let last_segment = &path_segments[path_segments.len() - 1];
                                                        ident_str = last_segment.ident.to_string();
                                                    },
                                                    _ => throw_error!(ty.span(), id => "Missing type for Option"),
                                                }

                                                option_arg_data_arg = Some(quote!(#generic_type))
                                            },
                                            _ => throw_error!(ty.span(), id => "Missing type for Option"),
                                        }
                                    }
                                    _ => throw_error!(ty.span(), id => "Missing type for Option"),
                                };
                            }
                        } else {
                            can_have_trailing_option = false;
                        }

                        if option_arg_data_arg.is_none() {
                            parse_function_data_args = quote! (
                                #type_path,
                                #parse_function_data_args
                            );
                        }

                        match ident_str.as_str() {
                            "f32" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(f32), quote!(F32), modifiers));
                            }
                            "f364" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(f64), quote!(F64), modifiers));
                            }
                            "u8" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(u8), quote!(U8), modifiers));
                            }
                            "u16" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(u16), quote!(U16), modifiers))
                            }
                            "u32" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(u32), quote!(U32), modifiers))
                            }
                            "u64" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(u64), quote!(U64), modifiers))
                            }
                            "i8" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(i8), quote!(I8), modifiers));
                            }
                            "i16" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(i16), quote!(I16), modifiers))
                            }
                            "i32" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(i32), quote!(I32), modifiers))
                            }
                            "i64" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(i64), quote!(I64), modifiers))
                            }
                            "usize" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(usize), quote!(USize), modifiers))
                            }
                            "isize" => {
                                parser_expr = check_result!(type_path.span(), id =>
                                    process_num_arg(quote!(isize), quote!(ISize), modifiers))
                            }
                            "bool" => {
                                parser_expr = quote!(
                                    graphite_command::minecraft::MinecraftParser::Bool
                                );
                            }
                            _ => {
                                throw_error!(ty.span(), id => format!("type {} does not correspond to a known Brigadier argument", ident_str))
                            }
                        }
                    }
                    syn::Type::Reference(reference) => {
                        match reference.elem.as_ref() {
                            syn::Type::Path(type_path) => {
                                let path_segments = &type_path.path.segments;
                                let last_segment = &path_segments[path_segments.len() - 1];
                                let ident_str = last_segment.ident.to_string();
                                
                                can_have_trailing_option = false;

                                parse_function_data_args = quote! (
                                    &'static #type_path,
                                    #parse_function_data_args
                                );

                                match ident_str.as_str() {
                                    "str" => {
                                        parser_expr = quote!(
                                            graphite_command::minecraft::MinecraftParser::Word
                                        );
                                    }
                                    _ => {
                                        throw_error!(ty.span(), id => format!("type &{} does not correspond to a known Brigadier argument", ident_str))
                                    }
                                }
                            }
                            _ => {
                                throw_error!(ty.span(), id => format!("type does not correspond to a known Brigadier argument"))
                            }
                        }
                    }
                    _ => {
                        throw_error!(ty.span(), id => format!("type does not correspond to a known Brigadier argument"))
                    }
                }

                if let Some(option_arg_data_arg) = &option_arg_data_arg {
                    trailing_options.push(TrailingOption {
                        data_arg: option_arg_data_arg.clone(),
                        data_arg_deconstruct: quote!(data.#deconstruct_index),
                    })
                } else {
                    parse_function_data_args_deconstruct = quote! (
                        data.#deconstruct_index,
                        #parse_function_data_args_deconstruct
                    );
                }

                let name = function_arg_ident.to_string();
                let suggestion = suggestions.remove(&name);

                let (suggestion_type, suggestions) = if let Some(suggestion) = suggestion {
                    let function = suggestion.function;
                    (
                        quote!(Some(graphite_mc_protocol::types::SuggestionType::AskServer)),
                        quote!(Some(#function))
                    )
                } else {
                    (
                        quote!(None),
                        quote!(None)
                    )
                };

                let parser_node = quote!(
                    graphite_command::minecraft::MinecraftArgumentNode {
                        name: stringify!(#function_arg_ident),
                        parse: #parser_expr,
                        dispatch_node: Box::from(#dispatch_node),
                        suggestion_type: #suggestion_type,
                        suggestions: #suggestions
                    }
                );

                let executor = if option_arg_data_arg.is_some() {
                    let command_identifier_parse_optional = format!("{}_brigadier_parse_optional{}", id, trailing_options.len());
                    let command_identifier_parse_optional: proc_macro2::TokenStream =
                        command_identifier_parse_optional.parse().unwrap();
                    quote!(Some(#command_identifier_parse_optional))
                } else {
                    quote!(None)
                };

                dispatch_node = quote!(
                    graphite_command::minecraft::MinecraftDispatchNode {
                        literals: std::collections::BTreeMap::new(),
                        aliases: std::collections::BTreeMap::new(),
                        parsers: vec![#parser_node],
                        executor: #executor,
                    }
                )
            }
        }
    }

    for (name, args) in suggestions {
        throw_error!(args.name.span(), id => format!("no completable function argument with name '{}'", name));
    }

    // Create parse function (raw bytes => arguments => command function)

    let mut tokens = proc_macro2::TokenStream::new();
    // let item2: proc_macro2::TokenStream = item.into();
    input.to_tokens(&mut tokens);

    for i in 0..trailing_options.len()+1 {
        let identifier_parse = if i == 0 {
            command_identifier_parse.clone()
        } else {
            let command_identifier_parse_optional = format!("{}_brigadier_parse_optional{}", id, i);
            command_identifier_parse_optional.parse().unwrap()
        };

        let mut data_args = parse_function_data_args.clone();
        let mut deconstruct = parse_function_data_args_deconstruct.clone();

        for j in (0..trailing_options.len()).rev() {
            if j < i {
                deconstruct = quote!(
                    #deconstruct
                    None,
                )
            } else {
                let trailing_option = trailing_options.get(j).unwrap();
                let deconstruct_option = trailing_option.data_arg_deconstruct.clone();
                deconstruct = quote!(
                    #deconstruct
                    Some(#deconstruct_option),
                );

                let data_arg = trailing_option.data_arg.clone();
                data_args = quote!(
                    #data_args
                    #data_arg,
                );
            }
        }

        let parse_function = quote!(
            fn #identifier_parse(context: #first_argument_ty, data: &[u8], spans: &[graphite_command::types::Span]) -> graphite_command::types::CommandDispatchResult {
                #[repr(C)]
                struct Data(#data_args);

                assert_eq!(spans.len()+#i, #attribute_argument_count, "parse function should receive spans equal to argument count");
                assert_eq!(data.len(), std::mem::size_of::<Data>(), "slice length doesn't match data size. something must have gone wrong with realignment");
                let data: &Data = unsafe { &*(data as *const _ as *const Data) };
    
                graphite_command::types::CommandDispatchResult::Success(#id(context, #deconstruct))
            }
        );
        tokens.extend(parse_function);
    }

    tokens.extend(dispatch_node);
    tokens.into()
}

fn add_from_where_clause(where_clause: WhereClause, resolved_bounds: &mut HashMap<proc_macro2::Ident, Punctuated<syn::TypeParamBound, token::Add>>) {
    for where_predicate in where_clause.predicates {
        match where_predicate {
            syn::WherePredicate::Type(ty) => {
                match ty.bounded_ty {
                    syn::Type::Path(path) => {
                        if path.path.segments.len() == 1 {
                            let segment = &path.path.segments[0];
                            let ident = segment.ident.clone();
                            resolved_bounds.insert(ident, ty.bounds);
                        }
                    }
                    _ => (),
                }
            },
            _ => (),
        }
    }
}

fn process_num_arg(
    raw_typ: proc_macro2::TokenStream,
    parser_typ: proc_macro2::TokenStream,
    modifiers: &Punctuated<syn::Expr, token::Semi>,
) -> result::Result<proc_macro2::TokenStream, &'static str> {
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
    Ok(quote!(
        graphite_command::minecraft::MinecraftParser::#parser_typ {
            min: #min_expr,
            max: #max_expr
        }
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
