pub struct NamedDst {
    pub a: u8,
    pub b: [u16],
}

/// ```lean, anneal
/// derive_sized sizedness_reject.NamedDst
/// ```
pub fn foo(_f: &NamedDst) {}

/// ```lean, anneal, spec
/// theorem spec : True := by trivial
/// ```
pub fn dummy_force_compile() {}
