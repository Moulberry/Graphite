use std::{collections::HashSet, result};

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::{quote, quote_spanned, ToTokens};
use syn::{
    braced, parenthesized, parse::Parse, parse_macro_input, punctuated::Punctuated,
    spanned::Spanned, Attribute, Token,
};

#[derive(Debug)]
enum SpecialInstruction {
    None,
    Pack,
    Invalid(Span, String),
}

#[derive(Debug)]
struct FieldInput {
    pub vis: syn::Visibility,
    pub ident: syn::Ident,
    pub field_type: syn::Type,
    pub special_instruction: SpecialInstruction,
    pub serialize_type: Option<syn::Type>,
}

impl FieldInput {
    fn get_serialize_type(&self) -> syn::Type {
        if let Some(serialize_type) = self.serialize_type.clone() {
            serialize_type
        } else {
            self.field_type.clone()
        }
    }
}

impl Parse for FieldInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let vis = input.parse()?;
        let ident = input.parse()?;
        let _: Token![:] = input.parse()?;
        let field_type = input.parse()?;

        let (special_instruction, serialize_type) = if input.peek(Token![as]) {
            let _: Token![as] = input.parse()?;

            if input.peek2(Token![!]) {
                let instruction: Ident = input.parse()?;
                let _: Token![!] = input.parse()?;

                let _parenthesized;
                parenthesized!(_parenthesized in input);

                let instruction_str = instruction.to_string();
                if instruction_str == "packed" {
                    match &field_type {
                        syn::Type::Path(path) => {
                            if path.path.is_ident("bool") {
                                (SpecialInstruction::Pack, None)
                            } else {
                                (
                                    SpecialInstruction::Invalid(
                                        instruction.span(),
                                        "packed instruction only valid for bools".into(),
                                    ),
                                    None,
                                )
                            }
                        }
                        _ => (
                            SpecialInstruction::Invalid(
                                instruction.span(),
                                "packed instruction only valid for bools".into(),
                            ),
                            None,
                        ),
                    }
                } else {
                    let error_str = format!(
                        "unknown custom serialization instruction: {}",
                        instruction_str
                    );
                    (
                        SpecialInstruction::Invalid(instruction.span(), error_str),
                        None,
                    )
                }
            } else {
                (SpecialInstruction::None, Some(input.parse()?))
            }
        } else {
            (SpecialInstruction::None, None)
        };

        Ok(Self {
            vis,
            ident,
            field_type,
            special_instruction,
            serialize_type,
        })
    }
}

impl ToTokens for FieldInput {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let vis = &self.vis;
        let ident = &self.ident;
        let field_type = &self.field_type;
        tokens.extend(quote!(#vis #ident: #field_type));
    }
}

#[derive(Debug)]
struct InputVariant {
    ident: syn::Ident,
    lifetime: Option<syn::Lifetime>,
    fields: Vec<FieldInput>,
}

impl Parse for InputVariant {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident = input.parse()?;

        let lifetime = if input.peek(Token![<]) {
            let _: Token![<] = input.parse()?;
            let lifetime = input.parse()?;
            let _: Token![>] = input.parse()?;
            Some(lifetime)
        } else {
            None
        };

        let braced;
        braced!(braced in input);
        let punctuated: Punctuated<FieldInput, Token![,]> = Punctuated::parse_terminated(&braced)?;

        Ok(Self {
            ident,
            lifetime,
            fields: punctuated.into_iter().collect(),
        })
    }
}

impl ToTokens for InputVariant {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = &self.ident;
        let lifetime = self.lifetime.iter();
        let fields = &self.fields;

        tokens.extend(quote!(
            #ident #(<#lifetime>)* {
                #(#fields),*
            }
        ));
    }
}

struct VariantSerializeImpl {
    read_impl: proc_macro2::TokenStream,
    read_impl_construct: proc_macro2::TokenStream,
    get_write_size_impl: proc_macro2::TokenStream,
    write_impl: proc_macro2::TokenStream,
}

