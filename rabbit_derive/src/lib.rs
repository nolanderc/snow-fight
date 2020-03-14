extern crate proc_macro;

#[macro_use]
mod macros;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    spanned::Spanned, Data, DataEnum, DataStruct, DeriveInput, Field, Fields, Ident, Result,
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

#[proc_macro_derive(Rabbit)]
pub fn derive_rabbit(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut pack = derive_pack_bits(item.clone());
    let unpack = derive_unpack_bits(item);
    pack.extend(unpack);
    pack
}

#[proc_macro_derive(PackBits)]
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
    let pack_fields = pack_idents(idents.iter());

    let output = quote! {
        let Self #destructure = self;
        #pack_fields
        Ok(())
    };

    Ok(output)
}

fn pack_enum_body(data: &DataEnum) -> Result<TokenStream> {
    let index_bits = index_bits(data)?;

    let variants = data.variants.iter().enumerate().map(|(index, variant)| {
        let variant_index = index as u32;

        let ident = &variant.ident;
        let (destructure, idents) = field_destructure(&variant.fields);
        let pack_fields = pack_idents(idents.iter());

        let rabbit = rabbit!();
        quote! {
            Self::#ident #destructure => {
                #rabbit::WriteBits::write(__writer, #variant_index, #index_bits)?;
                #pack_fields
            }
        }
    });

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
    let unpack_fields = unpack_fields(idents.iter().zip(&data.fields));

    let output = quote! {
        #unpack_fields
        Ok(Self #destructure)
    };

    Ok(output)
}

fn unpack_enum_body(data: &DataEnum) -> Result<TokenStream> {
    let index_bits = index_bits(data)?;

    let variants = data.variants.iter().enumerate().map(|(index, variant)| {
        let variant_index = index as u32;

        let ident = &variant.ident;
        let (destructure, idents) = field_destructure(&variant.fields);
        let unpack_fields = unpack_fields(idents.iter().zip(&variant.fields));

        quote! {
            #variant_index => {
                #unpack_fields
                Ok(Self::#ident #destructure)
            }
        }
    });

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

fn index_bits(data: &DataEnum) -> Result<u8> {
    if data.variants.is_empty() {
        Err(err!(data.enum_token, "enum must have atleast one variant"))
    } else {
        let max_index = data.variants.len().saturating_sub(1) as u32;
        Ok(32 - max_index.leading_zeros() as u8)
    }
}

fn pack_idents<'a>(names: impl Iterator<Item = &'a Ident>) -> TokenStream {
    let rabbit = rabbit!();
    quote! {
        #( #rabbit::PackBits::pack(#names, __writer)?; )*
    }
}

fn unpack_fields<'a>(fields: impl Iterator<Item = (&'a Ident, &'a Field)>) -> TokenStream {
    let rabbit = rabbit!();

    let readers = fields.map(|(ident, field)| {
        let ty = &field.ty;
        quote! {
            let #ident = <#ty as #rabbit::UnpackBits>::unpack(__reader)?;
        }
    });

    quote! {
        #( #readers )*
    }
}
