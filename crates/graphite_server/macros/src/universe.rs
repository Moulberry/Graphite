use proc_macro2::{TokenStream as TokenStream2, Span};
use quote::quote;
use syn::{ItemStruct, PathArguments::AngleBracketed};

pub(crate) fn derive_ticker_inner(item: ItemStruct) -> Result<TokenStream2, (Span, String)> {
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
            if crate::type_contains_ident(ty, "World") {
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
            let (last, args) = crate::get_last_ident(&path);

            if last == "World" {
                *method_tick = quote! {
                    #method_tick
                    self.#field_ident.tick();
                };
                *method_update_children_ptr = quote! {
                    #method_update_children_ptr
                    self.#field_ident.update_universe_ptr(universe);
                    self.#field_ident.initialize();
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
                            if crate::generic_arg_contains_ident(generic_arg.clone(), "World") {
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
                            if crate::generic_arg_contains_ident(generic_arg.clone(), "World") {
                                return Err((last.span(), "Map containing World<W> is unsupported, use WorldMap<W> instead".into()))
                            }
                        }
                    },
                    _ => ()
                }
            }
        },
        syn::Type::Tuple(_) => if crate::type_contains_ident(ty, "World") {
            return Err((field_ident.span(), "Tuple containing World<W> is unsupported".into()))
        },
        _ => todo!(),
    }
    return Ok(())
}