impl InputVariant {
    fn get_lifetime_or_default(&self) -> proc_macro2::TokenStream {
        if let Some(lifetime) = &self.lifetime {
            lifetime.to_token_stream()
        } else {
            quote!('a)
        }
    }

    fn get_ident_and_lifetime(&self) -> proc_macro2::TokenStream {
        let ident = &self.ident;
        let lifetime = self.lifetime.iter();

        quote!(#ident #(<#lifetime>)*)
    }

    fn get_slice_serializable_impl(
        &self,
        lifetime: proc_macro2::TokenStream,
        prefix: proc_macro2::TokenStream,
    ) -> result::Result<VariantSerializeImpl, (Span, String)> {
        let mut get_write_size_impl: proc_macro2::TokenStream = Default::default();
        let mut write_impl: proc_macro2::TokenStream = Default::default();
        let mut read_impl: proc_macro2::TokenStream = Default::default();
        let mut read_impl_construct: proc_macro2::TokenStream = Default::default();

        let mut total_packed = 0;
        let mut packed = Vec::new();

        for field_index in 0..self.fields.len() {
            let field = &self.fields[field_index];
            let field_ident = &field.ident;

            read_impl_construct.extend(quote!(
                #field_ident,
            ));

            let serialize_type = field.get_serialize_type();
            let serialize_type_span = serialize_type.span();
            let field_type = &field.field_type;

            let serializable_impl = quote_spanned!(
                serialize_type_span =>
                <#serialize_type as binary::slice_serialization::SliceSerializable<#lifetime, #field_type>>
            );

            match &field.special_instruction {
                SpecialInstruction::None => {}
                SpecialInstruction::Pack => {
                    if packed.len() == 8 {
                        return Err((
                            field_ident.span(),
                            "packed only supports up to 8 fields".into(),
                        ));
                    }
                    packed.push(field_ident.clone());

                    let has_more_packed = if field_index == self.fields.len() - 1 {
                        false
                    } else {
                        let next_field = &self.fields[field_index+1];
                        matches!(next_field.special_instruction, SpecialInstruction::Pack)
                    };

                    if !has_more_packed {
                        // Packed get_write_size
                        get_write_size_impl.extend(quote_spanned!(
                            serialize_type_span => 1 +
                        ));

                        // Packed write
                        let mut inner_write_impl = quote!();
                        for (index, field) in packed.iter().enumerate() {
                            let value: u8 = 1 << index;

                            let or = if index == 0 { quote!() } else { quote!(|) };
                            inner_write_impl.extend(quote_spanned!(
                                serialize_type_span =>
                                #or (if *#prefix #field { #value } else { 0 })
                            ))
                        }
                        write_impl.extend(
                            quote_spanned!(
                                serialize_type_span =>
                                bytes = <binary::slice_serialization::Single as SliceSerializable<'_, u8>>::write(bytes,
                                    #inner_write_impl
                                );
                            )
                        );

                        let packed_ident = Ident::new(
                            &format!("internal_packed_{total_packed}"),
                            serialize_type_span,
                        );

                        // Packed read
                        read_impl.extend(
                            quote_spanned!(
                                serialize_type_span =>
                                let #packed_ident = <binary::slice_serialization::Single as SliceSerializable<'_, u8>>::read(bytes)?;
                            )
                        );

                        for (index, field) in packed.iter().enumerate() {
                            let value: u8 = 1 << index;
                            read_impl.extend(quote_spanned!(
                                serialize_type_span =>
                                let #field = (#packed_ident & #value) != 0;
                            ))
                        }

                        total_packed += 1;
                        packed = Vec::new();
                    }

                    continue;
                }
                SpecialInstruction::Invalid(span, msg) => {
                    return Err((*span, msg.clone()));
                }
            }

            get_write_size_impl.extend(
                quote_spanned!(
                    serialize_type_span =>
                    #serializable_impl::get_write_size(#serializable_impl::maybe_deref(#prefix #field_ident)) +
                )
            );

            write_impl.extend(
                quote_spanned!(
                    serialize_type_span =>
                    bytes = #serializable_impl::write(bytes, #serializable_impl::maybe_deref(#prefix #field_ident));
                )
            );

            read_impl.extend(quote_spanned!(
                serialize_type_span =>
                let #field_ident = #serializable_impl::read(bytes)?;
            ));
        }

        Ok(VariantSerializeImpl {
            read_impl,
            read_impl_construct,
            get_write_size_impl,
            write_impl,
        })
    }
}

#[derive(Debug)]
enum Input {
    Struct {
        attributes: Vec<syn::Attribute>,
        vis: syn::Visibility,
        data: InputVariant,
    },
    Enum {
        attributes: Vec<syn::Attribute>,
        vis: syn::Visibility,
        ident: syn::Ident,
        lifetime: Option<syn::Lifetime>,
        variants: Vec<InputVariant>,
    },
}

impl Input {
    fn get_base_data(&self) -> proc_macro2::TokenStream {
        match self {
            Input::Struct {
                attributes,
                vis,
                data,
            } => {
                quote!(
                    #(#attributes)*
                    #vis struct #data
                )
            }
            Input::Enum {
                attributes,
                vis,
                ident,
                lifetime,
                variants,
            } => {
                let lifetime = lifetime.iter();
                quote!(
                    #(#attributes)*
                    #vis enum #ident #(<#lifetime>)* {
                        #(#variants),*
                    }
                )
            }
        }
    }

