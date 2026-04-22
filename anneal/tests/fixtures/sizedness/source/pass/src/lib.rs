pub struct NamedStruct {
    pub a: u8,
    pub b: u16,
}

/// ```lean, anneal
/// derive_sized sizedness_pass.NamedStruct
/// ```
pub fn foo(_f: &NamedStruct) {}

/// ```lean, anneal, spec
/// theorem spec : True := by trivial
/// ```
pub fn dummy_force_compile() {}
