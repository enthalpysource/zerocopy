use crate::{
    attrs::ItemMode,
    dummy,
    type_decl::{
        EnumDecl, FieldDecl, FieldsDecl, FieldsStyle, StructDecl, TypeData, TypeDecl, VariantDecl,
    },
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{parse_quote, Data, DeriveInput, Generics, Type};

pub fn expand_derive_serialize(input: &DeriveInput) -> syn::Result<TokenStream> {
    if let Data::Union(u) = &input.data {
        return Err(syn::Error::new(
            u.union_token.span(),
            "SerializeState does not support unions",
        ));
    }

    let decl = TypeDecl::from_derive_input(input)?;
    let impl_block = match &decl.data {
        TypeData::Struct(data) => derive_struct(&decl, data)?,
        TypeData::Enum(data) => derive_enum(&decl, data)?,
    };

    Ok(dummy::wrap_in_const(
        decl.attrs.serde_path.as_ref(),
        impl_block,
    ))
}

fn derive_struct(decl: &TypeDecl, data: &StructDecl) -> syn::Result<TokenStream> {
    let has_explicit_state = decl.attrs.state.is_some();
    let has_state_bound = decl.attrs.state_bound.is_some();
    let uses_generic_state = !has_explicit_state;
    let infer_bounds = !has_explicit_state && !has_state_bound;
    let impl_generics_storage = add_state_param(
        decl.generics,
        uses_generic_state,
        decl.attrs.state_bound.as_ref(),
    );
    let (impl_generics_ref, _, _) = impl_generics_storage.split_for_impl();
    let impl_generics = quote!(#impl_generics_ref);
    let (_, ty_generics_ref, _) = decl.generics.split_for_impl();
    let ty_generics = quote!(#ty_generics_ref);
    let mut where_clause = decl.generics.where_clause.clone();
    let state_tokens = state_type_tokens(decl);
    let field_types = collect_field_types_from_fields(&data.fields);
    if infer_bounds {
        add_serialize_bounds_from_types(&mut where_clause, &field_types, &state_tokens);
    } else {
        add_serialize_bounds_from_type_params(
            &mut where_clause,
            decl.generics,
            &state_tokens,
            decl.attrs.mode,
        );
    }
    let where_clause_tokens = match &where_clause {
        Some(clause) => quote!(#clause),
        None => TokenStream::new(),
    };
    let ident = decl.ident;

    let state_bound = decl.attrs.state_bound.as_ref();
    let explicit_state = decl.attrs.state.as_ref();
    let body = if decl.attrs.transparent {
        serialize_transparent(&data.fields, explicit_state, state_bound)?
    } else {
        serialize_struct_body(ident, &data.fields, explicit_state, state_bound)?
    };

    let default_serde_impl = default_serde_impl(decl, ident);

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics _serde_state::SerializeState<#state_tokens> for #ident #ty_generics #where_clause_tokens {
            fn serialize_state<__S>(
                &self,
                __state: &#state_tokens,
                __serializer: __S,
            ) -> ::core::result::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                #body
            }
        }

        #default_serde_impl
    })
}

