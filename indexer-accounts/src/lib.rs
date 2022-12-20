use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemStruct};

#[proc_macro_derive(FieldCount)]
pub fn derive_field_count(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let field_count = input.fields.iter().count();

    let output = quote! {
        impl #impl_generics FieldCount for #name #ty_generics #where_clause {
            fn field_count() -> usize {
                #field_count
            }
        }
    };

    TokenStream::from(output)
}
