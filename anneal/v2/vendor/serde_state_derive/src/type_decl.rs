use crate::attrs::{parse_field_attrs, parse_variant_attrs, FieldAttrs, ItemMode};
use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Attribute, Data, DataEnum, DataStruct, DeriveInput, Fields, Type};

pub struct TypeDecl<'a> {
    pub ident: &'a syn::Ident,
    pub generics: &'a syn::Generics,
    pub attrs: ContainerAttributes,
    pub data: TypeData<'a>,
}

pub enum TypeData<'a> {
    Struct(StructDecl<'a>),
    Enum(EnumDecl<'a>),
}

pub struct StructDecl<'a> {
    pub fields: FieldsDecl<'a>,
}

pub struct EnumDecl<'a> {
    pub variants: Vec<VariantDecl<'a>>,
}

pub struct VariantDecl<'a> {
    pub ident: &'a syn::Ident,
    pub fields: FieldsDecl<'a>,
}

pub struct FieldsDecl<'a> {
    pub style: FieldsStyle,
    pub fields: Vec<FieldDecl<'a>>,
    pub span: Span,
}

pub enum FieldsStyle {
    Named,
    Unnamed,
    Unit,
}

pub struct FieldDecl<'a> {
    pub field: &'a syn::Field,
    pub attrs: FieldAttrs,
}

impl<'a> TypeDecl<'a> {
    pub fn from_derive_input(input: &'a DeriveInput) -> syn::Result<Self> {
        let attrs = ContainerAttributes::from_attrs(&input.attrs)?;
        let data = match &input.data {
            Data::Struct(data) => TypeData::Struct(StructDecl::from_data(data, attrs.mode)?),
            Data::Enum(data) => TypeData::Enum(EnumDecl::from_data(data, attrs.mode)?),
            Data::Union(_) => unreachable!("unions are handled before TypeDecl construction"),
        };
        Ok(TypeDecl {
            ident: &input.ident,
            generics: &input.generics,
            attrs,
            data,
        })
    }
}

impl<'a> StructDecl<'a> {
    fn from_data(data: &'a DataStruct, mode: ItemMode) -> syn::Result<Self> {
        Ok(StructDecl {
            fields: FieldsDecl::from_fields(&data.fields, mode)?,
        })
    }
}

impl<'a> EnumDecl<'a> {
    fn from_data(data: &'a DataEnum, mode: ItemMode) -> syn::Result<Self> {
        let mut variants = Vec::new();
        for variant in &data.variants {
            let variant_mode = parse_variant_attrs(&variant.attrs, mode)?.mode();
            variants.push(VariantDecl {
                ident: &variant.ident,
                fields: FieldsDecl::from_fields(&variant.fields, variant_mode)?,
            });
        }
        Ok(EnumDecl { variants })
    }
}

impl<'a> FieldsDecl<'a> {
    fn from_fields(fields: &'a Fields, mode: ItemMode) -> syn::Result<Self> {
        let span = fields.span();
        match fields {
            Fields::Named(named) => {
                let mut result = Vec::with_capacity(named.named.len());
                for field in &named.named {
                    result.push(FieldDecl::new(field, mode)?);
                }
                Ok(FieldsDecl {
                    style: FieldsStyle::Named,
                    fields: result,
                    span,
                })
            }
            Fields::Unnamed(unnamed) => {
                let mut result = Vec::with_capacity(unnamed.unnamed.len());
                for field in &unnamed.unnamed {
                    result.push(FieldDecl::new(field, mode)?);
                }
                Ok(FieldsDecl {
                    style: FieldsStyle::Unnamed,
                    fields: result,
                    span,
                })
            }
            Fields::Unit => Ok(FieldsDecl {
                style: FieldsStyle::Unit,
                fields: Vec::new(),
                span,
            }),
        }
    }
}

impl<'a> FieldDecl<'a> {
    fn new(field: &'a syn::Field, default_mode: ItemMode) -> syn::Result<Self> {
        let attrs = parse_field_attrs(&field.attrs, default_mode)?;
        Ok(FieldDecl { field, attrs })
    }

    pub fn ty(&self) -> &'a Type {
        &self.field.ty
    }

    pub fn ident(&self) -> Option<&'a syn::Ident> {
        self.field.ident.as_ref()
    }

    pub fn mode(&self) -> ItemMode {
        self.attrs.mode
    }
}

pub struct ContainerAttributes {
    pub transparent: bool,
    pub serde_path: Option<syn::Path>,
    pub state: Option<Type>,
    pub state_bound: Option<Type>,
    pub default_state: Option<Type>,
    pub mode: ItemMode,
}

impl ContainerAttributes {
    fn from_attrs(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut result = ContainerAttributes {
            transparent: false,
            serde_path: None,
            state: None,
            state_bound: None,
            default_state: None,
            mode: ItemMode::Stateful,
        };

        for attr in attrs {
            let is_serde = attr.path().is_ident("serde");
            let is_serde_state = attr.path().is_ident("serde_state");
            if !(is_serde || is_serde_state) {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("transparent") {
                    result.transparent = true;
                    return Ok(());
                }
                if meta.path.is_ident("crate") {
                    let path = meta.value()?.parse()?;
                    result.serde_path = Some(path);
                    return Ok(());
                }
                if meta.path.is_ident("state") {
                    if !is_serde_state {
                        return Err(
                            meta.error("`state` must be specified with `serde_state(state = ..)`")
                        );
                    }
                    if result.state.is_some() {
                        return Err(meta.error("duplicate `state` attribute"));
                    }
                    if result.state_bound.is_some() {
                        return Err(meta.error(
                            "`state` cannot be combined with `state_implements`",
                        ));
                    }
                    let ty = meta.value()?.parse()?;
                    result.state = Some(ty);
                    return Ok(());
                }
                if meta.path.is_ident("state_implements") {
                    if !is_serde_state {
                        return Err(meta.error(
                            "`state_implements` must be specified with `serde_state(state_implements = ..)`",
                        ));
                    }
                    if result.state_bound.is_some() {
                        return Err(meta.error("duplicate `state_implements` attribute"));
                    }
                    if result.state.is_some() {
                        return Err(meta.error(
                            "`state_implements` cannot be combined with `state`",
                        ));
                    }
                    let ty = meta.value()?.parse()?;
                    result.state_bound = Some(ty);
                    return Ok(());
                }
                if meta.path.is_ident("default_state") {
                    if !is_serde_state {
                        return Err(meta.error(
                            "`default_state` must be specified with `serde_state(default_state = ..)`",
                        ));
                    }
                    if result.default_state.is_some() {
                        return Err(meta.error("duplicate `default_state` attribute"));
                    }
                    let ty = meta.value()?.parse()?;
                    result.default_state = Some(ty);
                    return Ok(());
                }
                if meta.path.is_ident("stateless") {
                    if !is_serde_state {
                        return Err(meta.error("`stateless` must be specified with `serde_state`"));
                    }
                    result.mode = ItemMode::Stateless;
                    return Ok(());
                }
                if meta.path.is_ident("stateful") {
                    if !is_serde_state {
                        return Err(meta.error("`stateful` must be specified with `serde_state`"));
                    }
                    result.mode = ItemMode::Stateful;
                    return Ok(());
                }
                if is_serde_state {
                    Err(meta.error("unsupported serde_state attribute"))
                } else {
                    Err(meta.error("unsupported serde attribute"))
                }
            })?;
        }

        Ok(result)
    }
}