fn derive_enum(decl: &TypeDecl, data: &EnumDecl) -> syn::Result<TokenStream> {
    let has_explicit_state = decl.attrs.state.is_some();
    let has_state_bound = decl.attrs.state_bound.is_some();
    let uses_generic_state = !has_explicit_state;
    let infer_bounds = !has_explicit_state && !has_state_bound;
    let impl_generics_storage = add_state_param(
        decl.generics,
        uses_generic_state,
        decl.attrs.state_bound.as_ref(),
    );
    let (impl_generics_ref, _, _) = impl_generics_storage.split_for_impl();
    let impl_generics = quote!(#impl_generics_ref);
    let (_, ty_generics_ref, _) = decl.generics.split_for_impl();
    let ty_generics = quote!(#ty_generics_ref);
    let mut where_clause = decl.generics.where_clause.clone();
    let state_tokens = state_type_tokens(decl);
    let field_types = collect_field_types_from_enum(data);
    if infer_bounds {
        add_serialize_bounds_from_types(&mut where_clause, &field_types, &state_tokens);
    } else {
        add_serialize_bounds_from_type_params(
            &mut where_clause,
            decl.generics,
            &state_tokens,
            decl.attrs.mode,
        );
    }
    let where_clause_tokens = match &where_clause {
        Some(clause) => quote!(#clause),
        None => TokenStream::new(),
    };
    let ident = decl.ident;
    let explicit_state = decl.attrs.state.as_ref();
    let body = serialize_enum_body(ident, data, explicit_state, decl.attrs.state_bound.as_ref())?;
    let default_serde_impl = default_serde_impl(decl, ident);

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics _serde_state::SerializeState<#state_tokens> for #ident #ty_generics #where_clause_tokens {
            fn serialize_state<__S>(
                &self,
                __state: &#state_tokens,
                __serializer: __S,
            ) -> ::core::result::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                #body
            }
        }

        #default_serde_impl
    })
}

