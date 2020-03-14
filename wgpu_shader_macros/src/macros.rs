
macro_rules! err {
    ($span:expr, $msg:expr) => {
        syn::Error::new_spanned($span, $msg)
    }
}

macro_rules! lib {
    () => {
        quote::quote! { wgpu_shader }
    }
}

