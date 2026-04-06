use proc_macro::TokenStream;
use quote::quote;
use syn::DeriveInput;
use syn::parse_macro_input;
use syn::spanned::Spanned;

#[proc_macro_derive(AsyncToService)]
pub fn derive_async_to_service(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    if !input.generics.params.is_empty() {
        return syn::Error::new(
            input.generics.span(),
            "AsyncToService derive only supports non-generic structs",
        )
        .to_compile_error()
        .into();
    }

    let name = input.ident;
    let fields = match input.data {
        syn::Data::Struct(data) => match data.fields {
            syn::Fields::Named(fields) => fields.named,
            _ => {
                return syn::Error::new(
                    name.span(),
                    "AsyncToService derive only supports named structs",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new(name.span(), "AsyncToService derive only supports structs")
                .to_compile_error()
                .into();
        }
    };

    let service_fields = fields.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named field");
        let vis = &field.vis;
        let ty = &field.ty;
        let cfg_attrs = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"));

        quote! {
            #(#cfg_attrs)*
            #vis #ident: <#ty as ::butil::prelude::AsyncToService>::Service
        }
    });

    let convert_fields = fields.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        let cfg_attrs = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"));

        quote! {
            #(#cfg_attrs)*
            let #ident = <#ty as ::butil::prelude::AsyncToService>::to_service(&self.#ident).await?;
        }
    });

    let init_fields = fields.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named field");
        let cfg_attrs = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"));

        quote! {
            #(#cfg_attrs)*
            #ident
        }
    });

    quote! {
        pub struct Service {
            #(#service_fields),*
        }

        impl ::butil::prelude::AsyncToService for #name {
            type Service = Service;

            fn to_service(
                &self,
            ) -> impl ::std::future::Future<Output = ::core::result::Result<Self::Service, ::butil::error::ConfigError>> {
                async move {
                    #(#convert_fields)*
                    ::core::result::Result::Ok(Service {
                        #(#init_fields),*
                    })
                }
            }
        }
    }
    .into()
}