fn state_bound_clause(bound: Option<&Type>) -> TokenStream {
    match bound {
        Some(ty) => quote!(+ #ty),
        None => TokenStream::new(),
    }
}

fn serialize_transparent(
    fields: &FieldsDecl<'_>,
    explicit_state: Option<&Type>,
    state_bound: Option<&Type>,
) -> syn::Result<TokenStream> {
    match fields.style {
        FieldsStyle::Named if fields.fields.len() == 1 => {
            let field = &fields.fields[0];
            let ident = field.ident().unwrap();
            let call =
                serialize_field_expr(field, quote!(&self.#ident), explicit_state, state_bound);
            Ok(quote! {
                _serde::Serialize::serialize(#call, __serializer)
            })
        }
        FieldsStyle::Unnamed if fields.fields.len() == 1 => {
            let index = syn::Index::from(0);
            let field = &fields.fields[0];
            let call =
                serialize_field_expr(field, quote!(&self.#index), explicit_state, state_bound);
            Ok(quote! {
                _serde::Serialize::serialize(#call, __serializer)
            })
        }
        _ => Err(syn::Error::new(
            fields.span,
            "transparent structs must have exactly one field",
        )),
    }
}

fn serialize_struct_body(
    ident: &syn::Ident,
    fields: &FieldsDecl<'_>,
    explicit_state: Option<&Type>,
    state_bound: Option<&Type>,
) -> syn::Result<TokenStream> {
    Ok(match fields.style {
        FieldsStyle::Named => {
            serialize_named_fields(ident, &fields.fields, explicit_state, state_bound)?
        }
        FieldsStyle::Unnamed => {
            serialize_unnamed_fields(ident, &fields.fields, explicit_state, state_bound)
        }
        FieldsStyle::Unit => serialize_unit_struct(ident),
    })
}

fn serialize_named_fields(
    ident: &syn::Ident,
    fields: &[FieldDecl<'_>],
    explicit_state: Option<&Type>,
    state_bound: Option<&Type>,
) -> syn::Result<TokenStream> {
    let type_name = ident.to_string();
    let len = fields.iter().filter(|field| !field.attrs.skip).count();
    let serialize_fields = fields
        .iter()
        .filter(|field| !field.attrs.skip)
        .map(|field| {
            let field_ident = field.ident().unwrap();
            let key = field.attrs.key(field_ident);
            let call = serialize_field_expr(
                field,
                quote!(&self.#field_ident),
                explicit_state,
                state_bound,
            );
            quote! {
                _serde::ser::SerializeStruct::serialize_field(
                    &mut __serde_state,
                    #key,
                    #call,
                )?;
            }
        });

    Ok(quote! {
        let mut __serde_state = _serde::Serializer::serialize_struct(__serializer, #type_name, #len)?;
        #(#serialize_fields)*
        _serde::ser::SerializeStruct::end(__serde_state)
    })
}

fn serialize_unnamed_fields(
    ident: &syn::Ident,
    fields: &[FieldDecl<'_>],
    explicit_state: Option<&Type>,
    state_bound: Option<&Type>,
) -> TokenStream {
    match fields.len() {
        0 => serialize_unit_struct(ident),
        1 => {
            let index = syn::Index::from(0);
            let call = serialize_field_expr(
                &fields[0],
                quote!(&self.#index),
                explicit_state,
                state_bound,
            );
            quote! {
                _serde::Serializer::serialize_newtype_struct(
                    __serializer,
                    stringify!(#ident),
                    #call,
                )
            }
        }
        len => {
            let serialize_fields = fields.iter().enumerate().map(|(i, field)| {
                let index = syn::Index::from(i);
                let call =
                    serialize_field_expr(field, quote!(&self.#index), explicit_state, state_bound);
                quote! {
                    _serde::ser::SerializeTupleStruct::serialize_field(
                        &mut __serde_state,
                        #call,
                    )?;
                }
            });
            quote! {
                let mut __serde_state = _serde::Serializer::serialize_tuple_struct(
                    __serializer,
                    stringify!(#ident),
                    #len,
                )?;
                #(#serialize_fields)*
                _serde::ser::SerializeTupleStruct::end(__serde_state)
            }
        }
    }
}

fn serialize_unit_struct(ident: &syn::Ident) -> TokenStream {
    quote! {
        _serde::Serializer::serialize_unit_struct(__serializer, stringify!(#ident))
    }
}

fn serialize_field_expr(
    field: &FieldDecl<'_>,
    value: TokenStream,
    explicit_state: Option<&Type>,
    state_bound: Option<&Type>,
) -> TokenStream {
    if let Some(with) = &field.attrs.with {
        let ty = field.ty();
        match explicit_state {
            Some(state_ty) => quote!(&{
                struct __SerdeStateWith<'state> {
                    value: &'state #ty,
                    state: &'state #state_ty,
                }

                impl<'state> _serde::Serialize for __SerdeStateWith<'state> {
                    fn serialize<__S>(&self, __serializer: __S) -> Result<__S::Ok, __S::Error>
                    where
                        __S: _serde::Serializer,
                    {
                        #with::serialize_state(self.value, self.state, __serializer)
                    }
                }

                __SerdeStateWith { value: #value, state: __state }
            }),
            None => {
                let bound = state_bound_clause(state_bound);
                quote!(&{
                    struct __SerdeStateWith<'state, State: ?Sized #bound> {
                        value: &'state #ty,
                        state: &'state State,
                    }

                    impl<'state, State: ?Sized #bound> _serde::Serialize
                        for __SerdeStateWith<'state, State>
                    {
                        fn serialize<__S>(&self, __serializer: __S) -> Result<__S::Ok, __S::Error>
                        where
                            __S: _serde::Serializer,
                        {
                            #with::serialize_state(self.value, self.state, __serializer)
                        }
                    }

                    __SerdeStateWith { value: #value, state: __state }
                })
            }
        }
    } else {
        match field.mode() {
            ItemMode::Stateful => quote!(&_serde_state::__private::wrap_serialize(#value, __state)),
            ItemMode::Stateless => quote!(#value),
        }
    }
}

fn serialize_enum_body(
    ident: &syn::Ident,
    data: &EnumDecl<'_>,
    explicit_state: Option<&Type>,
    state_bound: Option<&Type>,
) -> syn::Result<TokenStream> {
    let type_name = ident.to_string();
    let variants = data
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            serialize_enum_variant(
                variant,
                index as u32,
                &type_name,
                explicit_state,
                state_bound,
            )
        })
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(quote! {
        match self {
            #(#variants)*
        }
    })
}

fn serialize_enum_variant(
    variant: &VariantDecl<'_>,
    index: u32,
    type_name: &str,
    explicit_state: Option<&Type>,
    state_bound: Option<&Type>,
) -> syn::Result<TokenStream> {
    let variant_ident = variant.ident;
    let variant_name = variant_ident.to_string();
    let tokens = match variant.fields.style {
        FieldsStyle::Unit => {
            quote! {
                Self::#variant_ident => {
                    _serde::Serializer::serialize_unit_variant(
                        __serializer,
                        #type_name,
                        #index,
                        #variant_name,
                    )
                }
            }
        }
        FieldsStyle::Unnamed if variant.fields.fields.len() == 1 => {
            let binding = format_ident!("__variant_{}_field", index);
            let field = &variant.fields.fields[0];
            let call = serialize_field_expr(field, quote!(#binding), explicit_state, state_bound);
            quote! {
                Self::#variant_ident(ref #binding) => {
                    _serde::Serializer::serialize_newtype_variant(
                        __serializer,
                        #type_name,
                        #index,
                        #variant_name,
                        #call,
                    )
                }
            }
        }
        FieldsStyle::Unnamed => {
            let len = variant.fields.fields.len();
            let bindings: Vec<_> = (0..len)
                .map(|i| format_ident!("__variant_{}_field{}", index, i))
                .collect();
            let serialize_fields =
                bindings
                    .iter()
                    .zip(variant.fields.fields.iter())
                    .map(|(binding, field)| {
                        let call = serialize_field_expr(
                            field,
                            quote!(#binding),
                            explicit_state,
                            state_bound,
                        );
                        quote! {
                            _serde::ser::SerializeTupleVariant::serialize_field(
                                &mut __serde_state,
                                #call,
                            )?;
                        }
                    });
            quote! {
                Self::#variant_ident( #(ref #bindings),* ) => {
                    let mut __serde_state = _serde::Serializer::serialize_tuple_variant(
                        __serializer,
                        #type_name,
                        #index,
                        #variant_name,
                        #len,
                    )?;
                    #(#serialize_fields)*
                    _serde::ser::SerializeTupleVariant::end(__serde_state)
                }
            }
        }
        FieldsStyle::Named => {
            let field_idents: Vec<_> = variant
                .fields
                .fields
                .iter()
                .map(|field| field.ident().unwrap())
                .collect();
            let len = variant
                .fields
                .fields
                .iter()
                .filter(|field| !field.attrs.skip)
                .count();
            let serialize_fields = variant
                .fields
                .fields
                .iter()
                .filter(|field| !field.attrs.skip)
                .map(|field| {
                    let ident = field.ident().unwrap();
                    let name = field.attrs.key(ident);
                    let call =
                        serialize_field_expr(field, quote!(#ident), explicit_state, state_bound);
                    quote! {
                        _serde::ser::SerializeStructVariant::serialize_field(
                            &mut __serde_state,
                            #name,
                            #call,
                        )?;
                    }
                });
            quote! {
                Self::#variant_ident { #(ref #field_idents),* } => {
                    let mut __serde_state = _serde::Serializer::serialize_struct_variant(
                        __serializer,
                        #type_name,
                        #index,
                        #variant_name,
                        #len,
                    )?;
                    #(#serialize_fields)*
                    _serde::ser::SerializeStructVariant::end(__serde_state)
                }
            }
        }
    };

    Ok(tokens)
}

struct FieldType<'a> {
    ty: &'a syn::Type,
    mode: ItemMode,
}

impl<'a> FieldType<'a> {
    fn new(ty: &'a syn::Type, mode: ItemMode) -> Self {
        FieldType { ty, mode }
    }
}

fn collect_field_types_from_fields<'a>(fields: &'a FieldsDecl<'a>) -> Vec<FieldType<'a>> {
    let mut result = Vec::new();
    for field in &fields.fields {
        if field.attrs.skip {
            continue;
        }
        result.push(FieldType::new(field.ty(), field.mode()));
    }
    result
}

fn collect_field_types_from_enum<'a>(data: &'a EnumDecl<'a>) -> Vec<FieldType<'a>> {
    let mut result = Vec::new();
    for variant in &data.variants {
        result.extend(collect_field_types_from_fields(&variant.fields));
    }
    result
}

fn add_serialize_bounds_from_types(
    where_clause: &mut Option<syn::WhereClause>,
    field_types: &[FieldType<'_>],
    state_ty: &TokenStream,
) {
    if field_types.is_empty() {
        return;
    }

    let clause = where_clause.get_or_insert_with(|| syn::WhereClause {
        where_token: Default::default(),
        predicates: Default::default(),
    });

    for field in field_types {
        let ty = field.ty;
        match field.mode {
            ItemMode::Stateful => clause
                .predicates
                .push(parse_quote!(#ty: _serde_state::SerializeState<#state_ty>)),
            ItemMode::Stateless => clause.predicates.push(parse_quote!(#ty: _serde::Serialize)),
        }
    }
}

fn add_serialize_bounds_from_type_params(
    where_clause: &mut Option<syn::WhereClause>,
    generics: &Generics,
    state_ty: &TokenStream,
    mode: ItemMode,
) {
    let type_params: Vec<_> = generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect();
    if type_params.is_empty() {
        return;
    }

    let clause = where_clause.get_or_insert_with(|| syn::WhereClause {
        where_token: Default::default(),
        predicates: Default::default(),
    });

    for ident in type_params {
        match mode {
            ItemMode::Stateful => clause
                .predicates
                .push(parse_quote!(#ident: _serde_state::SerializeState<#state_ty>)),
            ItemMode::Stateless => clause
                .predicates
                .push(parse_quote!(#ident: _serde::Serialize)),
        }
    }
}

fn state_type_tokens(decl: &TypeDecl) -> TokenStream {
    if let Some(ty) = decl.attrs.state.as_ref() {
        quote!(#ty)
    } else {
        quote!(__State)
    }
}

fn add_state_param(
    generics: &Generics,
    include_state_param: bool,
    state_bound: Option<&Type>,
) -> Generics {
    let mut generics = generics.clone();
    if include_state_param {
        if let Some(bound) = state_bound {
            generics.params.push(parse_quote!(__State: ?Sized + #bound));
        } else {
            generics.params.push(parse_quote!(__State: ?Sized));
        }
    }
    generics
}

fn quote_where_clause(clause: &Option<syn::WhereClause>) -> TokenStream {
    match clause {
        Some(clause) => quote!(#clause),
        None => TokenStream::new(),
    }
}

fn default_serde_impl(decl: &TypeDecl, ident: &syn::Ident) -> Option<TokenStream> {
    let state_ty = decl.attrs.default_state.as_ref()?;
    let (impl_generics, ty_generics, _) = decl.generics.split_for_impl();
    let mut where_clause = decl.generics.where_clause.clone();
    add_default_state_bound(&mut where_clause, state_ty);
    let where_clause_tokens = quote_where_clause(&where_clause);
    Some(quote! {
        #[automatically_derived]
        impl #impl_generics _serde::Serialize for #ident #ty_generics #where_clause_tokens {
            fn serialize<__S>(&self, __serializer: __S) -> ::core::result::Result<__S::Ok, __S::Error>
            where
                __S: _serde::Serializer,
            {
                let __default_state = <#state_ty as ::core::default::Default>::default();
                _serde_state::SerializeState::serialize_state(
                    self,
                    &__default_state,
                    __serializer,
                )
            }
        }
    })
}

fn add_default_state_bound(where_clause: &mut Option<syn::WhereClause>, state_ty: &Type) {
    let clause = where_clause.get_or_insert_with(|| syn::WhereClause {
        where_token: Default::default(),
        predicates: Default::default(),
    });
    clause
        .predicates
        .push(parse_quote!(#state_ty: ::core::default::Default));
}
