use syn::{Attribute, Ident, LitStr, Path};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemMode {
    #[default]
    Stateful,
    Stateless,
}

#[derive(Clone)]
pub struct FieldAttrs {
    pub rename: Option<String>,
    pub skip: bool,
    pub mode: ItemMode,
    pub with: Option<Path>,
}

impl Default for FieldAttrs {
    fn default() -> Self {
        FieldAttrs {
            rename: None,
            skip: false,
            mode: ItemMode::Stateful,
            with: None,
        }
    }
}

impl FieldAttrs {
    pub fn key(&self, ident: &Ident) -> String {
        self.rename.clone().unwrap_or_else(|| ident.to_string())
    }
}

pub fn parse_field_attrs(attrs: &[Attribute], default_mode: ItemMode) -> syn::Result<FieldAttrs> {
    let mut result = FieldAttrs {
        mode: default_mode,
        ..FieldAttrs::default()
    };
    for attr in attrs {
        if attr.path().is_ident("serde") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    let value: LitStr = meta.value()?.parse()?;
                    result.rename = Some(value.value());
                    return Ok(());
                }
                if meta.path.is_ident("skip") {
                    result.skip = true;
                    return Ok(());
                }
                if meta.path.is_ident("with") {
                    let value: LitStr = meta.value()?.parse()?;
                    result.with = Some(value.parse()?);
                    return Ok(());
                }
                Err(meta.error("unsupported serde attribute"))
            })?;
        } else if attr.path().is_ident("serde_state") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("stateless") {
                    result.mode = ItemMode::Stateless;
                    return Ok(());
                }
                if meta.path.is_ident("stateful") {
                    result.mode = ItemMode::Stateful;
                    return Ok(());
                }
                Ok(())
            })?;
        }
    }
    Ok(result)
}

#[derive(Clone, Copy)]
pub struct VariantAttrs {
    pub mode: ItemMode,
}

impl VariantAttrs {
    pub fn mode(&self) -> ItemMode {
        self.mode
    }
}

impl Default for VariantAttrs {
    fn default() -> Self {
        VariantAttrs {
            mode: ItemMode::Stateful,
        }
    }
}

pub fn parse_variant_attrs(
    attrs: &[Attribute],
    default_mode: ItemMode,
) -> syn::Result<VariantAttrs> {
    let mut result = VariantAttrs { mode: default_mode };
    for attr in attrs {
        if attr.path().is_ident("serde_state") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("stateless") {
                    result.mode = ItemMode::Stateless;
                    return Ok(());
                }
                if meta.path.is_ident("stateful") {
                    result.mode = ItemMode::Stateful;
                    return Ok(());
                }
                Ok(())
            })?;
        }
    }
    Ok(result)
}