    fn get_slice_serializable_impl(
        &self,
    ) -> result::Result<proc_macro2::TokenStream, (Span, String)> {
        match self {
            Input::Struct {
                attributes: _,
                vis: _,
                data,
            } => {
                let ident = data.get_ident_and_lifetime();
                let variant_impl = data.get_slice_serializable_impl(
                    data.get_lifetime_or_default(),
                    quote!(&object.),
                )?;

                let read_impl = variant_impl.read_impl;
                let read_impl_construct = variant_impl.read_impl_construct;
                let get_write_size_impl = variant_impl.get_write_size_impl;
                let write_impl = variant_impl.write_impl;

                let lifetime = data.get_lifetime_or_default();

                Ok(quote!(
                    impl <#lifetime> binary::slice_serialization::SliceSerializable<#lifetime> for #ident {
                        type RefType = &#lifetime #ident;

                        fn read(bytes: &mut &#lifetime [u8]) -> anyhow::Result<#ident> {
                            #read_impl

                            Ok(Self {
                                #read_impl_construct
                            })
                        }

                        fn get_write_size(object: &#lifetime #ident) -> usize {
                            #get_write_size_impl
                            0
                        }

                        unsafe fn write<'bytes>(mut bytes: &'bytes mut [u8], object: &#lifetime #ident) -> &'bytes mut [u8] {
                            #write_impl
                            bytes
                        }

                        fn maybe_deref(t: &#lifetime #ident) -> Self::RefType {
                            t
                        }
                    }
                ))
            }
            Input::Enum {
                attributes: _,
                vis: _,
                ident,
                lifetime,
                variants,
            } => {
                // Check for duplicate variant names
                let mut variant_names: HashSet<String> = HashSet::new();
                for variant in variants {
                    let name = variant.ident.to_string();
                    if variant_names.contains(&name) {
                        return Err((variant.ident.span(), "Duplicate variant name".into()));
                    } else {
                        variant_names.insert(name);
                    }
                }

                let mut get_write_size_impl: proc_macro2::TokenStream = Default::default();
                let mut write_impl: proc_macro2::TokenStream = Default::default();
                let mut read_impl: proc_macro2::TokenStream = Default::default();

                let lifetime_iter = lifetime.iter();
                let ident = quote!(#ident #(<#lifetime_iter>)*);

                let lifetime = if let Some(lifetime) = &lifetime {
                    lifetime.to_token_stream()
                } else {
                    quote!('a)
                };

                for (index, variant) in variants.iter().enumerate() {
                    let ident = &variant.ident;
                    let variant_impl =
                        variant.get_slice_serializable_impl(lifetime.clone(), quote!())?;

                    let variant_read_impl = variant_impl.read_impl;
                    let variant_read_impl_construct = variant_impl.read_impl_construct;
                    let variant_get_write_size_impl = variant_impl.get_write_size_impl;
                    let variant_write_impl = variant_impl.write_impl;

                    let mut enum_struct_fields = quote!();
                    for field in &variant.fields {
                        let field_ident = &field.ident;
                        enum_struct_fields.extend(quote!(#field_ident,));
                    }

                    let index = index as u8;
                    read_impl.extend(quote!(
                        #index => {
                            #variant_read_impl

                            Ok(Self::#ident {
                                #variant_read_impl_construct
                            })
                        }
                    ));

                    get_write_size_impl.extend(quote!(
                        Self::#ident { #enum_struct_fields } => {
                            #variant_get_write_size_impl
                            1
                        }
                    ));

                    write_impl.extend(
                        quote!(
                            Self::#ident { #enum_struct_fields } => {
                                bytes = <binary::slice_serialization::Single as binary::slice_serialization::SliceSerializable<u8>>::write(bytes, #index);
                                #variant_write_impl
                                bytes
                            }
                        )
                    );
                }

                Ok(quote!(
                    impl <#lifetime> binary::slice_serialization::SliceSerializable<#lifetime> for #ident {
                        type RefType = &#lifetime #ident;

                        fn read(bytes: &mut &#lifetime [u8]) -> anyhow::Result<#ident> {
                            let discriminant = <binary::slice_serialization::Single as binary::slice_serialization::SliceSerializable<u8>>::read(bytes)?;
                            match discriminant {
                                #read_impl
                                _ => {
                                    anyhow::bail!("Unknown variant: {}", discriminant);
                                }
                            }
                        }

                        fn get_write_size(object: &#lifetime #ident) -> usize {
                            match object {
                                #get_write_size_impl
                            }
                        }

                        unsafe fn write<'bytes>(mut bytes: &'bytes mut [u8], object: &#lifetime #ident) -> &'bytes mut [u8] {
                            match object {
                                #write_impl
                            }
                        }

                        fn maybe_deref(t: &#lifetime #ident) -> Self::RefType {
                            t
                        }
                    }
                ))
            }
        }
    }
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attributes: Vec<syn::Attribute> = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;

        if input.peek(Token![enum]) {
            let _: Token![enum] = input.parse()?;
            let ident = input.parse()?;

            let lifetime = if input.peek(Token![<]) {
                let _: Token![<] = input.parse()?;
                let lifetime = input.parse()?;
                let _: Token![>] = input.parse()?;
                Some(lifetime)
            } else {
                None
            };

            let braced;
            braced!(braced in input);
            let punctuated: Punctuated<InputVariant, Token![,]> =
                Punctuated::parse_terminated(&braced)?;

            Ok(Self::Enum {
                attributes,
                vis,
                ident,
                lifetime,
                variants: punctuated.into_iter().collect(),
            })
        } else {
            let _: Token![struct] = input.parse()?;
            let data = input.parse()?;

            Ok(Self::Struct {
                attributes,
                vis,
                data,
            })
        }
    }
}

#[proc_macro]
pub fn slice_serializable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Input);

    let base_data = input.get_base_data();
    let serialize_impl = input.get_slice_serializable_impl();

    match serialize_impl {
        Ok(serialize_impl) => quote!(
            #base_data
            #serialize_impl
        )
        .into(),
        Err((span, msg)) => {
            let spanned = quote_spanned!(span => compile_error!(#msg););

            quote!(
                #base_data
                #spanned
            )
            .into()
        }
    }
}
