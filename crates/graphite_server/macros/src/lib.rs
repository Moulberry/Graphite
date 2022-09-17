use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, Span, Ident};
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, ItemStruct, PathArguments::AngleBracketed, Type};

fn type_contains_ident(ty: Type, ident: &str) -> bool {
    match ty {
        Type::Array(array) => {
            return type_contains_ident(*array.elem, ident);
        },
        Type::Group(group) => {
            return type_contains_ident(*group.elem, ident);
        },
        Type::Paren(paren) => {
            return type_contains_ident(*paren.elem, ident);
        },
        Type::Path(path) => {
            let (last, _)  = get_last_ident(&path);
            last == ident
        },
        Type::Tuple(tuple) => {
            for elem in tuple.elems {
                if type_contains_ident(elem, ident.clone()) {
                    return true;
                }
            }
            return false;
        },
        _ => false,
    }
}

fn generic_arg_contains_ident(generic_arg: syn::GenericArgument, ident: &str) -> bool {
    match generic_arg {
        syn::GenericArgument::Type(ty) => return type_contains_ident(ty, ident),
        _ => false
    }
}

fn derive_universe_ticker_inner(item: ItemStruct) -> Result<TokenStream2, (Span, String)> {
    let ident = item.ident;

    let mut method_tick = TokenStream2::new();
    let mut method_update_children_ptr = TokenStream2::new();

    for field in item.fields {
        let field_ident = field.ident.clone().expect("tuple structs are unsupported");
        parse_field(field.ty.clone(), field_ident, &mut method_tick, &mut method_update_children_ptr)?;
    }

    Ok(quote! {
        unsafe impl graphite_server::ticker::UniverseTicker<Self> for #ident {
            fn update_children_ptr(&mut self, universe: *mut Universe<Self>) {
                #method_update_children_ptr
            }
        
            fn tick(&mut self) {
                #method_tick
            }
        }
    }.into())
}

fn parse_field(ty: syn::Type, field_ident: syn::Ident, method_tick: &mut TokenStream2, method_update_children_ptr: &mut TokenStream2) -> Result<(), (Span, String)> {
    match ty.clone() {
        syn::Type::Array(_) => {
            if type_contains_ident(ty, "World") {
                return Err((field_ident.span(), "Array containing World<W> is unsupported".into()))
            }
        },
        syn::Type::Group(group) => {
            parse_field(*group.elem, field_ident, method_tick, method_update_children_ptr)?;
        },
        syn::Type::Paren(paren) => {
            parse_field(*paren.elem, field_ident, method_tick, method_update_children_ptr)?;
        },
        syn::Type::Path(path) => {
            let (last, args) = get_last_ident(&path);

            if last == "World" {
                *method_tick = quote! {
                    #method_tick
                    self.#field_ident.tick();
                };
                *method_update_children_ptr = quote! {
                    #method_update_children_ptr
                    self.#field_ident.update_universe_ptr(universe);
                    self.#field_ident.update_pointer();
                };
            // todo: add WorldVec
            } else if last == "WorldMap" {
                *method_tick = quote! {
                    #method_tick
                    self.#field_ident.tick();
                };
                *method_update_children_ptr = quote! {
                    #method_update_children_ptr
                    self.#field_ident.update_universe_ptr(universe);
                };
            } else if last == "Vec" {
                match args {
                    AngleBracketed(bracketed) => {
                        if bracketed.args.len() == 1 {
                            let generic_arg = &bracketed.args[0];
                            if generic_arg_contains_ident(generic_arg.clone(), "World") {
                                return Err((last.span(), "Vec containing World<W> is unsupported, use WorldVec<W> instead".into()))
                            }
                        }
                    },
                    _ => ()
                }
            } else if last == "HashMap" || last == "BTreeMap" || last == "StickyMap" {
                match args {
                    AngleBracketed(bracketed) => {
                        if bracketed.args.len() == 2 {
                            let generic_arg = &bracketed.args[1];
                            if generic_arg_contains_ident(generic_arg.clone(), "World") {
                                return Err((last.span(), "Map containing World<W> is unsupported, use WorldMap<W> instead".into()))
                            }
                        }
                    },
                    _ => ()
                }
            }
        },
        syn::Type::Tuple(_) => if type_contains_ident(ty, "World") {
            return Err((field_ident.span(), "Tuple containing World<W> is unsupported".into()))
        },
        _ => todo!(),
    }
    return Ok(())
}

fn get_last_ident(path: &syn::TypePath) -> (proc_macro2::Ident, syn::PathArguments) {
    let last = path.path.segments.last().unwrap();
    (last.ident.clone(), last.arguments.clone())
}

#[proc_macro_derive(UniverseTicker)]
pub fn derive_universe_ticker(item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as syn::ItemStruct);

    match derive_universe_ticker_inner(item) {
        Ok(output) => output.into(),
        Err((span, message)) => {
            quote_spanned!(span => compile_error!(#message);).into()
        },
    }
}

