use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Data, DeriveInput, Expr, Fields, GenericArgument, Ident, LitStr, PathArguments, Type,
    parse_macro_input, spanned::Spanned,
};

// ── Lo que el macro sabe de cada campo ─────────────────────────────────────

enum ValorDefecto {
    Trait,           // #[builder(default)]        → Default::default()
    Expr(Box<Expr>), // #[builder(default = "42")] → la expresión del string
}

struct Campo {
    ident: Ident,
    ty: Type,
    defecto: Option<ValorDefecto>,
    /// #[builder(each = "item")]: nombre del setter y tipo T del Vec<T>
    each: Option<(Ident, Type)>,
}

// ── Parseo de atributos #[builder(...)] ───────────────────────────────────

fn parsear_campo(field: &syn::Field) -> syn::Result<Campo> {
    let mut defecto = None;
    let mut each = None;

    for attr in field.attrs.iter().filter(|a| a.path().is_ident("builder")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") {
                defecto = if meta.input.peek(syn::Token![=]) {
                    // El string se parsea como expresión; sus tokens conservan el
                    // span del literal, así los errores de tipo apuntan a él.
                    let s: LitStr = meta.value()?.parse()?;
                    Some(ValorDefecto::Expr(Box::new(s.parse::<Expr>()?)))
                } else {
                    Some(ValorDefecto::Trait)
                };
                Ok(())
            } else if meta.path.is_ident("each") {
                let s: LitStr = meta.value()?.parse()?;
                let interior = tipo_interior_vec(&field.ty).ok_or_else(|| {
                    syn::Error::new_spanned(&field.ty, "`each` requiere un campo de tipo Vec<T>")
                })?;
                each = Some((Ident::new(&s.value(), s.span()), interior.clone()));
                Ok(())
            } else {
                Err(meta.error("atributo builder desconocido; se esperaba `default` o `each`"))
            }
        })?;
    }

    Ok(Campo {
        ident: field
            .ident
            .clone()
            .expect("solo se llama con campos nombrados"),
        ty: field.ty.clone(),
        defecto,
        each,
    })
}

/// El `T` de `Vec<T>`, o `None` si el tipo no es un `Vec`.
fn tipo_interior_vec(ty: &Type) -> Option<&Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Vec" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}

// ── Entry point del proc-macro ─────────────────────────────────────────────

#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive_builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Un error no es un panic: se convierte en compile_error!{...} con su span
    implementar_builder(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn implementar_builder(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let nombre = &input.ident;
    let vis = &input.vis;
    let nombre_builder = format_ident!("{nombre}Builder");

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            nombre,
            "Builder solo soporta structs con campos nombrados",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            data.fields.span(),
            "Builder requiere campos nombrados (no tuple structs)",
        ));
    };

    let campos = fields
        .named
        .iter()
        .map(parsear_campo)
        .collect::<syn::Result<Vec<_>>>()?;

    // Genéricos: `struct Paginado<T>` necesita `PaginadoBuilder<T>` e
    // `impl<T> ... for PaginadoBuilder<T>`. split_for_impl da cada pieza.
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // ── Campos del builder: Option<T>, o el Vec directamente con `each` ───
    let campos_struct = campos.iter().map(|c| {
        let (ident, ty) = (&c.ident, &c.ty);
        if c.each.is_some() {
            quote! { #ident: #ty, }
        } else {
            quote! { #ident: ::core::option::Option<#ty>, }
        }
    });

    let campos_vacios = campos.iter().map(|c| {
        let ident = &c.ident;
        if c.each.is_some() {
            quote! { #ident: ::std::vec::Vec::new(), }
        } else {
            quote! { #ident: ::core::option::Option::None, }
        }
    });

    // ── Setters ────────────────────────────────────────────────────────────
    let setters = campos.iter().map(|c| {
        let (ident, ty) = (&c.ident, &c.ty);
        match &c.each {
            // each: añade UN elemento en lugar de reemplazar el Vec
            Some((setter, interior)) => quote_spanned! { setter.span()=>
                pub fn #setter(mut self, valor: #interior) -> Self {
                    self.#ident.push(valor);
                    self
                }
            },
            None => quote_spanned! { ident.span()=>
                pub fn #ident(mut self, valor: #ty) -> Self {
                    self.#ident = ::core::option::Option::Some(valor);
                    self
                }
            },
        }
    });

    // ── Cuerpo de build() ──────────────────────────────────────────────────
    let build_campos = campos.iter().map(|c| {
        let ident = &c.ident;
        if c.each.is_some() {
            return quote! { #ident: self.#ident, };
        }
        match &c.defecto {
            Some(ValorDefecto::Trait) => quote! {
                #ident: self.#ident.unwrap_or_default(),
            },
            // Span de la expresión: un error de tipo apunta al "..." del atributo
            Some(ValorDefecto::Expr(expr)) => quote_spanned! { expr.span()=>
                #ident: self.#ident.unwrap_or_else(|| #expr),
            },
            None => {
                let msg = format!("falta el campo requerido `{ident}`");
                quote! { #ident: self.#ident.ok_or(#msg)?, }
            }
        }
    });

    let doc = format!("Builder de [`{nombre}`], generado por `#[derive(Builder)]`.");

    Ok(quote! {
        #[doc = #doc]
        #vis struct #nombre_builder #generics #where_clause {
            #(#campos_struct)*
        }

        // Default a mano: con #[derive(Default)] se exigiría T: Default
        impl #impl_generics ::core::default::Default for #nombre_builder #ty_generics #where_clause {
            fn default() -> Self {
                Self { #(#campos_vacios)* }
            }
        }

        impl #impl_generics #nombre_builder #ty_generics #where_clause {
            #(#setters)*

            pub fn build(self) -> ::core::result::Result<#nombre #ty_generics, ::std::string::String> {
                ::core::result::Result::Ok(#nombre {
                    #(#build_campos)*
                })
            }
        }

        impl #impl_generics #nombre #ty_generics #where_clause {
            pub fn builder() -> #nombre_builder #ty_generics {
                ::core::default::Default::default()
            }
        }
    })
}
