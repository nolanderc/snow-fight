extern crate proc_macro;

#[macro_use]
mod macros;

use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse::ParseStream, punctuated::Punctuated, spanned::Spanned, Data, DataEnum, DataStruct,
    DeriveInput, Field, Fields, Ident, Lit, MetaNameValue, Path, Result, Token,
};

struct Errors {
    error: Option<syn::Error>,
}

impl Errors {
    pub fn new() -> Self {
        Errors { error: None }
    }

    pub fn push(&mut self, error: syn::Error) {
        match self.error.as_mut() {
            None => self.error = Some(error),
            Some(err) => err.combine(error),
        }
    }

    pub fn finish<T>(self, value: T) -> Result<T> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(value),
        }
    }
}

struct Attributes {
    pack_fn: Option<Path>,
    unpack_fn: Option<Path>,
}

#[proc_macro_derive(Rabbit, attributes(rabbit))]
pub fn derive_rabbit(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut pack = derive_pack_bits(item.clone());
    let unpack = derive_unpack_bits(item);
    pack.extend(unpack);
    pack
}

#[proc_macro_derive(PackBits, attributes(rabbit))]
pub fn derive_pack_bits(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(item as DeriveInput);

    match impl_pack_bits(input) {
        Ok(output) => output.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(UnpackBits)]
pub fn derive_unpack_bits(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(item as DeriveInput);

    match impl_unpack_bits(input) {
        Ok(output) => output.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn impl_pack_bits(input: DeriveInput) -> Result<TokenStream> {
    let body = item_body(&input.data, pack_struct_body, pack_enum_body)?;

    let rabbit = rabbit!();
    let pack = quote! {
        fn pack<__W>(&self, __writer: &mut __W) -> Result<(), __W::Error>
        where
            __W: #rabbit::WriteBits,
        {
            #body
        }
    };

    impl_trait(&input, quote! { rabbit::PackBits }, pack)
}

fn impl_unpack_bits(input: DeriveInput) -> Result<TokenStream> {
    let body = item_body(&input.data, unpack_struct_body, unpack_enum_body)?;

    let rabbit = rabbit!();
    let unpack = quote! {
        fn unpack<__R>(__reader: &mut __R) -> Result<Self, __R::Error>
        where
            __R: #rabbit::ReadBits,
        {
            #body
        }
    };

    impl_trait(&input, quote! { rabbit::UnpackBits }, unpack)
}

fn item_body(
    data: &Data,
    struct_body: fn(&DataStruct) -> Result<TokenStream>,
    enum_body: fn(&DataEnum) -> Result<TokenStream>,
) -> Result<TokenStream> {
    match data {
        syn::Data::Struct(data) => struct_body(&data),
        syn::Data::Enum(data) => enum_body(&data),
        syn::Data::Union(data) => Err(err!(
            data.union_token,
            "only available for `struct`s and `enum`s"
        )),
    }
}

fn impl_trait(input: &DeriveInput, name: TokenStream, items: TokenStream) -> Result<TokenStream> {
    let mut errors = Errors::new();

    let ident = &input.ident;
    let lt = &input.generics.lt_token;
    let gt = &input.generics.gt_token;
    let where_clause = &input.generics.where_clause;
    let generic_params = &input.generics.params;
    let generic_idents = generic_params.iter().filter_map(|param| match param {
        syn::GenericParam::Type(ty) => Some(ty.ident.clone()),
        syn::GenericParam::Const(value) => Some(value.ident.clone()),
        syn::GenericParam::Lifetime(life) => {
            errors.push(err!(life, "lifetimes are not allowed"));
            None
        }
    });

    let output = quote! {
        impl #lt #generic_params #gt #name
            for #ident #lt #(#generic_idents),* #gt
                #where_clause
        {
            #items
        }
    };

    errors.finish(output)
}

fn pack_struct_body(data: &DataStruct) -> Result<TokenStream> {
    let (destructure, idents) = field_destructure(&data.fields);
    let attrs = field_attributes(&data.fields)?;
    let pack_fields = pack_fields(idents.iter().zip(&attrs));

    let output = quote! {
        let Self #destructure = self;
        #pack_fields
        Ok(())
    };

    Ok(output)
}

fn pack_enum_body(data: &DataEnum) -> Result<TokenStream> {
    let index_bits = index_bits(data)?;

    let variants = data
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let variant_index = index as u32;

            let ident = &variant.ident;
            let (destructure, idents) = field_destructure(&variant.fields);
            let attrs = field_attributes(&variant.fields)?;
            let pack_fields = pack_fields(idents.iter().zip(&attrs));

            let rabbit = rabbit!();
            Ok(quote! {
                Self::#ident #destructure => {
                    #rabbit::WriteBits::write(__writer, #variant_index, #index_bits)?;
                    #pack_fields
                }
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let output = quote! {
        match self {
            #( #variants ),*
        }

        Ok(())
    };

    Ok(output)
}

fn unpack_struct_body(data: &DataStruct) -> Result<TokenStream> {
    let (destructure, idents) = field_destructure(&data.fields);
    let unpack_fields = unpack_fields(idents.iter().zip(&data.fields))?;

    let output = quote! {
        #unpack_fields
        Ok(Self #destructure)
    };

    Ok(output)
}

fn unpack_enum_body(data: &DataEnum) -> Result<TokenStream> {
    let index_bits = index_bits(data)?;

    let variants = data
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let variant_index = index as u32;

            let ident = &variant.ident;
            let (destructure, idents) = field_destructure(&variant.fields);
            let unpack_fields = unpack_fields(idents.iter().zip(&variant.fields))?;

            Ok(quote! {
                #variant_index => {
                    #unpack_fields
                    Ok(Self::#ident #destructure)
                }
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let rabbit = rabbit!();
    let output = quote! {
        let variant_index = #rabbit::ReadBits::read(__reader, #index_bits)?;
        match variant_index {
            #( #variants ),*
            _ => Err(<__R::Error as #rabbit::read::Error>::custom(
                format!("unknown variant index: {}", variant_index)
            )),
        }
    };

    Ok(output)
}

fn field_destructure(fields: &Fields) -> (TokenStream, Vec<Ident>) {
    let idents = field_idents(fields).collect::<Vec<_>>();

    let destructure = match fields {
        Fields::Named(_) => quote! { { #( #idents ),* } },
        Fields::Unnamed(_) => quote! {( #( #idents ),* ) },
        Fields::Unit => quote! {},
    };

    (destructure, idents)
}

fn field_idents<'a>(fields: &'a Fields) -> impl Iterator<Item = Ident> + 'a {
    fields
        .iter()
        .enumerate()
        .map(|(i, field)| match &field.ident {
            Some(ident) => ident.clone(),
            None => Ident::new(&format!("_{}", i), field.span()),
        })
}

fn field_attributes(fields: &Fields) -> Result<Vec<Attributes>> {
    fields.iter().map(extract_attributes).collect()
}

fn extract_attributes(field: &Field) -> Result<Attributes> {
    let mut attrs = Attributes::default();

    let raw_attrs = field
        .attrs
        .iter()
        .filter(|attr| attr.path.is_ident("rabbit"));

    for attr in raw_attrs {
        let args = attr.parse_args_with(|stream: ParseStream| {
            Punctuated::<MetaNameValue, Token![,]>::parse_terminated(stream)
        })?;

        let lit_str = |lit| match lit {
            Lit::Str(value) => Ok(value),
            _ => Err(err!(lit, "expected a string literal")),
        };

        for arg in args {
            if arg.path.is_ident("pack") {
                attrs.pack_fn = Some(lit_str(arg.lit)?.parse()?);
            } else if arg.path.is_ident("unpack") {
                attrs.unpack_fn = Some(lit_str(arg.lit)?.parse()?);
            } else if arg.path.is_ident("with") {
                let value: Path = lit_str(arg.lit)?.parse()?;
                let member = |ident| {
                    let mut path = value.clone();
                    path.segments
                        .push(Ident::new(ident, Span::call_site()).into());
                    path
                };
                attrs.pack_fn = Some(member("pack"));
                attrs.unpack_fn = Some(member("unpack"));
            } else {
                return Err(err!(
                    &arg.path,
                    format!("unknown attribute: `{}`", arg.path.to_token_stream())
                ));
            }
        }
    }

    Ok(attrs)
}

fn index_bits(data: &DataEnum) -> Result<u8> {
    if data.variants.is_empty() {
        Err(err!(data.enum_token, "enum must have atleast one variant"))
    } else {
        let max_index = data.variants.len().saturating_sub(1) as u32;
        Ok(32 - max_index.leading_zeros() as u8)
    }
}

fn pack_fields<'a>(fields: impl Iterator<Item = (&'a Ident, &'a Attributes)>) -> TokenStream {
    let rabbit = rabbit!();

    let mut extractors = Vec::new();
    for (ident, attrs) in fields {
        let extractor = if let Some(pack_fn) = attrs.pack_fn.as_ref() {
            quote! { (#pack_fn)(#ident, __writer)?; }
        } else {
            quote! { #rabbit::PackBits::pack(#ident, __writer)?; }
        };

        extractors.push(extractor)
    }

    quote! { #( #extractors )* }
}

fn unpack_fields<'a>(fields: impl Iterator<Item = (&'a Ident, &'a Field)>) -> Result<TokenStream> {
    let rabbit = rabbit!();

    let mut readers = Vec::new();
    for (ident, field) in fields {
        let attrs = extract_attributes(field)?;

        let reader = if let Some(unpack_fn) = attrs.unpack_fn.as_ref() {
            quote! { (#unpack_fn)(__reader)? }
        } else {
            quote! { #rabbit::UnpackBits::unpack(__reader)? }
        };

        let ty = &field.ty;
        readers.push(quote! { let #ident: #ty = #reader; });
    }

    Ok(quote! { #( #readers )* })
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes {
            pack_fn: None,
            unpack_fn: None,
        }
    }
}
