extern crate proc_macro;

#[macro_use]
mod macros;

use proc_macro2::TokenStream;
use quote::*;
use syn::punctuated::Punctuated;
use syn::*;

#[proc_macro_derive(VertexLayout, attributes(vertex))]
pub fn derive_vertex(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    match impl_vertex(input) {
        Ok(output) => output.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct FieldOptions {
    size: Option<Expr>,
    format: Expr,
    location: Expr,
}

fn impl_vertex(input: DeriveInput) -> Result<TokenStream> {
    let data = match input.data {
        Data::Struct(data) => data,
        Data::Enum(data) => return Err(err!(&data.enum_token, "only allowed on structs")),
        Data::Union(data) => return Err(err!(&data.union_token, "only allowed on structs")),
    };

    let mut attributes = Vec::new();

    let mut current_offset = quote! { 0 };

    for field in data.fields.iter() {
        let options = FieldOptions::from_field(&field)?;

        let size = options
            .size
            .map(|size| quote! { #size })
            .unwrap_or_else(|| {
                let ty = &field.ty;
                quote! {
                    ::std::mem::size_of::<#ty>() as u64
                }
            });

        let offset = &current_offset;
        let format = options.format;
        let location = options.location;

        let attribute = quote! {
            wgpu::VertexAttributeDescriptor {
                offset: #offset,
                format: #format,
                shader_location: #location,
            }
        };

        attributes.push(attribute);

        current_offset = quote! { (#current_offset) + (#size) };
    }

    let lib = lib!();
    let ident = input.ident;

    let output = quote! {
        impl #lib::VertexLayout for #ident {
            const ATTRIBUTES: &'static [wgpu::VertexAttributeDescriptor] = &[
                #(#attributes),*
            ];
        }
    };

    Ok(output)
}

impl FieldOptions {
    pub fn from_field(field: &Field) -> Result<Self> {
        let pairs = field
            .attrs
            .iter()
            .map(|attr| {
                attr.parse_args_with(|stream: parse::ParseStream| {
                    Punctuated::<NameValue, Token![,]>::parse_terminated(stream)
                })
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten();

        let mut size = None;
        let mut format = None;
        let mut location = None;

        for NameValue { name, value } in pairs.into_iter() {
            if name == "format" {
                format = Some(value);
            } else if name == "location" {
                location = Some(value);
            } else if name == "size" {
                size = Some(value);
            }
        }

        Ok(FieldOptions {
            size,
            format: format.ok_or_else(|| err!(field, "missing `vertex(format = ...)`"))?,
            location: location.ok_or_else(|| err!(field, "missing `vertex(location = ...)`"))?,
        })
    }
}

struct NameValue {
    name: Ident,
    value: Expr,
}

impl parse::Parse for NameValue {
    fn parse(input: parse::ParseStream) -> Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![=]>()?;
        let value = input.parse()?;
        Ok(NameValue { name, value })
    }
}
